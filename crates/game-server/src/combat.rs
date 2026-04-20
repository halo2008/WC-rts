//! Strategic-layer combat — auto-resolve exchange on every contested hex.
//!
//! Runs once per strategic tick, after units have been advanced. For each hex
//! holding units from two or more nations we call `game_core::autoresolve`
//! pairwise between nations, aggregate the damage, persist HP changes and
//! delete anything that hit 0 HP. Groups that lose their last member are
//! disbanded.
//!
//! Detection is deliberately simple: any two nations co-located end up
//! fighting, no alliances or war declarations yet. That's Etap 6.

use crate::messages::{CombatPayload, GroupDisbandedPayload, UnitDeathPayload, WsMessage};
use crate::AppState;
use game_core::{autoresolve, CombatUnit, Hex, Terrain, UnitType};
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

/// Run one round of combat resolution. Safe to call every tick — when no
/// hexes are contested it's a single aggregate query and returns early.
pub async fn resolve_combats(state: &AppState) -> sqlx::Result<()> {
    let contested = sqlx::query_as::<_, (i32, i32)>(
        "SELECT hex_q, hex_r FROM units \
         WHERE hp > 0 \
         GROUP BY hex_q, hex_r \
         HAVING COUNT(DISTINCT nation_id) >= 2",
    )
    .fetch_all(&state.db)
    .await?;

    if contested.is_empty() {
        return Ok(());
    }

    for (hq, hr) in contested {
        resolve_hex(state, hq, hr).await?;
    }

    Ok(())
}

async fn resolve_hex(state: &AppState, hq: i32, hr: i32) -> sqlx::Result<()> {
    let rows = sqlx::query_as::<_, (i64, String, Uuid, i32, f32, Option<i64>)>(
        "SELECT id, unit_type, nation_id, hp, morale, group_id \
         FROM units WHERE hex_q = $1 AND hex_r = $2 AND hp > 0",
    )
    .bind(hq)
    .bind(hr)
    .fetch_all(&state.db)
    .await?;

    // Group live units by nation.
    #[derive(Clone)]
    struct UnitEntry {
        id: i64,
        ut: UnitType,
        hp: i32,
        morale: f32,
        group_id: Option<i64>,
    }
    let mut by_nation: HashMap<Uuid, Vec<UnitEntry>> = HashMap::new();
    for (id, ut_str, nid, hp, morale, gid) in rows {
        let Ok(ut) = UnitType::from_str(&ut_str) else {
            tracing::warn!("unit {} has invalid unit_type {}", id, ut_str);
            continue;
        };
        by_nation.entry(nid).or_default().push(UnitEntry {
            id,
            ut,
            hp,
            morale,
            group_id: gid,
        });
    }
    if by_nation.len() < 2 {
        return Ok(());
    }

    let terrain = state
        .terrain
        .get(&(hq, hr))
        .copied()
        .unwrap_or(Terrain::Plains);
    let hex = Hex::new(hq, hr);

    // Aggregate damage for every unit involved — each pair of nations runs
    // a separate autoresolve so damage compounds when 3+ nations collide.
    let mut damage_by_unit: HashMap<i64, i32> = HashMap::new();
    let nations: Vec<Uuid> = by_nation.keys().copied().collect();

    for i in 0..nations.len() {
        for j in (i + 1)..nations.len() {
            let side_a: Vec<CombatUnit> = by_nation[&nations[i]]
                .iter()
                .map(|u| CombatUnit {
                    id: u.id as u64,
                    unit_type: u.ut,
                    hp: u.hp,
                    morale: u.morale,
                })
                .collect();
            let side_b: Vec<CombatUnit> = by_nation[&nations[j]]
                .iter()
                .map(|u| CombatUnit {
                    id: u.id as u64,
                    unit_type: u.ut,
                    hp: u.hp,
                    morale: u.morale,
                })
                .collect();
            let outcome = autoresolve(hex, &side_a, &side_b, terrain);
            for loss in &outcome.side_a_losses {
                *damage_by_unit.entry(loss.unit_id as i64).or_insert(0) += loss.damage;
            }
            for loss in &outcome.side_b_losses {
                *damage_by_unit.entry(loss.unit_id as i64).or_insert(0) += loss.damage;
            }
        }
    }

    // Persist damage, then delete units that hit 0 HP.
    let mut dead_ids: Vec<i64> = Vec::new();
    // Track groups whose membership is touched so we can auto-disband
    // anything that went empty.
    let mut affected_groups: std::collections::HashSet<i64> =
        std::collections::HashSet::new();

    for nation_units in by_nation.values() {
        for u in nation_units {
            let Some(dmg) = damage_by_unit.get(&u.id).copied() else { continue };
            if dmg <= 0 {
                continue;
            }
            let new_hp = (u.hp - dmg).max(0);
            if new_hp == 0 {
                dead_ids.push(u.id);
                if let Some(gid) = u.group_id {
                    affected_groups.insert(gid);
                }
            } else {
                sqlx::query("UPDATE units SET hp = $2 WHERE id = $1")
                    .bind(u.id)
                    .bind(new_hp)
                    .execute(&state.db)
                    .await?;
            }
        }
    }

    if !dead_ids.is_empty() {
        sqlx::query("DELETE FROM units WHERE id = ANY($1)")
            .bind(&dead_ids)
            .execute(&state.db)
            .await?;
    }

    // Broadcast combat summary + per-unit deaths so the frontend can clean up.
    let _ = state.tx.send(WsMessage::Combat(CombatPayload {
        hex_q: hq,
        hex_r: hr,
        nations_involved: nations.clone(),
        total_dead: dead_ids.len() as i32,
    }));
    for uid in &dead_ids {
        let _ = state
            .tx
            .send(WsMessage::UnitDeath(UnitDeathPayload { unit_id: *uid }));
    }

    // Disband any group that just lost its last member.
    for gid in affected_groups {
        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)::BIGINT FROM units WHERE group_id = $1",
        )
        .bind(gid)
        .fetch_one(&state.db)
        .await?;
        if remaining == 0 {
            sqlx::query("DELETE FROM unit_groups WHERE id = $1")
                .bind(gid)
                .execute(&state.db)
                .await?;
            let _ = state
                .tx
                .send(WsMessage::GroupDisbanded(GroupDisbandedPayload { id: gid }));
        }
    }

    Ok(())
}
