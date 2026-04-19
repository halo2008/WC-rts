use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Axial hex coordinate with wrap-around on Q axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct Hex {
    pub q: i32,
    pub r: i32,
}

impl Hex {
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }

    /// Wrap Q into [0, width). Used for equirectangular wrap-around (east-west).
    pub fn wrap(self, width: i32) -> Self {
        Self {
            q: ((self.q % width) + width) % width,
            r: self.r,
        }
    }

    /// Axial neighbors (6 directions, pointy-top orientation).
    pub const DIRECTIONS: [Hex; 6] = [
        Hex::new(1, 0),
        Hex::new(1, -1),
        Hex::new(0, -1),
        Hex::new(-1, 0),
        Hex::new(-1, 1),
        Hex::new(0, 1),
    ];

    pub fn neighbors(self) -> [Hex; 6] {
        Self::DIRECTIONS.map(|d| self + d)
    }

    /// Wrap-aware neighbors for the strategic map.
    pub fn neighbors_wrap(self, width: i32) -> [Hex; 6] {
        self.neighbors().map(|h| h.wrap(width))
    }

    /// Axial distance (cube distance). Does NOT account for wrapping.
    pub fn distance(self, other: Hex) -> i32 {
        let dq = self.q - other.q;
        let dr = self.r - other.r;
        let ds = -dq - dr;
        dq.abs().max(dr.abs()).max(ds.abs())
    }

    /// Wrap-aware distance on Q axis only.
    /// Takes the shorter path around the globe.
    pub fn distance_wrap(self, other: Hex, width: i32) -> i32 {
        let dq = (self.q - other.q).abs();
        let dq_wrap = width - dq;
        let dq_min = dq.min(dq_wrap);
        let dr = (self.r - other.r).abs();
        let ds = (-(self.q - other.q) - (self.r - other.r)).abs();
        let ds_wrap = width - (-(self.q - other.q)).abs();
        // Approximate: just use shortest Q delta
        let dq_eff = dq_min;
        let ds_eff = ds.min(ds_wrap + dq_wrap - dq);
        dq_eff.max(dr).max(ds_eff.min(dq_eff + dr))
    }

    /// Hexes in a ring of radius `n` around self.
    pub fn ring(self, n: i32) -> Vec<Hex> {
        if n == 0 {
            return vec![self];
        }
        let mut result = Vec::with_capacity((6 * n) as usize);
        let mut hex = self + Self::DIRECTIONS[4] * n; // start SW
        for i in 0..6 {
            for _ in 0..n {
                result.push(hex);
                hex = hex + Self::DIRECTIONS[i];
            }
        }
        result
    }

    /// All hexes within radius `n` (inclusive).
    pub fn spiral(self, n: i32) -> Vec<Hex> {
        let mut result = vec![self];
        for i in 1..=n {
            result.extend(self.ring(i));
        }
        result
    }

    /// Line from self to target (bresenham-style on hex grid).
    pub fn line_to(self, target: Hex) -> Vec<Hex> {
        let n = self.distance(target);
        if n == 0 {
            return vec![self];
        }
        let mut result = Vec::with_capacity((n + 1) as usize);
        for i in 0..=n {
            result.push(self.lerp(target, i as f32 / n as f32).round());
        }
        result
    }

    fn lerp(self, other: Hex, t: f32) -> HexF {
        HexF {
            q: self.q as f32 + (other.q as f32 - self.q as f32) * t,
            r: self.r as f32 + (other.r as f32 - self.r as f32) * t,
        }
    }
}

impl std::ops::Add<Hex> for Hex {
    type Output = Hex;
    fn add(self, rhs: Hex) -> Hex {
        Hex::new(self.q + rhs.q, self.r + rhs.r)
    }
}

impl std::ops::Mul<i32> for Hex {
    type Output = Hex;
    fn mul(self, rhs: i32) -> Hex {
        Hex::new(self.q * rhs, self.r * rhs)
    }
}

/// Floating-point hex for interpolation.
struct HexF {
    q: f32,
    r: f32,
}

impl HexF {
    fn round(self) -> Hex {
        let s = -self.q - self.r;
        let rq = self.q.round();
        let rr = self.r.round();
        let rs = s.round();

        let dq = (rq - self.q).abs();
        let dr = (rr - self.r).abs();
        let ds = (rs - s).abs();

        if dq > dr && dq > ds {
            Hex::new((-rr - rs) as i32, rr as i32)
        } else if dr > ds {
            Hex::new(rq as i32, (-rq - rs) as i32)
        } else {
            Hex::new(rq as i32, rr as i32)
        }
    }
}

/// Convert axial hex to pixel position (pointy-top).
pub fn hex_to_pixel(hex: Hex, size: f32) -> (f32, f32) {
    let x = size * (3.0_f32.sqrt() * hex.q as f32 + 3.0_f32.sqrt() / 2.0 * hex.r as f32);
    let y = size * (1.5 * hex.r as f32);
    (x, y)
}

/// Convert pixel position to axial hex (pointy-top).
pub fn pixel_to_hex(x: f32, y: f32, size: f32) -> Hex {
    let q = (3.0_f32.sqrt() / 3.0 * x - 1.0 / 3.0 * y) / size;
    let r = (2.0 / 3.0 * y) / size;
    HexF { q, r }.round()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_addition() {
        assert_eq!(Hex::new(1, 2) + Hex::new(3, 4), Hex::new(4, 6));
    }

    #[test]
    fn hex_distance_origin() {
        assert_eq!(Hex::new(0, 0).distance(Hex::new(0, 0)), 0);
        assert_eq!(Hex::new(0, 0).distance(Hex::new(1, 0)), 1);
        assert_eq!(Hex::new(0, 0).distance(Hex::new(3, -2)), 3);
    }

    #[test]
    fn hex_neighbors_count() {
        assert_eq!(Hex::new(5, 5).neighbors().len(), 6);
    }

    #[test]
    fn hex_wrap() {
        let width = 1200;
        assert_eq!(Hex::new(-1, 5).wrap(width), Hex::new(1199, 5));
        assert_eq!(Hex::new(1200, 5).wrap(width), Hex::new(0, 5));
        assert_eq!(Hex::new(600, 5).wrap(width), Hex::new(600, 5));
    }

    #[test]
    fn hex_ring_radius_1() {
        let ring = Hex::new(0, 0).ring(1);
        assert_eq!(ring.len(), 6);
        for h in &ring {
            assert_eq!(h.distance(Hex::new(0, 0)), 1);
        }
    }

    #[test]
    fn hex_spiral_radius_2() {
        let spiral = Hex::new(0, 0).spiral(2);
        assert_eq!(spiral.len(), 1 + 6 + 12); // 19
    }

    #[test]
    fn pixel_roundtrip() {
        let hex = Hex::new(5, 3);
        let (x, y) = hex_to_pixel(hex, 10.0);
        let result = pixel_to_hex(x, y, 10.0);
        assert_eq!(result, hex);
    }

    #[test]
    fn line_to() {
        let line = Hex::new(0, 0).line_to(Hex::new(3, 0));
        assert_eq!(line.len(), 4);
        assert_eq!(*line.first().unwrap(), Hex::new(0, 0));
        assert_eq!(*line.last().unwrap(), Hex::new(3, 0));
    }
}
