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

/// A viewport query result: the cell plus the *unwrapped* q column it is
/// rendered at. For hexes past the anti-meridian, `render_q` may be outside
/// `[0, width)` — this lets the renderer place the wrap-around copy at the
/// right world-x without guessing.
pub struct ViewportCell<'a> {
    pub cell: &'a HexCell,
    pub render_q: i32,
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
    ) -> Vec<ViewportCell<'_>> {
        let sq3 = 3.0_f32.sqrt();
        let hex_w = sq3 * hex_size;
        // Pointy-top rows are 1.5 * size tall (not 2.0 * size — that's a flat-top
        // hex). Getting this wrong causes row range to be ~33% too tight and the
        // top/bottom rows to drop out.
        let row_h = 1.5 * hex_size;

        let min_r_row = ((cam_y - view_height / 2.0) / row_h).floor() as i32 - 1;
        let max_r_row = ((cam_y + view_height / 2.0) / row_h).ceil() as i32 + 1;

        let mut result = Vec::new();
        for r in min_r_row.max(0)..=max_r_row.min(self.height - 1) {
            // Each row is shifted right by r/2 columns (pointy-top axial → pixel).
            // Compute the visible column range per-row, otherwise larger r values
            // lose the left half of the viewport (the "diagonal cut" bug).
            let offset = r as f32 / 2.0;
            let min_q_col = ((cam_x - view_width / 2.0) / hex_w - offset).floor() as i32 - 1;
            let max_q_col = ((cam_x + view_width / 2.0) / hex_w - offset).ceil() as i32 + 1;
            for q_col in min_q_col..=max_q_col {
                let q = ((q_col % self.width) + self.width) % self.width;
                if let Some(cell) = self.get(q, r) {
                    // Use q_col (may be negative or >= width) so the pixel
                    // position reflects the wrap-around copy actually visible,
                    // not the canonical hex at cell.hex.q.
                    let (px, py) = hex_to_pixel(Hex::new(q_col, r), hex_size);
                    if px >= cam_x - view_width / 2.0 - hex_w
                        && px <= cam_x + view_width / 2.0 + hex_w
                        && py >= cam_y - view_height / 2.0 - row_h
                        && py <= cam_y + view_height / 2.0 + row_h
                    {
                        result.push(ViewportCell { cell, render_q: q_col });
                    }
                }
            }
        }
        result
    }
}

/// Earth-shaped terrain generator. Continents are sum-of-Gaussian blobs at
/// roughly their real-world centres; noise on top breaks up smooth edges
/// so coastlines look organic. Not remotely satellite-accurate — the goal
/// is "rozpoznawalnie Ziemia" when you zoom out or flip to polar view.
fn generate_placeholder_terrain(q: i32, r: i32, width: i32, height: i32) -> Terrain {
    // Geographic coords: lon ∈ (-180, 180], lat ∈ (-90, 90].
    let lon = (q as f32 / width as f32) * 360.0 - 180.0;
    let lat = 90.0 - (r as f32 / height as f32) * 180.0;

    // Ice cap — hard north/south polar rings. Gives the flat-earth "ice
    // wall" look around the outer rim of the polar projection.
    if lat < -75.0 || lat > 82.0 {
        return Terrain::Ice;
    }

    // Continent mass — higher = more landy. Each blob: (centre_lon,
    // centre_lat, stddev_lon, stddev_lat, strength). Stddev in degrees.
    let mut mass: f32 = 0.0;
    // North America (main body)
    mass += blob(lon, lat, -100.0, 45.0, 32.0, 18.0, 1.5);
    // NA northern stretch / Canada
    mass += blob(lon, lat, -95.0, 60.0, 40.0, 10.0, 1.2);
    // Central America
    mass += blob(lon, lat, -85.0, 18.0, 12.0, 8.0, 1.0);
    // South America
    mass += blob(lon, lat, -62.0, -15.0, 14.0, 22.0, 1.3);
    // Greenland
    mass += blob(lon, lat, -40.0, 72.0, 14.0, 8.0, 1.4);
    // Europe
    mass += blob(lon, lat, 15.0, 52.0, 22.0, 12.0, 1.4);
    // Scandinavia
    mass += blob(lon, lat, 18.0, 64.0, 12.0, 8.0, 1.2);
    // North Africa + Sahara
    mass += blob(lon, lat, 18.0, 18.0, 22.0, 14.0, 1.3);
    // Sub-Saharan + Southern Africa
    mass += blob(lon, lat, 25.0, -10.0, 18.0, 20.0, 1.2);
    // Middle East
    mass += blob(lon, lat, 50.0, 30.0, 14.0, 10.0, 1.1);
    // Asia (main)
    mass += blob(lon, lat, 95.0, 45.0, 42.0, 18.0, 1.5);
    // Siberia
    mass += blob(lon, lat, 110.0, 65.0, 48.0, 10.0, 1.3);
    // India
    mass += blob(lon, lat, 78.0, 22.0, 12.0, 10.0, 1.1);
    // South-East Asia / Indonesia
    mass += blob(lon, lat, 115.0, 5.0, 18.0, 8.0, 1.0);
    // Australia
    mass += blob(lon, lat, 135.0, -25.0, 16.0, 10.0, 1.2);
    // Antarctica — big continuous landmass at the south polar cap
    if lat < -65.0 {
        mass += 2.5;
    }

    // Noise adds fractal-looking coastlines so the blobs aren't circular.
    let n = simple_hash(q, r) as f32 / u32::MAX as f32;
    let n2 = simple_hash(q + 7919, r + 1597) as f32 / u32::MAX as f32;
    let noise = (n - 0.5) * 0.45 + (n2 - 0.5) * 0.25;

    let effective = mass + noise;

    if effective < 0.35 {
        // Deep ocean unless close to the coast.
        if effective < 0.15 {
            Terrain::DeepOcean
        } else {
            Terrain::Ocean
        }
    } else if effective < 0.55 {
        // Shallow coast band just off the landmass.
        Terrain::Coast
    } else {
        // Land — classify by latitude + inland hash.
        let sub = simple_hash(q + 500, r + 500) as f32 / u32::MAX as f32;
        let abs_lat = lat.abs();

        if abs_lat > 60.0 {
            // Boreal / tundra
            if sub < 0.5 { Terrain::Tundra } else { Terrain::Forest }
        } else if abs_lat < 30.0 && is_desert_band(lon, lat) {
            if sub < 0.7 { Terrain::Desert } else { Terrain::Plains }
        } else {
            // Temperate — plains / forest / hills / occasional mountain.
            if sub < 0.35 {
                Terrain::Plains
            } else if sub < 0.60 {
                Terrain::Forest
            } else if sub < 0.78 {
                Terrain::Hills
            } else if sub < 0.92 {
                Terrain::Mountain
            } else {
                Terrain::Urban
            }
        }
    }
}

/// Gaussian-like continent blob. Longitude handled with a 360° wrap so
/// continents straddling the antimeridian (unused here) work regardless.
fn blob(
    lon: f32,
    lat: f32,
    cx: f32,
    cy: f32,
    sx: f32,
    sy: f32,
    strength: f32,
) -> f32 {
    let dlon = {
        let mut d = (lon - cx).rem_euclid(360.0);
        if d > 180.0 {
            d -= 360.0;
        }
        d
    };
    let dlat = lat - cy;
    let rr = (dlon / sx).powi(2) + (dlat / sy).powi(2);
    strength * (-rr).exp()
}

/// Rough subtropical desert bands — Sahara / Arabia / Kalahari / Gobi /
/// Outback. Region check only; elsewhere we stay in the temperate classifier.
fn is_desert_band(lon: f32, lat: f32) -> bool {
    let abs_lat = lat.abs();
    if !(15.0..=32.0).contains(&abs_lat) {
        return false;
    }
    // Sahara + Arabia (lat > 0)
    if lat > 0.0 && (-15.0..=55.0).contains(&lon) {
        return true;
    }
    // Gobi (lat > 0, east Asia)
    if lat > 35.0 && (90.0..=115.0).contains(&lon) {
        return true;
    }
    // Outback (lat < 0)
    if lat < 0.0 && (115.0..=140.0).contains(&lon) {
        return true;
    }
    // Kalahari (lat < 0, south Africa)
    if lat < 0.0 && (15.0..=30.0).contains(&lon) {
        return true;
    }
    false
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
