use crate::hex::Hex;
use crate::military::unit::UnitType;
use crate::terrain::Terrain;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Lifecycle of a unit transit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TransitStatus {
    /// Unit is moving along the path.
    Active,
    /// Path complete — unit is at its destination.
    Completed,
    /// Player cancelled the order.
    Cancelled,
    /// Unit blocked (next hex impassable for its domain) and is waiting.
    Blocked,
}

/// A pending strategic movement order with live progress. Persisted as a row
/// in `unit_transits`; the strategic tick advances `progress_hex` each second.
///
/// `path` stores hexes from start (index 0) to destination (last). Progress is
/// a float index: `progress_hex = 1.5` means the unit is halfway between
/// `path[1]` and `path[2]`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Transit {
    pub unit_id: u64,
    pub path: Vec<Hex>,
    /// Float index into `path`, in [0, path.len() - 1].
    pub progress_hex: f32,
    pub status: TransitStatus,
    /// Base speed (hex/second) for this unit type on its native terrain.
    pub base_speed: f32,
}

impl Transit {
    pub fn new(unit_id: u64, unit_type: UnitType, path: Vec<Hex>) -> Self {
        Self {
            unit_id,
            path,
            progress_hex: 0.0,
            status: TransitStatus::Active,
            base_speed: unit_type.stats().base_speed,
        }
    }

    /// Returns `(current_hex, next_hex, fraction)` where fraction is 0.0 at
    /// current_hex and 1.0 at next_hex. If the transit is complete, both
    /// hexes are the final one.
    pub fn interpolated_position(&self) -> (Hex, Hex, f32) {
        if self.path.is_empty() {
            return (Hex::new(0, 0), Hex::new(0, 0), 0.0);
        }
        let last = self.path.len() - 1;
        let idx = self.progress_hex.floor() as usize;
        if idx >= last {
            let end = self.path[last];
            return (end, end, 0.0);
        }
        let frac = self.progress_hex - idx as f32;
        (self.path[idx], self.path[idx + 1], frac)
    }

    /// Advance by `dt` seconds, looking up the terrain at each leg through
    /// `terrain_at`. Returns true if the transit just reached its goal.
    ///
    /// Speed per leg depends on the terrain of the *destination* hex (you
    /// "spend" your speed entering that hex). Impassable destinations flip
    /// the transit to `Blocked`.
    pub fn tick(&mut self, dt: f32, unit_type: UnitType, terrain_at: impl Fn(Hex) -> Terrain) -> bool {
        if self.status != TransitStatus::Active && self.status != TransitStatus::Blocked {
            return false;
        }
        if self.path.len() <= 1 {
            self.status = TransitStatus::Completed;
            return true;
        }

        let last = self.path.len() - 1;
        let mut remaining = dt;

        while remaining > 0.0 {
            let idx = self.progress_hex.floor() as usize;
            if idx >= last {
                self.progress_hex = last as f32;
                self.status = TransitStatus::Completed;
                return true;
            }
            let leg_target = self.path[idx + 1];
            let leg_terrain = terrain_at(leg_target);
            let speed = match unit_type.speed_on(leg_terrain) {
                Some(s) if s > 0.0 => s,
                _ => {
                    self.status = TransitStatus::Blocked;
                    return false;
                }
            };

            let frac_left = (idx as f32 + 1.0) - self.progress_hex;
            let time_to_next_hex = frac_left / speed;

            if remaining >= time_to_next_hex {
                // Finish this leg.
                self.progress_hex = (idx + 1) as f32;
                remaining -= time_to_next_hex;
            } else {
                // Partial progress inside this leg.
                self.progress_hex += remaining * speed;
                remaining = 0.0;
            }
        }

        self.status = TransitStatus::Active;
        false
    }

    /// Current "closest" hex — useful for collision / combat checks where a
    /// single authoritative location is needed.
    pub fn current_hex(&self) -> Hex {
        let (from, to, f) = self.interpolated_position();
        if f >= 0.5 {
            to
        } else {
            from
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plains_everywhere(_: Hex) -> Terrain {
        Terrain::Plains
    }

    #[test]
    fn transit_completes_along_plains() {
        let path = vec![
            Hex::new(0, 0),
            Hex::new(1, 0),
            Hex::new(2, 0),
            Hex::new(3, 0),
        ];
        let mut t = Transit::new(1, UnitType::Infantry, path);
        // Plains speed for Infantry = 0.25 hex/s → 12s for 3 hops.
        let done = t.tick(20.0, UnitType::Infantry, plains_everywhere);
        assert!(done);
        assert_eq!(t.status, TransitStatus::Completed);
    }

    #[test]
    fn transit_partial_leaves_unit_between_hexes() {
        let path = vec![Hex::new(0, 0), Hex::new(1, 0), Hex::new(2, 0)];
        let mut t = Transit::new(1, UnitType::Infantry, path);
        // 2 seconds at 0.25 hex/s = 0.5 hex of progress.
        t.tick(2.0, UnitType::Infantry, plains_everywhere);
        assert_eq!(t.status, TransitStatus::Active);
        let (from, to, f) = t.interpolated_position();
        assert_eq!(from, Hex::new(0, 0));
        assert_eq!(to, Hex::new(1, 0));
        assert!((f - 0.5).abs() < 1e-3);
    }

    #[test]
    fn transit_blocks_on_impassable_ocean() {
        let path = vec![Hex::new(0, 0), Hex::new(1, 0)];
        let mut t = Transit::new(1, UnitType::Infantry, path);
        let ocean = |_: Hex| Terrain::Ocean;
        let done = t.tick(10.0, UnitType::Infantry, ocean);
        assert!(!done);
        assert_eq!(t.status, TransitStatus::Blocked);
    }

    #[test]
    fn ship_traverses_ocean_but_not_land() {
        let path = vec![Hex::new(0, 0), Hex::new(1, 0)];
        let mut t = Transit::new(1, UnitType::Ship, path.clone());
        let all_ocean = |_: Hex| Terrain::Ocean;
        let done = t.tick(10.0, UnitType::Ship, all_ocean);
        assert!(done);

        let mut t2 = Transit::new(2, UnitType::Ship, path);
        let all_plains = |_: Hex| Terrain::Plains;
        t2.tick(10.0, UnitType::Ship, all_plains);
        assert_eq!(t2.status, TransitStatus::Blocked);
    }
}
