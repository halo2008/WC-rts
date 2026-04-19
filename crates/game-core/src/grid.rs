use crate::hex::{hex_to_pixel, pixel_to_hex, Hex};
use crate::terrain::Terrain;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct HexCell {
    pub hex: Hex,
    pub terrain: Terrain,
    pub elevation: i32,
}

/// Strategic world map grid (equirectangular, 1200x600).
pub struct StrategicGrid {
    pub width: i32,
    pub height: i32,
    cells: Vec<HexCell>,
}

impl StrategicGrid {
    /// Access the underlying cells slice.
    pub fn cells(&self) -> &[HexCell] {
        &self.cells
    }

    pub fn new(width: i32, height: i32) -> Self {
        let mut cells = Vec::with_capacity((width * height) as usize);
        for r in 0..height {
            for q in 0..width {
                let terrain = generate_placeholder_terrain(q, r, width, height);
                let elevation = placeholder_elevation(q, r, width, height);
                cells.push(HexCell {
                    hex: Hex::new(q, r),
                    terrain,
                    elevation,
                });
            }
        }
        Self {
            width,
            height,
            cells,
        }
    }

    pub fn get(&self, q: i32, r: i32) -> Option<&HexCell> {
        if r < 0 || r >= self.height {
            return None;
        }
        let wq = ((q % self.width) + self.width) % self.width;
        let idx = (r * self.width + wq) as usize;
        self.cells.get(idx)
    }

    /// Zwraca HexCell na podanej pozycji pikselowej (z wrap-around na osi X).
    /// px, py — pozycja pikselowa na mapie, hex_size — rozmiar hexa
    pub fn get_hex_at_pixel(&self, px: f32, py: f32, hex_size: f32) -> Option<&HexCell> {
        let hex = pixel_to_hex(px, py, hex_size);
        let wrapped = hex.wrap(self.width);
        // Upewnij się że r jest w zakresie
        if wrapped.r < 0 || wrapped.r >= self.height {
            return None;
        }
        self.get(wrapped.q, wrapped.r)
    }

    /// Return hexes visible in a viewport defined by pixel bounds.
    pub fn viewport(
        &self,
        cam_x: f32,
        cam_y: f32,
        view_width: f32,
        view_height: f32,
        hex_size: f32,
    ) -> Vec<&HexCell> {
        let sq3 = 3.0_f32.sqrt();
        let hex_w = sq3 * hex_size;
        let hex_h = 2.0 * hex_size;

        let min_q_col = ((cam_x - view_width / 2.0) / hex_w).floor() as i32 - 1;
        let max_q_col = ((cam_x + view_width / 2.0) / hex_w).ceil() as i32 + 1;
        let min_r_row = ((cam_y - view_height / 2.0) / hex_h).floor() as i32 - 1;
        let max_r_row = ((cam_y + view_height / 2.0) / hex_h).ceil() as i32 + 1;

        let mut result = Vec::new();
        for r in min_r_row.max(0)..=max_r_row.min(self.height - 1) {
            for q_col in min_q_col..=max_q_col {
                let q = ((q_col % self.width) + self.width) % self.width;
                if let Some(cell) = self.get(q, r) {
                    let (px, py) = hex_to_pixel(cell.hex, hex_size);
                    if px >= cam_x - view_width / 2.0 - hex_w
                        && px <= cam_x + view_width / 2.0 + hex_w
                        && py >= cam_y - view_height / 2.0 - hex_h
                        && py <= cam_y + view_height / 2.0 + hex_h
                    {
                        result.push(cell);
                    }
                }
            }
        }
        result
    }
}

/// Placeholder terrain generation — latitude + simple noise approximation.
/// Produces a rough globe: oceans, poles, bands of terrain.
fn generate_placeholder_terrain(q: i32, r: i32, width: i32, height: i32) -> Terrain {
    let lat = r as f32 / height as f32; // 0 = north pole, 1 = south pole
    let _lon = q as f32 / width as f32;

    // Pseudo-noise based on position (deterministic, no rand)
    let noise = simple_hash(q, r) as f32 / u32::MAX as f32;

    // Polar regions
    if lat < 0.08 || lat > 0.92 {
        return if noise < 0.3 { Terrain::Ice } else { Terrain::Tundra };
    }
    if lat < 0.13 || lat > 0.87 {
        return if noise < 0.4 {
            Terrain::Tundra
        } else if noise < 0.7 {
            Terrain::Forest
        } else {
            Terrain::Ocean
        };
    }

    // Subtropical desert bands
    if (lat > 0.22 && lat < 0.32) || (lat > 0.68 && lat < 0.78) {
        if noise < 0.35 {
            return Terrain::Desert;
        }
        if noise < 0.55 {
            return Terrain::Plains;
        }
    }

    // Temperate zone
    let land_chance = 0.45 + 0.15 * (simple_hash(q + 1000, r) as f32 / u32::MAX as f32);
    if noise < land_chance {
        let sub = simple_hash(q + 500, r + 500) as f32 / u32::MAX as f32;
        if sub < 0.35 {
            return Terrain::Plains;
        }
        if sub < 0.55 {
            return Terrain::Forest;
        }
        if sub < 0.70 {
            return Terrain::Hills;
        }
        if sub < 0.80 {
            return Terrain::Mountain;
        }
        if sub < 0.88 {
            return Terrain::Urban;
        }
        Terrain::Coast
    } else {
        let deep = simple_hash(q + 2000, r + 2000) as f32 / u32::MAX as f32;
        if deep < 0.6 {
            Terrain::Ocean
        } else {
            Terrain::DeepOcean
        }
    }
}

fn placeholder_elevation(q: i32, r: i32, _width: i32, height: i32) -> i32 {
    let lat = r as f32 / height as f32;
    let base = if lat < 0.1 || lat > 0.9 {
        0
    } else {
        (simple_hash(q, r) % 3000) as i32 - 500
    };
    base.max(-100)
}

/// Deterministic hash for pseudo-random terrain.
fn simple_hash(q: i32, r: i32) -> u32 {
    let mut h: u32 = 374761393;
    h = h.wrapping_mul(1103515245).wrapping_add(q as u32);
    h = h.wrapping_mul(1103515245).wrapping_add(r as u32);
    h ^= h >> 16;
    h = h.wrapping_mul(2246822519);
    h ^= h >> 13;
    h = h.wrapping_mul(3266489917);
    h ^= h >> 16;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_create() {
        let grid = StrategicGrid::new(100, 50);
        assert_eq!(grid.width, 100);
        assert_eq!(grid.height, 50);
    }

    #[test]
    fn grid_get_wrapping() {
        let grid = StrategicGrid::new(100, 50);
        assert!(grid.get(-1, 5).is_some());
        assert_eq!(grid.get(-1, 5).unwrap().hex.q, 99);
        assert!(grid.get(100, 5).is_some());
        assert_eq!(grid.get(100, 5).unwrap().hex.q, 0);
        assert!(grid.get(50, -1).is_none());
    }

    #[test]
    fn grid_viewport_returns_cells() {
        let grid = StrategicGrid::new(200, 100);
        let cells = grid.viewport(0.0, 0.0, 800.0, 600.0, 4.0);
        assert!(!cells.is_empty());
    }

    #[test]
    fn terrain_deterministic() {
        let t1 = generate_placeholder_terrain(42, 17, 1200, 600);
        let t2 = generate_placeholder_terrain(42, 17, 1200, 600);
        assert_eq!(t1, t2);
    }
}
