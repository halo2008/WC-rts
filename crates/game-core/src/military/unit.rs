use crate::economy::Resource;
use crate::hex::Hex;
use crate::terrain::Terrain;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;

pub type UnitId = u64;

/// What layer of the world a unit operates in. Controls which hexes it can
/// traverse (ocean vs land vs any) and which transit animation plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum UnitDomain {
    Land,
    Sea,
    Air,
}

/// Six basic unit types. Each type has immutable stats (via `stats()`) used
/// for movement, supply and combat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum UnitType {
    Infantry,
    MechInfantry,
    Armor,
    Artillery,
    Ship,
    Aircraft,
}

/// Immutable combat/movement stats for a unit type. Kept as a plain struct so
/// server, WASM and tests all see identical numbers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct UnitStats {
    pub domain: UnitDomain,
    /// Base hex/second strategic speed on the unit's native terrain.
    pub base_speed: f32,
    pub max_hp: i32,
    pub attack: i32,
    pub defense: i32,
    /// Supply (food+fuel) consumed per strategic tick (1 Hz). Fuel is only
    /// consumed by mechanised/aerial units.
    pub supply_per_tick: f32,
    pub fuel_per_tick: f32,
    pub build_cost_metals: f32,
    pub build_cost_fuel: f32,
    pub build_time_seconds: f32,
}

impl UnitType {
    pub const ALL: [UnitType; 6] = [
        UnitType::Infantry,
        UnitType::MechInfantry,
        UnitType::Armor,
        UnitType::Artillery,
        UnitType::Ship,
        UnitType::Aircraft,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            UnitType::Infantry => "Infantry",
            UnitType::MechInfantry => "MechInfantry",
            UnitType::Armor => "Armor",
            UnitType::Artillery => "Artillery",
            UnitType::Ship => "Ship",
            UnitType::Aircraft => "Aircraft",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            UnitType::Infantry => "Piechota",
            UnitType::MechInfantry => "Piechota zmech.",
            UnitType::Armor => "Czołgi",
            UnitType::Artillery => "Artyleria",
            UnitType::Ship => "Flota",
            UnitType::Aircraft => "Lotnictwo",
        }
    }

    pub fn stats(self) -> UnitStats {
        match self {
            UnitType::Infantry => UnitStats {
                domain: UnitDomain::Land,
                base_speed: 0.25,
                max_hp: 1000,
                attack: 40,
                defense: 50,
                supply_per_tick: 1.0,
                fuel_per_tick: 0.0,
                build_cost_metals: 100.0,
                build_cost_fuel: 0.0,
                build_time_seconds: 60.0,
            },
            UnitType::MechInfantry => UnitStats {
                domain: UnitDomain::Land,
                base_speed: 0.45,
                max_hp: 1200,
                attack: 55,
                defense: 55,
                supply_per_tick: 1.2,
                fuel_per_tick: 0.8,
                build_cost_metals: 200.0,
                build_cost_fuel: 40.0,
                build_time_seconds: 100.0,
            },
            UnitType::Armor => UnitStats {
                domain: UnitDomain::Land,
                base_speed: 0.55,
                max_hp: 1500,
                attack: 90,
                defense: 70,
                supply_per_tick: 1.5,
                fuel_per_tick: 2.0,
                build_cost_metals: 400.0,
                build_cost_fuel: 80.0,
                build_time_seconds: 160.0,
            },
            UnitType::Artillery => UnitStats {
                domain: UnitDomain::Land,
                base_speed: 0.18,
                max_hp: 800,
                attack: 110,
                defense: 30,
                supply_per_tick: 1.3,
                fuel_per_tick: 0.6,
                build_cost_metals: 300.0,
                build_cost_fuel: 30.0,
                build_time_seconds: 120.0,
            },
            UnitType::Ship => UnitStats {
                domain: UnitDomain::Sea,
                base_speed: 0.7,
                max_hp: 2500,
                attack: 80,
                defense: 90,
                supply_per_tick: 2.0,
                fuel_per_tick: 3.0,
                build_cost_metals: 800.0,
                build_cost_fuel: 100.0,
                build_time_seconds: 300.0,
            },
            UnitType::Aircraft => UnitStats {
                domain: UnitDomain::Air,
                base_speed: 1.5,
                max_hp: 900,
                attack: 100,
                defense: 40,
                supply_per_tick: 1.5,
                fuel_per_tick: 4.0,
                build_cost_metals: 600.0,
                build_cost_fuel: 150.0,
                build_time_seconds: 220.0,
            },
        }
    }

    /// Primary resource consumed per tick — used by the economy layer to
    /// apply supply.
    pub fn primary_upkeep_resource(self) -> Resource {
        match self {
            UnitType::Ship | UnitType::Aircraft | UnitType::Armor => Resource::Fuel,
            _ => Resource::Food,
        }
    }

    /// Effective speed (hex/second) for this unit type entering the given
    /// terrain. Returns `None` when terrain is impassable for this domain.
    pub fn speed_on(self, terrain: Terrain) -> Option<f32> {
        let stats = self.stats();
        match stats.domain {
            UnitDomain::Air => Some(stats.base_speed), // ignores terrain
            UnitDomain::Sea => {
                if terrain.is_water() {
                    Some(stats.base_speed)
                } else {
                    None
                }
            }
            UnitDomain::Land => {
                if !terrain.is_passable_land() {
                    return None;
                }
                let cost = terrain.movement_cost();
                if !cost.is_finite() {
                    return None;
                }
                Some(stats.base_speed / cost)
            }
        }
    }
}

impl FromStr for UnitType {
    type Err = ParseUnitTypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        UnitType::ALL
            .iter()
            .copied()
            .find(|ut| ut.as_str() == s)
            .ok_or_else(|| ParseUnitTypeError(s.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct ParseUnitTypeError(pub String);

impl std::fmt::Display for ParseUnitTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown unit type: {}", self.0)
    }
}

impl std::error::Error for ParseUnitTypeError {}

/// A single unit instance on the strategic map. Individual units are cheap;
/// the UI groups them into stacks via `Stack::from_units`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Unit {
    pub id: UnitId,
    pub unit_type: UnitType,
    pub nation_id: String,
    pub hex: Hex,
    pub hp: i32,
    pub max_hp: i32,
    pub morale: f32,
    pub experience: f32,
}

impl Unit {
    pub fn new(id: UnitId, unit_type: UnitType, nation_id: String, hex: Hex) -> Self {
        let max_hp = unit_type.stats().max_hp;
        Self {
            id,
            unit_type,
            nation_id,
            hex,
            hp: max_hp,
            max_hp,
            morale: 1.0,
            experience: 0.0,
        }
    }

    pub fn hp_fraction(&self) -> f32 {
        if self.max_hp == 0 {
            0.0
        } else {
            self.hp as f32 / self.max_hp as f32
        }
    }

    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }
}

/// A view-only aggregation of co-located units. Not persisted — recomputed on
/// each strategic viewport query.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Stack {
    pub hex: Hex,
    pub nation_id: String,
    pub count: u32,
    pub total_hp: i32,
    pub total_max_hp: i32,
    pub average_morale: f32,
    pub dominant_type: UnitType,
}

impl Stack {
    /// Aggregate a slice of units that share hex + nation into a single stack.
    /// The caller must filter by hex/nation; this just sums.
    pub fn from_units(units: &[Unit]) -> Option<Self> {
        let first = units.first()?;
        let mut total_hp = 0;
        let mut total_max_hp = 0;
        let mut morale_sum = 0.0_f32;
        let mut counts: [u32; 6] = [0; 6];
        for u in units {
            total_hp += u.hp;
            total_max_hp += u.max_hp;
            morale_sum += u.morale;
            let idx = UnitType::ALL.iter().position(|t| *t == u.unit_type).unwrap_or(0);
            counts[idx] += 1;
        }
        let dominant_idx = counts
            .iter()
            .enumerate()
            .max_by_key(|(_, c)| **c)
            .map(|(i, _)| i)
            .unwrap_or(0);
        Some(Stack {
            hex: first.hex,
            nation_id: first.nation_id.clone(),
            count: units.len() as u32,
            total_hp,
            total_max_hp,
            average_morale: morale_sum / units.len() as f32,
            dominant_type: UnitType::ALL[dominant_idx],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_type_from_str_roundtrip() {
        for ut in UnitType::ALL {
            assert_eq!(UnitType::from_str(ut.as_str()).unwrap(), ut);
        }
        assert!(UnitType::from_str("Dragon").is_err());
    }

    #[test]
    fn infantry_cannot_walk_on_ocean() {
        assert!(UnitType::Infantry.speed_on(Terrain::Ocean).is_none());
        assert!(UnitType::Infantry.speed_on(Terrain::Plains).is_some());
    }

    #[test]
    fn ships_only_on_water() {
        assert!(UnitType::Ship.speed_on(Terrain::Ocean).is_some());
        assert!(UnitType::Ship.speed_on(Terrain::Plains).is_none());
    }

    #[test]
    fn aircraft_ignore_terrain() {
        assert!(UnitType::Aircraft.speed_on(Terrain::Mountain).is_some());
        assert!(UnitType::Aircraft.speed_on(Terrain::DeepOcean).is_some());
    }

    #[test]
    fn terrain_slows_land_units_proportionally() {
        let plains = UnitType::Armor.speed_on(Terrain::Plains).unwrap();
        let mountain = UnitType::Armor.speed_on(Terrain::Mountain).unwrap();
        assert!(mountain < plains);
    }

    #[test]
    fn stack_dominant_type_picks_majority() {
        let units = vec![
            Unit::new(1, UnitType::Infantry, "PL".into(), Hex::new(0, 0)),
            Unit::new(2, UnitType::Infantry, "PL".into(), Hex::new(0, 0)),
            Unit::new(3, UnitType::Armor, "PL".into(), Hex::new(0, 0)),
        ];
        let stack = Stack::from_units(&units).unwrap();
        assert_eq!(stack.count, 3);
        assert_eq!(stack.dominant_type, UnitType::Infantry);
    }
}
