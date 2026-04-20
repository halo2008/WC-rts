use crate::hex::Hex;
use crate::military::unit::{Unit, UnitType};
use crate::terrain::Terrain;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub type GroupId = u64;

/// HoI-style army group: a named collection of units that operate together.
/// The group is persisted on the server; `Unit::group_id` points back here.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct UnitGroup {
    pub id: GroupId,
    pub nation_id: String,
    pub name: String,
    pub hex: Hex,
    /// IDs of member units. Denormalised (the source of truth is
    /// `units.group_id`) but convenient for client-side rendering.
    pub member_unit_ids: Vec<u64>,
}

impl UnitGroup {
    pub fn new(id: GroupId, nation_id: String, name: String, hex: Hex) -> Self {
        Self {
            id,
            nation_id,
            name,
            hex,
            member_unit_ids: Vec::new(),
        }
    }

    /// Effective group speed on a given terrain. Equal to the slowest member's
    /// speed — the whole group moves at the pace of its slowest unit. Returns
    /// `None` if any member can't traverse the terrain (blocks the group).
    pub fn speed_on(unit_types: &[UnitType], terrain: Terrain) -> Option<f32> {
        if unit_types.is_empty() {
            return None;
        }
        let mut slowest: Option<f32> = None;
        for ut in unit_types {
            let s = ut.speed_on(terrain)?;
            slowest = Some(match slowest {
                Some(cur) => cur.min(s),
                None => s,
            });
        }
        slowest
    }

    /// Aggregated HP fraction across all members — useful for UI health bars.
    pub fn hp_fraction(members: &[Unit]) -> f32 {
        let total: i32 = members.iter().map(|u| u.hp).sum();
        let max: i32 = members.iter().map(|u| u.max_hp).sum();
        if max == 0 {
            0.0
        } else {
            total as f32 / max as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_speed_is_slowest_member() {
        // Infantry (0.25) + Armor (0.55) → slowest = Infantry.
        let types = [UnitType::Infantry, UnitType::Armor];
        let s = UnitGroup::speed_on(&types, Terrain::Plains).unwrap();
        let expected = UnitType::Infantry.speed_on(Terrain::Plains).unwrap();
        assert!((s - expected).abs() < 1e-4);
    }

    #[test]
    fn group_blocked_if_any_member_cant_traverse() {
        // Infantry can't walk on ocean — whole group is blocked.
        let types = [UnitType::Infantry, UnitType::Ship];
        assert!(UnitGroup::speed_on(&types, Terrain::Ocean).is_none());
    }

    #[test]
    fn group_with_just_infantry_works_on_plains() {
        let types = [UnitType::Infantry, UnitType::Infantry];
        assert!(UnitGroup::speed_on(&types, Terrain::Plains).is_some());
    }

    #[test]
    fn empty_group_has_no_speed() {
        assert!(UnitGroup::speed_on(&[], Terrain::Plains).is_none());
    }
}
