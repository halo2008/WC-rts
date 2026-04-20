//! Types broadcast over the unified `/ws/strategic` channel. Each variant
//! carries a `type` discriminator so the JS side can `switch` on it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::tick::TickUpdate;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// Unit transit delta emitted every strategic tick.
    #[serde(rename = "transit")]
    Transit(TickUpdate),
    /// Full nation stockpile snapshot — emitted after each economy tick.
    #[serde(rename = "nation_state")]
    NationState(NationStatePayload),
    /// A unit was created (player action or production queue completion).
    #[serde(rename = "unit_spawn")]
    UnitSpawn(UnitSpawnPayload),
    /// A group was created, updated or had membership changed. The frontend
    /// re-fetches full group state on receive.
    #[serde(rename = "group_update")]
    GroupUpdate(GroupUpdatePayload),
    /// A group was disbanded — clients should drop it from UI state.
    #[serde(rename = "group_disbanded")]
    GroupDisbanded(GroupDisbandedPayload),
    /// Live group transit delta — one per tick per moving group.
    #[serde(rename = "group_transit")]
    GroupTransit(GroupTransitPayload),
    /// Auto-resolve combat happened on this hex; one message per contested hex.
    #[serde(rename = "combat")]
    Combat(CombatPayload),
    /// A unit was destroyed (HP ≤ 0). Clients drop it from their local state.
    #[serde(rename = "unit_death")]
    UnitDeath(UnitDeathPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NationStatePayload {
    pub nation_id: Uuid,
    pub fuel: f32,
    pub metals: f32,
    pub tech: f32,
    pub food: f32,
    pub war_support: f32,
    pub stability: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitSpawnPayload {
    pub id: i64,
    pub unit_type: String,
    pub nation_id: Uuid,
    pub hex_q: i32,
    pub hex_r: i32,
    pub hp: i32,
    pub max_hp: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUpdatePayload {
    pub id: i64,
    pub nation_id: Uuid,
    pub name: String,
    pub hex_q: i32,
    pub hex_r: i32,
    pub member_unit_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDisbandedPayload {
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupTransitPayload {
    pub group_id: i64,
    pub status: String,
    pub progress_hex: f32,
    pub from_q: i32,
    pub from_r: i32,
    pub to_q: i32,
    pub to_r: i32,
    pub fraction: f32,
    pub current_q: i32,
    pub current_r: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatPayload {
    pub hex_q: i32,
    pub hex_r: i32,
    pub nations_involved: Vec<Uuid>,
    pub total_dead: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitDeathPayload {
    pub unit_id: i64,
}
