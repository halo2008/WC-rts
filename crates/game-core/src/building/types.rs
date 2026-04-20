use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;
use crate::hex::Hex;

/// Unique identifier for a building instance.
pub type BuildingId = u64;

/// All possible building types in the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum BuildingType {
    // ─── Infrastructure ───
    Headquarters,
    PowerPlant,
    SupplyDepot,
    Barracks,
    Factory,
    Airfield,
    Port,

    // ─── Defense ───
    Bunker,
    TrenchLine,
    Minefield,
    AABattery,
    AntiTankPosition,
    Wall,

    // ─── Resource extraction ───
    Mine,
    OilWell,
    Farm,
    LumberMill,

    // ─── Special ───
    RadarStation,
    CommsTower,
    ResearchLab,
    Hospital,
}

impl BuildingType {
    /// Base construction time in seconds.
    pub fn build_time(self) -> f32 {
        match self {
            BuildingType::Headquarters => 300.0,
            BuildingType::PowerPlant => 180.0,
            BuildingType::SupplyDepot => 120.0,
            BuildingType::Barracks => 150.0,
            BuildingType::Factory => 240.0,
            BuildingType::Airfield => 360.0,
            BuildingType::Port => 300.0,
            BuildingType::Bunker => 90.0,
            BuildingType::TrenchLine => 60.0,
            BuildingType::Minefield => 45.0,
            BuildingType::AABattery => 120.0,
            BuildingType::AntiTankPosition => 100.0,
            BuildingType::Wall => 80.0,
            BuildingType::Mine => 200.0,
            BuildingType::OilWell => 250.0,
            BuildingType::Farm => 150.0,
            BuildingType::LumberMill => 160.0,
            BuildingType::RadarStation => 180.0,
            BuildingType::CommsTower => 120.0,
            BuildingType::ResearchLab => 400.0,
            BuildingType::Hospital => 200.0,
        }
    }

    /// Maximum level this building can be upgraded to.
    pub fn max_level(self) -> u8 {
        match self {
            BuildingType::Headquarters => 5,
            BuildingType::PowerPlant => 3,
            BuildingType::SupplyDepot => 3,
            BuildingType::Barracks => 3,
            BuildingType::Factory => 3,
            BuildingType::Airfield => 3,
            BuildingType::Port => 3,
            BuildingType::Bunker => 3,
            BuildingType::TrenchLine => 2,
            BuildingType::Minefield => 1,
            BuildingType::AABattery => 3,
            BuildingType::AntiTankPosition => 2,
            BuildingType::Wall => 2,
            BuildingType::Mine => 3,
            BuildingType::OilWell => 3,
            BuildingType::Farm => 3,
            BuildingType::LumberMill => 3,
            BuildingType::RadarStation => 3,
            BuildingType::CommsTower => 2,
            BuildingType::ResearchLab => 3,
            BuildingType::Hospital => 3,
        }
    }

    /// Base HP at level 1.
    pub fn base_hp(self) -> i32 {
        match self {
            BuildingType::Headquarters => 5000,
            BuildingType::PowerPlant => 2000,
            BuildingType::SupplyDepot => 1500,
            BuildingType::Barracks => 2500,
            BuildingType::Factory => 2000,
            BuildingType::Airfield => 3000,
            BuildingType::Port => 2500,
            BuildingType::Bunker => 3000,
            BuildingType::TrenchLine => 1000,
            BuildingType::Minefield => 200,
            BuildingType::AABattery => 1500,
            BuildingType::AntiTankPosition => 1500,
            BuildingType::Wall => 2000,
            BuildingType::Mine => 1200,
            BuildingType::OilWell => 1000,
            BuildingType::Farm => 800,
            BuildingType::LumberMill => 1000,
            BuildingType::RadarStation => 1000,
            BuildingType::CommsTower => 800,
            BuildingType::ResearchLab => 1500,
            BuildingType::Hospital => 1200,
        }
    }

    /// Power consumption (negative) or production (positive).
    pub fn power_balance(self) -> i32 {
        match self {
            BuildingType::Headquarters => -5,
            BuildingType::PowerPlant => 20,
            BuildingType::SupplyDepot => -2,
            BuildingType::Barracks => -3,
            BuildingType::Factory => -8,
            BuildingType::Airfield => -10,
            BuildingType::Port => -5,
            BuildingType::Bunker => -1,
            BuildingType::TrenchLine => 0,
            BuildingType::Minefield => 0,
            BuildingType::AABattery => -2,
            BuildingType::AntiTankPosition => -1,
            BuildingType::Wall => 0,
            BuildingType::Mine => -4,
            BuildingType::OilWell => -5,
            BuildingType::Farm => -1,
            BuildingType::LumberMill => -2,
            BuildingType::RadarStation => -6,
            BuildingType::CommsTower => -3,
            BuildingType::ResearchLab => -10,
            BuildingType::Hospital => -4,
        }
    }

    /// Whether this building type is a fortification.
    pub fn is_fortification(self) -> bool {
        matches!(
            self,
            BuildingType::Bunker
                | BuildingType::TrenchLine
                | BuildingType::Minefield
                | BuildingType::AABattery
                | BuildingType::AntiTankPosition
                | BuildingType::Wall
        )
    }

    /// Whether this building type extracts resources.
    pub fn is_extraction(self) -> bool {
        matches!(
            self,
            BuildingType::Mine | BuildingType::OilWell | BuildingType::Farm | BuildingType::LumberMill
        )
    }

    /// Category label for UI grouping.
    pub fn category(self) -> BuildingCategory {
        match self {
            BuildingType::Headquarters
            | BuildingType::PowerPlant
            | BuildingType::SupplyDepot
            | BuildingType::Barracks
            | BuildingType::Factory
            | BuildingType::Airfield
            | BuildingType::Port => BuildingCategory::Infrastructure,

            BuildingType::Bunker
            | BuildingType::TrenchLine
            | BuildingType::Minefield
            | BuildingType::AABattery
            | BuildingType::AntiTankPosition
            | BuildingType::Wall => BuildingCategory::Defense,

            BuildingType::Mine
            | BuildingType::OilWell
            | BuildingType::Farm
            | BuildingType::LumberMill => BuildingCategory::Extraction,

            BuildingType::RadarStation
            | BuildingType::CommsTower
            | BuildingType::ResearchLab
            | BuildingType::Hospital => BuildingCategory::Special,
        }
    }

    /// CSS-like color for rendering (r, g, b 0-255).
    pub fn color_rgb(self) -> (u8, u8, u8) {
        match self {
            BuildingType::Headquarters => (255, 215, 0),
            BuildingType::PowerPlant => (255, 255, 100),
            BuildingType::SupplyDepot => (139, 119, 101),
            BuildingType::Barracks => (0, 100, 200),
            BuildingType::Factory => (150, 150, 150),
            BuildingType::Airfield => (100, 149, 237),
            BuildingType::Port => (0, 128, 128),
            BuildingType::Bunker => (101, 67, 33),
            BuildingType::TrenchLine => (139, 90, 43),
            BuildingType::Minefield => (255, 0, 0),
            BuildingType::AABattery => (128, 0, 128),
            BuildingType::AntiTankPosition => (165, 42, 42),
            BuildingType::Wall => (160, 160, 160),
            BuildingType::Mine => (180, 140, 60),
            BuildingType::OilWell => (50, 50, 50),
            BuildingType::Farm => (34, 139, 34),
            BuildingType::LumberMill => (0, 100, 0),
            BuildingType::RadarStation => (0, 191, 255),
            BuildingType::CommsTower => (192, 192, 192),
            BuildingType::ResearchLab => (148, 0, 211),
            BuildingType::Hospital => (255, 20, 147),
        }
    }

    /// Short label for display.
    pub fn label(self) -> &'static str {
        match self {
            BuildingType::Headquarters => "HQ",
            BuildingType::PowerPlant => "Power",
            BuildingType::SupplyDepot => "Supply",
            BuildingType::Barracks => "Barracks",
            BuildingType::Factory => "Factory",
            BuildingType::Airfield => "Airfield",
            BuildingType::Port => "Port",
            BuildingType::Bunker => "Bunker",
            BuildingType::TrenchLine => "Trench",
            BuildingType::Minefield => "Mines",
            BuildingType::AABattery => "AA",
            BuildingType::AntiTankPosition => "AT",
            BuildingType::Wall => "Wall",
            BuildingType::Mine => "Mine",
            BuildingType::OilWell => "Oil",
            BuildingType::Farm => "Farm",
            BuildingType::LumberMill => "Lumber",
            BuildingType::RadarStation => "Radar",
            BuildingType::CommsTower => "Comms",
            BuildingType::ResearchLab => "Lab",
            BuildingType::Hospital => "Hospital",
        }
    }

    /// Canonical enum-variant name (matches Debug/serde).
    pub fn as_str(self) -> &'static str {
        match self {
            BuildingType::Headquarters => "Headquarters",
            BuildingType::PowerPlant => "PowerPlant",
            BuildingType::SupplyDepot => "SupplyDepot",
            BuildingType::Barracks => "Barracks",
            BuildingType::Factory => "Factory",
            BuildingType::Airfield => "Airfield",
            BuildingType::Port => "Port",
            BuildingType::Bunker => "Bunker",
            BuildingType::TrenchLine => "TrenchLine",
            BuildingType::Minefield => "Minefield",
            BuildingType::AABattery => "AABattery",
            BuildingType::AntiTankPosition => "AntiTankPosition",
            BuildingType::Wall => "Wall",
            BuildingType::Mine => "Mine",
            BuildingType::OilWell => "OilWell",
            BuildingType::Farm => "Farm",
            BuildingType::LumberMill => "LumberMill",
            BuildingType::RadarStation => "RadarStation",
            BuildingType::CommsTower => "CommsTower",
            BuildingType::ResearchLab => "ResearchLab",
            BuildingType::Hospital => "Hospital",
        }
    }

    /// All variants, in a stable order suitable for UI listings.
    pub const ALL: [BuildingType; 21] = [
        BuildingType::Headquarters,
        BuildingType::PowerPlant,
        BuildingType::SupplyDepot,
        BuildingType::Barracks,
        BuildingType::Factory,
        BuildingType::Airfield,
        BuildingType::Port,
        BuildingType::Bunker,
        BuildingType::TrenchLine,
        BuildingType::Minefield,
        BuildingType::AABattery,
        BuildingType::AntiTankPosition,
        BuildingType::Wall,
        BuildingType::Mine,
        BuildingType::OilWell,
        BuildingType::Farm,
        BuildingType::LumberMill,
        BuildingType::RadarStation,
        BuildingType::CommsTower,
        BuildingType::ResearchLab,
        BuildingType::Hospital,
    ];
}

impl FromStr for BuildingType {
    type Err = ParseBuildingTypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BuildingType::ALL
            .iter()
            .copied()
            .find(|bt| bt.as_str() == s)
            .ok_or_else(|| ParseBuildingTypeError(s.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct ParseBuildingTypeError(pub String);

impl std::fmt::Display for ParseBuildingTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown building type: {}", self.0)
    }
}

impl std::error::Error for ParseBuildingTypeError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum BuildingCategory {
    Infrastructure,
    Defense,
    Extraction,
    Special,
}

/// A placed building instance on the battlefield.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Building {
    pub id: BuildingId,
    pub building_type: BuildingType,
    pub hex: Hex,
    pub level: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub nation_id: Option<String>,
    pub is_active: bool,
}

impl Building {
    /// Create a new building at level 1 with full HP.
    pub fn new(id: BuildingId, building_type: BuildingType, hex: Hex) -> Self {
        let max_hp = building_type.base_hp();
        Self {
            id,
            building_type,
            hex,
            level: 1,
            hp: max_hp,
            max_hp,
            nation_id: None,
            is_active: true,
        }
    }

    /// Upgrade building to next level. Returns false if already at max.
    pub fn upgrade(&mut self) -> bool {
        if self.level >= self.building_type.max_level() {
            return false;
        }
        self.level += 1;
        let old_max = self.max_hp;
        self.max_hp = self.building_type.base_hp() * (self.level as i32);
        // Heal by the HP difference
        self.hp += self.max_hp - old_max;
        true
    }

    /// Apply damage. Returns true if building is destroyed.
    pub fn take_damage(&mut self, damage: i32) -> bool {
        self.hp = (self.hp - damage).max(0);
        self.hp == 0
    }

    /// Repair building by given amount, capped at max_hp.
    pub fn repair(&mut self, amount: i32) {
        self.hp = (self.hp + amount).min(self.max_hp);
    }

    /// Effective power at current level.
    pub fn effective_power(&self) -> i32 {
        self.building_type.power_balance() * self.level as i32
    }

    /// HP fraction 0.0..1.0.
    pub fn hp_fraction(&self) -> f32 {
        if self.max_hp == 0 {
            0.0
        } else {
            self.hp as f32 / self.max_hp as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_type_from_str_roundtrip() {
        for bt in BuildingType::ALL {
            assert_eq!(BuildingType::from_str(bt.as_str()).unwrap(), bt);
        }
    }

    #[test]
    fn building_type_from_str_rejects_garbage() {
        assert!(BuildingType::from_str("NotABuilding").is_err());
    }
}
