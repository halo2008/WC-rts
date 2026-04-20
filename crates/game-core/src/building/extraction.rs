use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;
use super::types::BuildingType;

/// Types of resource deposits found on battlefield hexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DepositType {
    Metals,
    Oil,
    Farmland,
    Timber,
}

impl DepositType {
    /// The extraction building type that can harvest this deposit.
    pub fn extraction_building(self) -> BuildingType {
        match self {
            DepositType::Metals => BuildingType::Mine,
            DepositType::Oil => BuildingType::OilWell,
            DepositType::Farmland => BuildingType::Farm,
            DepositType::Timber => BuildingType::LumberMill,
        }
    }

    /// Display label.
    pub fn label(self) -> &'static str {
        match self {
            DepositType::Metals => "Metals",
            DepositType::Oil => "Oil",
            DepositType::Farmland => "Farmland",
            DepositType::Timber => "Timber",
        }
    }

    /// Color for rendering on the map.
    pub fn color_rgb(self) -> (u8, u8, u8) {
        match self {
            DepositType::Metals => (180, 140, 60),
            DepositType::Oil => (50, 50, 50),
            DepositType::Farmland => (34, 139, 34),
            DepositType::Timber => (0, 100, 0),
        }
    }

    /// Canonical name (matches Debug/serde/DB CHECK constraint).
    pub fn as_str(self) -> &'static str {
        match self {
            DepositType::Metals => "Metals",
            DepositType::Oil => "Oil",
            DepositType::Farmland => "Farmland",
            DepositType::Timber => "Timber",
        }
    }
}

impl FromStr for DepositType {
    type Err = ParseDepositTypeError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Metals" => DepositType::Metals,
            "Oil" => DepositType::Oil,
            "Farmland" => DepositType::Farmland,
            "Timber" => DepositType::Timber,
            _ => return Err(ParseDepositTypeError(s.to_string())),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ParseDepositTypeError(pub String);

impl std::fmt::Display for ParseDepositTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown deposit type: {}", self.0)
    }
}

impl std::error::Error for ParseDepositTypeError {}

/// A resource deposit on a battlefield hex.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ResourceDeposit {
    pub deposit_type: DepositType,
    pub hex_q: i32,
    pub hex_r: i32,
    /// Richness 0.5..2.0 — multiplier on extraction rate.
    pub richness: f32,
    /// Remaining amount (depletes over time). None = infinite.
    pub remaining: Option<f32>,
}

impl ResourceDeposit {
    pub fn new(deposit_type: DepositType, hex_q: i32, hex_r: i32, richness: f32) -> Self {
        Self {
            deposit_type,
            hex_q,
            hex_r,
            richness,
            remaining: None, // infinite by default
        }
    }

    /// Extract resources. Returns amount extracted, and reduces remaining.
    pub fn extract(&mut self, rate: f32, dt: f32) -> f32 {
        let amount = rate * self.richness * dt;
        if let Some(ref mut remaining) = self.remaining {
            let extracted = amount.min(*remaining);
            *remaining -= extracted;
            extracted
        } else {
            amount
        }
    }
}

/// Extraction rate configuration per building type and level.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct ExtractionRate {
    /// Base extraction rate per second at level 1.
    pub base_rate: f32,
}

impl ExtractionRate {
    /// Get extraction rate for a building type at a given level.
    pub fn for_building(building_type: BuildingType, level: u8) -> Option<f32> {
        let base = match building_type {
            BuildingType::Mine => 2.0,
            BuildingType::OilWell => 3.0,
            BuildingType::Farm => 1.5,
            BuildingType::LumberMill => 1.0,
            _ => return None,
        };
        Some(base * level as f32)
    }
}

/// An extraction building placed on a deposit.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ExtractionBuilding {
    pub building_type: BuildingType,
    pub hex_q: i32,
    pub hex_r: i32,
    pub level: u8,
    pub nation_id: Option<String>,
    /// The deposit this building is extracting from.
    pub deposit_type: DepositType,
}

impl ExtractionBuilding {
    pub fn new(building_type: BuildingType, hex_q: i32, hex_r: i32, deposit_type: DepositType) -> Self {
        Self {
            building_type,
            hex_q,
            hex_r,
            level: 1,
            nation_id: None,
            deposit_type,
        }
    }

    /// Calculate extraction per second at current level.
    pub fn extraction_rate(&self) -> f32 {
        ExtractionRate::for_building(self.building_type, self.level).unwrap_or(0.0)
    }

    /// Tick extraction: returns (resource_type, amount) produced in `dt` seconds.
    pub fn tick_extraction(&self, deposit: &mut ResourceDeposit, dt: f32) -> (DepositType, f32) {
        let rate = self.extraction_rate();
        let amount = deposit.extract(rate, dt);
        (deposit.deposit_type, amount)
    }
}
