//! Economy tick — runs every 5 minutes. Applies per-nation production
//! (scaled by the `economy` budget slider and tier) and unit upkeep
//! (food + fuel based on `UnitType::stats().supply_per_tick / fuel_per_tick`
//! accumulated over the tick duration).

use crate::messages::{NationStatePayload, WsMessage};
use crate::AppState;
use game_core::UnitType;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::time::{interval, Duration, MissedTickBehavior};
use uuid::Uuid;

/// Seconds between economy ticks. Mirrors the design-doc "5-min economy".
pub const ECONOMY_TICK_SECONDS: u64 = 300;

pub async fn economy_tick_loop(state: AppState) {
    let mut ticker = interval(Duration::from_secs(ECONOMY_TICK_SECONDS));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    // Burn the zero-delay first fire — we don't want an instant tick the
    // moment the server starts.
    ticker.tick().await;

    loop {
        ticker.tick().await;
        if let Err(e) = tick_once(&state).await {
            tracing::error!("economy tick failed: {}", e);
        }
    }
}

async fn tick_once(state: &AppState) -> sqlx::Result<()> {
    let tick_seconds = ECONOMY_TICK_SECONDS as f32;

    // ── Aggregate upkeep per nation ──────────────────────────────────────
    let unit_rows =
        sqlx::query_as::<_, (Uuid, String, i64)>(
            "SELECT nation_id, unit_type, COUNT(*) \
             FROM units GROUP BY nation_id, unit_type",
        )
        .fetch_all(&state.db)
        .await?;

    let mut upkeep: HashMap<Uuid, (f32, f32)> = HashMap::new(); // (food, fuel)
    for (nation_id, unit_type_str, count) in unit_rows {
        let Ok(ut) = UnitType::from_str(&unit_type_str) else {
            continue;
        };
        let stats = ut.stats();
        let entry = upkeep.entry(nation_id).or_insert((0.0, 0.0));
        entry.0 += stats.supply_per_tick * count as f32 * tick_seconds;
        entry.1 += stats.fuel_per_tick * count as f32 * tick_seconds;
    }

    // ── Apply production + upkeep to every nation_state row ──────────────
    let nations = sqlx::query_as::<_, (Uuid, i16, f32, f32, f32, f32, f32)>(
        "SELECT ns.nation_id, n.tier, ns.budget_economy, \
                ns.fuel, ns.metals, ns.tech, ns.food \
         FROM nation_state ns JOIN nations n ON n.id = ns.nation_id",
    )
    .fetch_all(&state.db)
    .await?;

    for (nation_id, tier, budget_econ, fuel, metals, tech, food) in nations {
        let (base_fuel, base_metals, base_tech, base_food) = base_production(tier);
        let econ_mul = budget_econ.clamp(0.0, 1.0);

        let prod_fuel = base_fuel * econ_mul;
        let prod_metals = base_metals * econ_mul;
        let prod_tech = base_tech * econ_mul;
        let prod_food = base_food * econ_mul;

        let (food_cost, fuel_cost) = upkeep.get(&nation_id).copied().unwrap_or((0.0, 0.0));

        let new_fuel = (fuel + prod_fuel - fuel_cost).max(0.0);
        let new_metals = (metals + prod_metals).max(0.0);
        let new_tech = (tech + prod_tech).max(0.0);
        let new_food = (food + prod_food - food_cost).max(0.0);

        // Basic morale / stability feedback — if we hit zero food or fuel,
        // stability drifts down; if supplies are comfortable, it drifts up.
        // Clamp to [0.0, 1.0].
        let deficit = (food + prod_food < food_cost) || (fuel + prod_fuel < fuel_cost);
        let stability_delta = if deficit { -0.03 } else { 0.01 };

        // Single round-trip: update, clamp stability, return the final pair
        // so the broadcast reflects what's in the DB.
        let snapshot = sqlx::query_as::<_, (f32, f32)>(
            "UPDATE nation_state SET \
                fuel = $2, metals = $3, tech = $4, food = $5, \
                stability = LEAST(1.0, GREATEST(0.0, stability + $6)), \
                updated_at = now() \
             WHERE nation_id = $1 \
             RETURNING war_support, stability",
        )
        .bind(nation_id)
        .bind(new_fuel)
        .bind(new_metals)
        .bind(new_tech)
        .bind(new_food)
        .bind(stability_delta)
        .fetch_one(&state.db)
        .await?;

        let _ = state.tx.send(WsMessage::NationState(NationStatePayload {
            nation_id,
            fuel: new_fuel,
            metals: new_metals,
            tech: new_tech,
            food: new_food,
            war_support: snapshot.0,
            stability: snapshot.1,
        }));
    }

    tracing::info!("economy tick complete ({} nations)", upkeep.len().max(1));
    Ok(())
}

/// Base production per 5-min tick at 100% economy budget. Tier-scaled; lower
/// tiers get smaller absolute numbers but the same shape. Values picked to
/// roughly cover Infantry + a couple of Armor stacks.
fn base_production(tier: i16) -> (f32, f32, f32, f32) {
    // Fuel, Metals, Tech, Food
    match tier {
        1 => (250.0, 350.0, 180.0, 500.0),
        2 => (160.0, 220.0, 110.0, 320.0),
        3 => (100.0, 140.0, 70.0, 200.0),
        _ => (60.0, 90.0, 45.0, 130.0),
    }
}
