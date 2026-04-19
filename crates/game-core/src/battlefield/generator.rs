use crate::hex::Hex;
use crate::terrain::Terrain;
use super::instance::{BattlefieldCell, BattlefieldInstance};

/// Configuration for battlefield generation.
#[derive(Debug, Clone)]
pub struct BattlefieldConfig {
    /// Width of the battlefield in hexes.
    pub width: i32,
    /// Height of the battlefield in hexes.
    pub height: i32,
    /// Random seed for deterministic generation.
    pub seed: u64,
    /// The strategic hex (q, r) this battlefield belongs to.
    pub strategic_q: i32,
    pub strategic_r: i32,
    /// Macro terrain from the strategic map — influences battlefield biome.
    pub macro_terrain: Terrain,
    /// Macro elevation from the strategic map.
    pub macro_elevation: i32,
}

impl Default for BattlefieldConfig {
    fn default() -> Self {
        Self {
            width: 64,
            height: 64,
            seed: 0,
            strategic_q: 0,
            strategic_r: 0,
            macro_terrain: Terrain::Plains,
            macro_elevation: 0,
        }
    }
}

/// Deterministic procedural battlefield generator.
/// Uses FBM noise + biome from macro hex to create terrain.
pub struct BattlefieldGenerator;

impl BattlefieldGenerator {
    /// Generate a battlefield instance from the given config.
    /// The generation is fully deterministic: same seed + config = same battlefield.
    pub fn generate(config: BattlefieldConfig) -> BattlefieldInstance {
        let mut cells = Vec::with_capacity((config.width * config.height) as usize);

        for r in 0..config.height {
            for q in 0..config.width {
                let cell = Self::generate_cell(q, r, &config);
                cells.push(cell);
            }
        }

        // Generate resource deposits
        let deposits = Self::generate_deposits(&config);

        BattlefieldInstance::new(
            config.strategic_q,
            config.strategic_r,
            config.width,
            config.height,
            config.seed,
            cells,
            deposits,
            config.macro_terrain,
        )
    }

    /// Generate a single battlefield cell.
    fn generate_cell(q: i32, r: i32, config: &BattlefieldConfig) -> BattlefieldCell {
        let terrain = Self::generate_terrain(q, r, config);
        let elevation = Self::generate_elevation(q, r, config);
        let forest_density = Self::generate_forest_density(q, r, config);
        let has_road = Self::generate_road(q, r, config);
        let has_river = Self::generate_river(q, r, config);

        BattlefieldCell {
            hex: Hex::new(q, r),
            terrain,
            elevation,
            forest_density,
            has_road,
            has_river,
            building_id: None,
            deposit: None,
        }
    }

    /// Generate terrain type for a battlefield hex based on noise + macro terrain.
    fn generate_terrain(q: i32, r: i32, config: &BattlefieldConfig) -> Terrain {
        let n1 = Self::fbm_noise(q as f64 * 0.08, r as f64 * 0.08, config.seed, 4);
        let n2 = Self::fbm_noise(q as f64 * 0.15 + 100.0, r as f64 * 0.15 + 100.0, config.seed + 1, 3);

        match config.macro_terrain {
            Terrain::DeepOcean | Terrain::Ocean => {
                // Ocean hex → mostly water with some islands
                if n1 > 0.6 {
                    Terrain::Coast
                } else if n1 > 0.75 {
                    Terrain::Plains
                } else {
                    Terrain::Ocean
                }
            }
            Terrain::Coast => {
                if n1 > 0.3 {
                    Terrain::Plains
                } else if n1 > 0.1 {
                    Terrain::Coast
                } else {
                    Terrain::Ocean
                }
            }
            Terrain::Plains => {
                if n1 > 0.6 {
                    Terrain::Forest
                } else if n1 > 0.4 {
                    Terrain::Hills
                } else if n2 > 0.7 {
                    Terrain::Urban
                } else {
                    Terrain::Plains
                }
            }
            Terrain::Forest => {
                if n1 > 0.2 {
                    Terrain::Forest
                } else {
                    Terrain::Plains
                }
            }
            Terrain::Hills => {
                if n1 > 0.5 {
                    Terrain::Mountain
                } else if n1 > 0.2 {
                    Terrain::Hills
                } else {
                    Terrain::Plains
                }
            }
            Terrain::Mountain => {
                if n1 > 0.3 {
                    Terrain::Mountain
                } else {
                    Terrain::Hills
                }
            }
            Terrain::Desert => {
                if n1 > 0.7 {
                    Terrain::Hills
                } else {
                    Terrain::Desert
                }
            }
            Terrain::Tundra => {
                if n1 > 0.6 {
                    Terrain::Ice
                } else if n1 > 0.3 {
                    Terrain::Mountain
                } else {
                    Terrain::Tundra
                }
            }
            Terrain::Urban => {
                if n2 > 0.5 {
                    Terrain::Urban
                } else {
                    Terrain::Plains
                }
            }
            Terrain::Ice => {
                if n1 > 0.3 {
                    Terrain::Ice
                } else {
                    Terrain::Tundra
                }
            }
        }
    }

    /// Generate elevation using noise.
    fn generate_elevation(q: i32, r: i32, config: &BattlefieldConfig) -> i32 {
        let n = Self::fbm_noise(q as f64 * 0.05, r as f64 * 0.05, config.seed + 42, 4);
        let base = config.macro_elevation as f64;
        ((n * 50.0) + base).round() as i32
    }

    /// Generate forest density (0.0 = no trees, 1.0 = dense forest).
    fn generate_forest_density(q: i32, r: i32, config: &BattlefieldConfig) -> f32 {
        let n = Self::fbm_noise(q as f64 * 0.1, r as f64 * 0.1, config.seed + 100, 3);
        if config.macro_terrain == Terrain::Forest {
            (n * 0.5 + 0.5).max(0.0).min(1.0) as f32
        } else {
            (n * 0.3).max(0.0).min(1.0) as f32
        }
    }

    /// Determine if this hex has a road.
    fn generate_road(q: i32, r: i32, config: &BattlefieldConfig) -> bool {
        // Simple road pattern: cross through center
        let cx = config.width / 2;
        let cy = config.height / 2;
        // Horizontal road through center
        let h_road = r == cy && q > cx - 10 && q < cx + 10;
        // Vertical road through center
        let v_road = q == cx && r > cy - 10 && r < cy + 10;
        h_road || v_road
    }

    /// Determine if this hex has a river.
    fn generate_river(q: i32, r: i32, config: &BattlefieldConfig) -> bool {
        // River: a winding path using noise
        let n = Self::fbm_noise(r as f64 * 0.1, 0.0, config.seed + 200, 2);
        let river_x = (config.width as f64 * 0.3 + n * config.width as f64 * 0.4).round() as i32;
        q >= river_x - 1 && q <= river_x + 1 && r > 5 && r < config.height - 5
    }

    /// Generate resource deposits for the battlefield.
    fn generate_deposits(config: &BattlefieldConfig) -> Vec<(i32, i32, crate::building::extraction::DepositType, f32)> {
        let mut deposits = Vec::new();
        let mut rng = SimpleRng::new(config.seed + 999);

        // Place 2-5 deposits based on macro terrain
        let count = match config.macro_terrain {
            Terrain::Mountain => 4,
            Terrain::Hills => 3,
            Terrain::Desert => 3,
            Terrain::Forest => 3,
            Terrain::Plains => 2,
            _ => 2,
        };

        for _ in 0..count {
            let dq = (rng.next() % (config.width as u64 - 10)) as i32 + 5;
            let dr = (rng.next() % (config.height as u64 - 10)) as i32 + 5;
            let richness = 0.5 + (rng.next() % 150) as f32 / 100.0;

            let deposit_type = match config.macro_terrain {
                Terrain::Mountain | Terrain::Hills => crate::building::extraction::DepositType::Metals,
                Terrain::Desert => crate::building::extraction::DepositType::Oil,
                Terrain::Forest => crate::building::extraction::DepositType::Timber,
                Terrain::Plains => crate::building::extraction::DepositType::Farmland,
                _ => crate::building::extraction::DepositType::Metals,
            };

            deposits.push((dq, dr, deposit_type, richness));
        }

        deposits
    }

    /// Fractional Brownian Motion noise — simple hash-based implementation.
    /// No external noise crate needed, deterministic.
    fn fbm_noise(x: f64, y: f64, seed: u64, octaves: u32) -> f64 {
        let mut value = 0.0;
        let mut amplitude = 1.0;
        let mut frequency = 1.0;
        let mut max_value = 0.0;

        for i in 0..octaves {
            value += amplitude * Self::value_noise(x * frequency, y * frequency, seed + i as u64);
            max_value += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }

        value / max_value
    }

    /// Simple 2D value noise using hash.
    fn value_noise(x: f64, y: f64, seed: u64) -> f64 {
        let ix = x.floor() as i64;
        let iy = y.floor() as i64;
        let fx = x - ix as f64;
        let fy = y - iy as f64;

        // Smoothstep
        let sx = fx * fx * (3.0 - 2.0 * fx);
        let sy = fy * fy * (3.0 - 2.0 * fy);

        let n00 = Self::hash2d(ix, iy, seed);
        let n10 = Self::hash2d(ix + 1, iy, seed);
        let n01 = Self::hash2d(ix, iy + 1, seed);
        let n11 = Self::hash2d(ix + 1, iy + 1, seed);

        let nx0 = n00 + sx * (n10 - n00);
        let nx1 = n01 + sx * (n11 - n01);

        nx0 + sy * (nx1 - nx0)
    }

    /// Hash function for 2D coordinates → [0, 1).
    fn hash2d(x: i64, y: i64, seed: u64) -> f64 {
        let mut h = seed;
        h = h.wrapping_add(x as u64);
        h = h.wrapping_add(y as u64);
        // Simple hash mixing
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51afd7ed558ccd);
        h ^= h >> 33;
        h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
        h ^= h >> 33;
        (h as f64) / (u64::MAX as f64)
    }
}

/// Simple deterministic RNG for deposit placement.
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        // xorshift64
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deterministic_generation() {
        let config = BattlefieldConfig {
            width: 32,
            height: 32,
            seed: 42,
            strategic_q: 100,
            strategic_r: 200,
            macro_terrain: Terrain::Plains,
            macro_elevation: 10,
        };

        let bf1 = BattlefieldGenerator::generate(config.clone());
        let bf2 = BattlefieldGenerator::generate(config);

        // Same seed = same terrain
        assert_eq!(bf1.cells.len(), bf2.cells.len());
        for (a, b) in bf1.cells.iter().zip(bf2.cells.iter()) {
            assert_eq!(a.terrain, b.terrain);
            assert_eq!(a.elevation, b.elevation);
        }
    }

    #[test]
    fn test_different_seeds_different_terrain() {
        let config1 = BattlefieldConfig {
            width: 16,
            height: 16,
            seed: 1,
            ..Default::default()
        };
        let config2 = BattlefieldConfig {
            width: 16,
            height: 16,
            seed: 2,
            ..Default::default()
        };

        let bf1 = BattlefieldGenerator::generate(config1);
        let bf2 = BattlefieldGenerator::generate(config2);

        // Different seeds should produce different terrain (with very high probability)
        let mut differences = 0;
        for (a, b) in bf1.cells.iter().zip(bf2.cells.iter()) {
            if a.terrain != b.terrain {
                differences += 1;
            }
        }
        assert!(differences > 0, "Different seeds should produce different terrain");
    }

    #[test]
    fn test_ocean_macro_terrain() {
        let config = BattlefieldConfig {
            width: 32,
            height: 32,
            seed: 42,
            macro_terrain: Terrain::Ocean,
            ..Default::default()
        };
        let bf = BattlefieldGenerator::generate(config);
        // Should have some water hexes
        let water_count = bf.cells.iter().filter(|c| c.terrain.is_water()).count();
        assert!(water_count > 0, "Ocean macro terrain should produce water hexes");
    }
}
