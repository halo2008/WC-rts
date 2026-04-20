use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum Terrain {
    DeepOcean,
    Ocean,
    Coast,
    Plains,
    Forest,
    Hills,
    Mountain,
    Desert,
    Tundra,
    Urban,
    Ice,
}

impl Terrain {
    /// Canonical string name (matches Debug/Display/serde).
    pub fn as_str(self) -> &'static str {
        match self {
            Terrain::DeepOcean => "DeepOcean",
            Terrain::Ocean => "Ocean",
            Terrain::Coast => "Coast",
            Terrain::Plains => "Plains",
            Terrain::Forest => "Forest",
            Terrain::Hills => "Hills",
            Terrain::Mountain => "Mountain",
            Terrain::Desert => "Desert",
            Terrain::Tundra => "Tundra",
            Terrain::Urban => "Urban",
            Terrain::Ice => "Ice",
        }
    }
}

/// Parse a `Terrain` from its canonical name. Unknown strings are rejected.
impl FromStr for Terrain {
    type Err = ParseTerrainError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "DeepOcean" => Terrain::DeepOcean,
            "Ocean" => Terrain::Ocean,
            "Coast" => Terrain::Coast,
            "Plains" => Terrain::Plains,
            "Forest" => Terrain::Forest,
            "Hills" => Terrain::Hills,
            "Mountain" => Terrain::Mountain,
            "Desert" => Terrain::Desert,
            "Tundra" => Terrain::Tundra,
            "Urban" => Terrain::Urban,
            "Ice" => Terrain::Ice,
            _ => return Err(ParseTerrainError(s.to_string())),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ParseTerrainError(pub String);

impl std::fmt::Display for ParseTerrainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown terrain: {}", self.0)
    }
}

impl std::error::Error for ParseTerrainError {}

impl Terrain {
    /// CSS-like color for rendering (r, g, b 0-255).
    pub fn color_rgb(self) -> (u8, u8, u8) {
        match self {
            Terrain::DeepOcean => (20, 40, 100),
            Terrain::Ocean => (30, 60, 140),
            Terrain::Coast => (50, 100, 170),
            Terrain::Plains => (120, 180, 60),
            Terrain::Forest => (40, 120, 40),
            Terrain::Hills => (160, 140, 80),
            Terrain::Mountain => (130, 110, 90),
            Terrain::Desert => (210, 190, 120),
            Terrain::Tundra => (180, 200, 210),
            Terrain::Urban => (80, 80, 80),
            Terrain::Ice => (230, 240, 250),
        }
    }

    /// Movement cost multiplier (1.0 = normal).
    pub fn movement_cost(self) -> f32 {
        match self {
            Terrain::DeepOcean => f32::INFINITY,
            Terrain::Ocean => f32::INFINITY,
            Terrain::Coast => 1.5,
            Terrain::Plains => 1.0,
            Terrain::Forest => 1.5,
            Terrain::Hills => 2.0,
            Terrain::Mountain => 3.0,
            Terrain::Desert => 1.5,
            Terrain::Tundra => 2.0,
            Terrain::Urban => 1.2,
            Terrain::Ice => 2.5,
        }
    }

    pub fn is_water(self) -> bool {
        matches!(self, Terrain::DeepOcean | Terrain::Ocean | Terrain::Coast)
    }

    pub fn is_passable_land(self) -> bool {
        !matches!(self, Terrain::DeepOcean | Terrain::Ocean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_from_str_roundtrip() {
        for t in [
            Terrain::DeepOcean, Terrain::Ocean, Terrain::Coast, Terrain::Plains,
            Terrain::Forest, Terrain::Hills, Terrain::Mountain, Terrain::Desert,
            Terrain::Tundra, Terrain::Urban, Terrain::Ice,
        ] {
            assert_eq!(Terrain::from_str(t.as_str()).unwrap(), t);
        }
    }

    #[test]
    fn terrain_from_str_rejects_garbage() {
        assert!(Terrain::from_str("Not a terrain").is_err());
    }
}
