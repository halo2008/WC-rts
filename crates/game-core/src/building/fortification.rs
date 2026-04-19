use serde::{Deserialize, Serialize};
use ts_rs::TS;
use super::types::BuildingType;

/// Types of fortifications that can be placed on the battlefield.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum FortificationType {
    Trench,
    Bunker,
    Minefield,
    AABattery,
    AntiTank,
}

impl FortificationType {
    /// Convert to the corresponding BuildingType.
    pub fn to_building_type(self) -> BuildingType {
        match self {
            FortificationType::Trench => BuildingType::TrenchLine,
            FortificationType::Bunker => BuildingType::Bunker,
            FortificationType::Minefield => BuildingType::Minefield,
            FortificationType::AABattery => BuildingType::AABattery,
            FortificationType::AntiTank => BuildingType::AntiTankPosition,
        }
    }

    /// Defense bonus description.
    pub fn description(self) -> &'static str {
        match self {
            FortificationType::Trench => "Infantry cover, reduces incoming damage by 30%",
            FortificationType::Bunker => "Heavy cover, reduces incoming damage by 60%",
            FortificationType::Minefield => "Damages units entering the hex, slows movement",
            FortificationType::AABattery => "Anti-air defense, engages aircraft in range",
            FortificationType::AntiTank => "Anti-vehicle emplacement, bonus vs armor",
        }
    }
}

/// Defense bonuses provided by fortifications.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct DefenseBonus {
    /// Damage reduction fraction (0.0 = none, 0.6 = 60% less damage).
    pub cover_bonus: f32,
    /// Extra damage dealt to attackers when defending.
    pub counter_attack: f32,
    /// Movement speed penalty for enemies entering this hex (multiplier, 1.0 = normal).
    pub movement_penalty: f32,
    /// Damage dealt to enemy units entering this hex (minefield).
    pub entry_damage: f32,
    /// Anti-air engagement range in hexes.
    pub aa_range: f32,
    /// Bonus damage vs armored units.
    pub anti_armor_bonus: f32,
}

impl DefenseBonus {
    /// No bonus.
    pub fn none() -> Self {
        Self {
            cover_bonus: 0.0,
            counter_attack: 0.0,
            movement_penalty: 1.0,
            entry_damage: 0.0,
            aa_range: 0.0,
            anti_armor_bonus: 0.0,
        }
    }

    /// Get the defense bonus for a given fortification type at a given level.
    pub fn for_fortification(ftype: FortificationType, level: u8) -> Self {
        let level_mult = level as f32;
        match ftype {
            FortificationType::Trench => Self {
                cover_bonus: 0.3 + 0.05 * level_mult,
                counter_attack: 0.0,
                movement_penalty: 0.8,
                entry_damage: 0.0,
                aa_range: 0.0,
                anti_armor_bonus: 0.0,
            },
            FortificationType::Bunker => Self {
                cover_bonus: 0.6 + 0.05 * level_mult,
                counter_attack: 0.1 * level_mult,
                movement_penalty: 0.9,
                entry_damage: 0.0,
                aa_range: 0.0,
                anti_armor_bonus: 0.0,
            },
            FortificationType::Minefield => Self {
                cover_bonus: 0.0,
                counter_attack: 0.0,
                movement_penalty: 0.3,
                entry_damage: 50.0 * level_mult,
                aa_range: 0.0,
                anti_armor_bonus: 0.0,
            },
            FortificationType::AABattery => Self {
                cover_bonus: 0.1,
                counter_attack: 0.0,
                movement_penalty: 1.0,
                entry_damage: 0.0,
                aa_range: 3.0 + level_mult,
                anti_armor_bonus: 0.0,
            },
            FortificationType::AntiTank => Self {
                cover_bonus: 0.2,
                counter_attack: 0.15 * level_mult,
                movement_penalty: 0.9,
                entry_damage: 0.0,
                aa_range: 0.0,
                anti_armor_bonus: 0.5 * level_mult,
            },
        }
    }
}

/// A placed fortification on the battlefield.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Fortification {
    pub fortification_type: FortificationType,
    pub hex_q: i32,
    pub hex_r: i32,
    pub level: u8,
    pub hp: i32,
    pub max_hp: i32,
    pub nation_id: Option<String>,
}

impl Fortification {
    pub fn new(ftype: FortificationType, hex_q: i32, hex_r: i32) -> Self {
        let bt = ftype.to_building_type();
        let max_hp = bt.base_hp();
        Self {
            fortification_type: ftype,
            hex_q,
            hex_r,
            level: 1,
            hp: max_hp,
            max_hp,
            nation_id: None,
        }
    }

    /// Get the defense bonus for this fortification.
    pub fn defense_bonus(&self) -> DefenseBonus {
        DefenseBonus::for_fortification(self.fortification_type, self.level)
    }

    /// Apply damage. Returns true if destroyed.
    pub fn take_damage(&mut self, damage: i32) -> bool {
        self.hp = (self.hp - damage).max(0);
        self.hp == 0
    }
}
