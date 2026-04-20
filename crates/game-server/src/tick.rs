//! Strategic tick loop — runs every second, advances every active transit by
//! one in-game second, commits new positions, and also processes the unit
//! production queue. Deltas are broadcast on the shared WebSocket channel.

use crate::messages::{GroupTransitPayload, UnitSpawnPayload, WsMessage};
use crate::AppState;
use game_core::{Hex, Terrain, Transit, TransitStatus, UnitGroup, UnitType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::{interval, Duration, MissedTickBehavior};
use uuid::Uuid;

/// Terrain lookup by axial (q, r). Populated once from the `hex_map` table at
/// startup. Hexes not in the table fall back to `Plains`.
pub type TerrainCache = Arc<HashMap<(i32, i32), Terrain>>;

/// A single transit delta broadcast to strategic-map listeners.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickUpdate {
    pub unit_id: i64,
    pub status: String,
    pub progress_hex: f32,
    pub from_q: i32,
    pub from_r: i32,
    pub to_q: i32,
    pub to_r: i32,
    /// 0.0 = at `from_*`, 1.0 = at `to_*`.
    pub fraction: f32,
    /// Authoritative current hex (rounded). Clients can snap sprites here if
    /// they don't interpolate.
    pub current_q: i32,
    pub current_r: i32,
}

/// Populate the terrain cache from the `hex_map` table. Cheap at 720k rows;
/// ~25 MB in RAM and we read it read-only forever.
pub async fn load_terrain(db: &sqlx::PgPool) -> sqlx::Result<TerrainCache> {
    let rows = sqlx::query_as::<_, (i32, i32, String)>("SELECT q, r, terrain FROM hex_map")
        .fetch_all(db)
        .await?;

    let mut map = HashMap::with_capacity(rows.len());
    for (q, r, t) in rows {
        if let Ok(terrain) = Terrain::from_str(&t) {
            map.insert((q, r), terrain);
        }
    }

    tracing::info!("Loaded terrain cache: {} cells", map.len());
    Ok(Arc::new(map))
}

/// Fire-and-forget background task. Cancels with the tokio runtime.
pub async fn strategic_tick_loop(state: AppState) {
    let mut ticker = interval(Duration::from_secs(1));
    // If the previous tick takes longer than 1s we skip missed beats rather
    // than pile up runs.
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        if let Err(e) = tick_once(&state).await {
            tracing::error!("strategic tick failed: {}", e);
        }
    }
}

async fn tick_once(state: &AppState) -> sqlx::Result<()> {
    advance_transits(state).await?;
    advance_group_transits(state).await?;
    // Combat resolves AFTER movement — units that just arrived at a contested
    // hex get to trade blows this tick.
    if let Err(e) = crate::combat::resolve_combats(state).await {
        tracing::error!("combat resolution failed: {}", e);
    }
    advance_production(state).await?;
    Ok(())
}

/// Advance every active group transit by 1 in-game second. A group moves as
/// one: all member units are repositioned to the group's new hex, and the
/// transit progresses at the slowest member's terrain-adjusted speed.
async fn advance_group_transits(state: &AppState) -> sqlx::Result<()> {
    let rows = sqlx::query_as::<_, (i64, serde_json::Value, f32, f32)>(
        "SELECT group_id, path, progress_hex, base_speed \
         FROM group_transits \
         WHERE status IN ('Active', 'Blocked')",
    )
    .fetch_all(&state.db)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    for (group_id, path_json, progress_hex, base_speed) in rows {
        let Ok(points) = serde_json::from_value::<Vec<HexPoint>>(path_json) else {
            tracing::warn!("group {} transit path invalid json", group_id);
            continue;
        };
        let path: Vec<Hex> = points.iter().map(|p| Hex::new(p.q, p.r)).collect();

        // Look up the slowest member so group advance respects HoI-style
        // "whole group moves as slowest unit".
        let member_types = sqlx::query_as::<_, (String,)>(
            "SELECT unit_type FROM units WHERE group_id = $1",
        )
        .bind(group_id)
        .fetch_all(&state.db)
        .await?;

        if member_types.is_empty() {
            // Empty group — cancel its transit so we don't spin forever.
            sqlx::query(
                "UPDATE group_transits SET status = 'Cancelled', updated_at = now() \
                 WHERE group_id = $1",
            )
            .bind(group_id)
            .execute(&state.db)
            .await?;
            continue;
        }

        let types: Vec<UnitType> = member_types
            .iter()
            .filter_map(|(s,)| UnitType::from_str(s).ok())
            .collect();

        // We reuse `Transit::tick` by synthesising a fake unit_type whose
        // `speed_on(terrain)` returns the group's slowest speed. Simpler: run
        // the math directly here.
        let mut transit = Transit {
            unit_id: group_id as u64,
            path: path.clone(),
            progress_hex,
            status: TransitStatus::Active,
            base_speed,
        };

        let cache = state.terrain.clone();
        let terrain_at = |h: Hex| cache.get(&(h.q, h.r)).copied().unwrap_or(Terrain::Plains);

        // Advance using the group's slowest speed on the current tile.
        tick_group_transit(&mut transit, 1.0, &types, terrain_at);

        let status_str = status_to_str(transit.status);
        let current = transit.current_hex();

        sqlx::query(
            "UPDATE group_transits SET progress_hex = $2, status = $3, updated_at = now() \
             WHERE group_id = $1",
        )
        .bind(group_id)
        .bind(transit.progress_hex)
        .bind(status_str)
        .execute(&state.db)
        .await?;

        // Snap every member to the group's authoritative hex.
        sqlx::query(
            "UPDATE units SET hex_q = $2, hex_r = $3 WHERE group_id = $1",
        )
        .bind(group_id)
        .bind(current.q)
        .bind(current.r)
        .execute(&state.db)
        .await?;

        // Keep the group's own hex in sync too so list_groups reflects reality.
        sqlx::query(
            "UPDATE unit_groups SET hex_q = $2, hex_r = $3 WHERE id = $1",
        )
        .bind(group_id)
        .bind(current.q)
        .bind(current.r)
        .execute(&state.db)
        .await?;

        let (from, to, frac) = transit.interpolated_position();
        let _ = state.tx.send(WsMessage::GroupTransit(GroupTransitPayload {
            group_id,
            status: status_str.to_string(),
            progress_hex: transit.progress_hex,
            from_q: from.q,
            from_r: from.r,
            to_q: to.q,
            to_r: to.r,
            fraction: frac,
            current_q: current.q,
            current_r: current.r,
        }));
    }

    Ok(())
}

/// Advance a group transit by `dt` seconds, using the slowest member's speed
/// on each traversed leg. Mirrors `Transit::tick` but for groups.
fn tick_group_transit(
    transit: &mut Transit,
    dt: f32,
    member_types: &[UnitType],
    terrain_at: impl Fn(Hex) -> Terrain,
) {
    if transit.path.is_empty() || matches!(transit.status, TransitStatus::Completed | TransitStatus::Cancelled) {
        return;
    }
    let last_idx = (transit.path.len() as f32) - 1.0;
    if transit.progress_hex >= last_idx {
        transit.status = TransitStatus::Completed;
        return;
    }

    let mut remaining = dt;
    while remaining > 0.0 && transit.progress_hex < last_idx {
        let idx = transit.progress_hex.floor() as usize;
        let next_hex = transit.path.get(idx + 1).copied().unwrap_or_else(|| transit.path[idx]);
        let terrain = terrain_at(next_hex);
        let speed = match UnitGroup::speed_on(member_types, terrain) {
            Some(s) => s,
            None => {
                transit.status = TransitStatus::Blocked;
                return;
            }
        };
        let leg_remaining = (idx as f32 + 1.0) - transit.progress_hex;
        let time_to_finish_leg = leg_remaining / speed;
        if time_to_finish_leg <= remaining {
            transit.progress_hex = idx as f32 + 1.0;
            remaining -= time_to_finish_leg;
        } else {
            transit.progress_hex += speed * remaining;
            remaining = 0.0;
        }
    }

    if transit.progress_hex >= last_idx {
        transit.progress_hex = last_idx;
        transit.status = TransitStatus::Completed;
    } else if !matches!(transit.status, TransitStatus::Blocked) {
        transit.status = TransitStatus::Active;
    }
}

async fn advance_transits(state: &AppState) -> sqlx::Result<()> {
    // Pull every active/blocked transit plus the unit it belongs to.
    let rows = sqlx::query_as::<_, (i64, String, serde_json::Value, f32, f32)>(
        "SELECT u.id, u.unit_type, t.path, t.progress_hex, t.base_speed \
         FROM unit_transits t \
         JOIN units u ON u.id = t.unit_id \
         WHERE t.status IN ('Active', 'Blocked')",
    )
    .fetch_all(&state.db)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    for (unit_id, unit_type_str, path_json, progress_hex, base_speed) in rows {
        let Ok(unit_type) = UnitType::from_str(&unit_type_str) else {
            tracing::warn!("unit {} has invalid unit_type {}", unit_id, unit_type_str);
            continue;
        };

        let Ok(points) = serde_json::from_value::<Vec<HexPoint>>(path_json) else {
            tracing::warn!("unit {} transit path is not a valid hex array", unit_id);
            continue;
        };
        let path: Vec<Hex> = points.iter().map(|p| Hex::new(p.q, p.r)).collect();

        let mut transit = Transit {
            unit_id: unit_id as u64,
            path,
            progress_hex,
            status: TransitStatus::Active,
            base_speed,
        };

        let cache = state.terrain.clone();
        let terrain_at = move |h: Hex| -> Terrain {
            cache.get(&(h.q, h.r)).copied().unwrap_or(Terrain::Plains)
        };

        transit.tick(1.0, unit_type, terrain_at);

        let status_str = status_to_str(transit.status);
        let current = transit.current_hex();

        // Persist new transit state.
        sqlx::query(
            "UPDATE unit_transits SET progress_hex = $2, status = $3, updated_at = now() \
             WHERE unit_id = $1",
        )
        .bind(unit_id)
        .bind(transit.progress_hex)
        .bind(status_str)
        .execute(&state.db)
        .await?;

        // Advance the unit's authoritative position.
        sqlx::query("UPDATE units SET hex_q = $2, hex_r = $3 WHERE id = $1")
            .bind(unit_id)
            .bind(current.q)
            .bind(current.r)
            .execute(&state.db)
            .await?;

        // Broadcast — `send` only fails when no receivers exist, which is fine.
        let (from, to, frac) = transit.interpolated_position();
        let _ = state.tx.send(WsMessage::Transit(TickUpdate {
            unit_id,
            status: status_str.to_string(),
            progress_hex: transit.progress_hex,
            from_q: from.q,
            from_r: from.r,
            to_q: to.q,
            to_r: to.r,
            fraction: frac,
            current_q: current.q,
            current_r: current.r,
        }));
    }

    Ok(())
}

/// Advance every in-progress production order by 1 second of build time.
/// Completed orders: insert a new unit at the spawn hex, mark the order
/// Completed, and broadcast a UnitSpawn event.
async fn advance_production(state: &AppState) -> sqlx::Result<()> {
    // Lock in candidates first — progress them atomically per row.
    let orders = sqlx::query_as::<_, (i64, Uuid, String, i32, i32, f32, f32)>(
        "SELECT id, nation_id, unit_type, spawn_hex_q, spawn_hex_r, progress, total_time \
         FROM production_queue WHERE status = 'InProgress'",
    )
    .fetch_all(&state.db)
    .await?;

    for (order_id, nation_id, unit_type_str, spawn_q, spawn_r, progress, total_time) in orders {
        let next = progress + 1.0;
        if next >= total_time {
            // Complete — create the unit + mark the order done.
            let Ok(unit_type) = UnitType::from_str(&unit_type_str) else {
                tracing::warn!(
                    "production order {} has invalid unit_type {}",
                    order_id,
                    unit_type_str
                );
                continue;
            };
            let max_hp = unit_type.stats().max_hp;

            let unit_id = sqlx::query_scalar::<_, i64>(
                "INSERT INTO units (unit_type, nation_id, hex_q, hex_r, hp, max_hp) \
                 VALUES ($1, $2, $3, $4, $5, $5) RETURNING id",
            )
            .bind(unit_type.as_str())
            .bind(nation_id)
            .bind(spawn_q)
            .bind(spawn_r)
            .bind(max_hp)
            .fetch_one(&state.db)
            .await?;

            sqlx::query(
                "UPDATE production_queue SET status = 'Completed', progress = total_time \
                 WHERE id = $1",
            )
            .bind(order_id)
            .execute(&state.db)
            .await?;

            let _ = state.tx.send(WsMessage::UnitSpawn(UnitSpawnPayload {
                id: unit_id,
                unit_type: unit_type.as_str().to_string(),
                nation_id,
                hex_q: spawn_q,
                hex_r: spawn_r,
                hp: max_hp,
                max_hp,
            }));
        } else {
            sqlx::query("UPDATE production_queue SET progress = $2 WHERE id = $1")
                .bind(order_id)
                .bind(next)
                .execute(&state.db)
                .await?;
        }
    }

    Ok(())
}

fn status_to_str(s: TransitStatus) -> &'static str {
    match s {
        TransitStatus::Active => "Active",
        TransitStatus::Blocked => "Blocked",
        TransitStatus::Completed => "Completed",
        TransitStatus::Cancelled => "Cancelled",
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct HexPoint {
    q: i32,
    r: i32,
}
