use crate::hex::Hex;
use std::collections::HashMap;

/// Spatial index for fast hex-based queries.
/// Uses a hash map keyed by (q, r) for O(1) lookups.
/// Supports wrap-around on the Q axis for the strategic map.
pub struct HexIndex<T> {
    data: HashMap<(i32, i32), T>,
    width: Option<i32>,
}

impl<T> HexIndex<T> {
    /// Create a new spatial index. If `width` is provided, Q coordinates
    /// are wrapped modulo width on insert and lookup.
    pub fn new(width: Option<i32>) -> Self {
        Self {
            data: HashMap::new(),
            width,
        }
    }

    /// Insert a value at the given hex coordinate.
    /// If wrap-around is enabled, Q is wrapped automatically.
    pub fn insert(&mut self, hex: Hex, value: T) {
        let key = self.key(hex);
        self.data.insert(key, value);
    }

    /// Get a value at the given hex coordinate.
    pub fn get(&self, hex: Hex) -> Option<&T> {
        self.data.get(&self.key(hex))
    }

    /// Get a mutable reference to a value at the given hex coordinate.
    pub fn get_mut(&mut self, hex: Hex) -> Option<&mut T> {
        let key = self.key(hex);
        self.data.get_mut(&key)
    }

    /// Remove a value at the given hex coordinate.
    pub fn remove(&mut self, hex: Hex) -> Option<T> {
        self.data.remove(&self.key(hex))
    }

    /// Check if a hex coordinate has an entry.
    pub fn contains(&self, hex: Hex) -> bool {
        self.data.contains_key(&self.key(hex))
    }

    /// Number of entries in the index.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Is the index empty?
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Query all entries within a given radius of a center hex.
    /// Returns references to (hex, value) pairs.
    pub fn query_radius(&self, center: Hex, radius: i32) -> Vec<(Hex, &T)> {
        let mut results = Vec::new();
        for hex in center.spiral(radius) {
            if let Some(value) = self.get(hex) {
                results.push((hex, value));
            }
        }
        results
    }

    /// Query all entries within a rectangular viewport (in hex coordinates).
    /// Useful for "what's visible on screen" queries.
    pub fn query_rect(&self, min_q: i32, min_r: i32, max_q: i32, max_r: i32) -> Vec<(Hex, &T)> {
        let mut results = Vec::new();
        for r in min_r..=max_r {
            for q in min_q..=max_q {
                let hex = if let Some(w) = self.width {
                    Hex::new(q, r).wrap(w)
                } else {
                    Hex::new(q, r)
                };
                if let Some(value) = self.get(hex) {
                    results.push((hex, value));
                }
            }
        }
        results
    }

    /// Iterate over all entries.
    pub fn iter(&self) -> impl Iterator<Item = (Hex, &T)> {
        self.data.iter().map(|(&(q, r), v)| (Hex::new(q, r), v))
    }

    fn key(&self, hex: Hex) -> (i32, i32) {
        if let Some(w) = self.width {
            (hex.wrap(w).q, hex.r)
        } else {
            (hex.q, hex.r)
        }
    }
}

impl<T> Default for HexIndex<T> {
    fn default() -> Self {
        Self::new(None)
    }
}

/// Line-of-sight check between two hexes.
/// Returns true if there is clear LOS, false if blocked.
/// `is_blocking` — closure returning true if a hex blocks LOS (e.g., mountain, tall building).
pub fn line_of_sight(
    from: Hex,
    to: Hex,
    is_blocking: impl Fn(Hex) -> bool,
) -> bool {
    let line = from.line_to(to);
    // Check all hexes between from and to (exclusive of endpoints)
    for hex in line.iter().skip(1).take(line.len().saturating_sub(2)) {
        if is_blocking(*hex) {
            return false;
        }
    }
    true
}

/// Line-of-sight with elevation awareness.
/// Returns true if LOS is clear considering elevation differences.
/// `elevation_at` — closure returning the elevation of a hex.
/// `blocking_height` — additional height of blocking objects (e.g., buildings).
pub fn line_of_sight_elevated(
    from: Hex,
    to: Hex,
    elevation_at: impl Fn(Hex) -> i32,
    blocking_height: impl Fn(Hex) -> i32,
) -> bool {
    let line = from.line_to(to);
    let from_elev = elevation_at(from) + blocking_height(from);
    let to_elev = elevation_at(to) + blocking_height(to);
    let dist = from.distance(to) as f32;

    if dist < 1.0 {
        return true;
    }

    for (i, hex) in line.iter().enumerate() {
        if i == 0 || i == line.len() - 1 {
            continue;
        }
        let t = i as f32 / dist;
        let line_elev = from_elev as f32 + (to_elev - from_elev) as f32 * t;
        let hex_top = elevation_at(*hex) + blocking_height(*hex);

        if hex_top as f32 > line_elev {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_index_insert_get() {
        let mut index: HexIndex<String> = HexIndex::new(None);
        index.insert(Hex::new(5, 3), "test".to_string());
        assert_eq!(index.get(Hex::new(5, 3)), Some(&"test".to_string()));
        assert_eq!(index.get(Hex::new(5, 4)), None);
    }

    #[test]
    fn hex_index_wrap_around() {
        let mut index: HexIndex<String> = HexIndex::new(Some(10));
        index.insert(Hex::new(10, 5), "wrapped".to_string());
        // q=10 wraps to q=0
        assert_eq!(index.get(Hex::new(0, 5)), Some(&"wrapped".to_string()));
        assert_eq!(index.get(Hex::new(10, 5)), Some(&"wrapped".to_string()));
    }

    #[test]
    fn hex_index_query_radius() {
        let mut index: HexIndex<i32> = HexIndex::new(None);
        index.insert(Hex::new(0, 0), 0);
        index.insert(Hex::new(1, 0), 1);
        index.insert(Hex::new(5, 5), 99);

        let results = index.query_radius(Hex::new(0, 0), 1);
        assert_eq!(results.len(), 2); // (0,0) and (1,0)
    }

    #[test]
    fn hex_index_remove() {
        let mut index: HexIndex<i32> = HexIndex::new(None);
        index.insert(Hex::new(3, 3), 42);
        assert_eq!(index.remove(Hex::new(3, 3)), Some(42));
        assert_eq!(index.get(Hex::new(3, 3)), None);
    }

    #[test]
    fn los_clear() {
        let is_blocking = |_: Hex| false;
        assert!(line_of_sight(Hex::new(0, 0), Hex::new(5, 0), is_blocking));
    }

    #[test]
    fn los_blocked() {
        let is_blocking = |hex: Hex| hex.q == 3 && hex.r == 0;
        assert!(!line_of_sight(Hex::new(0, 0), Hex::new(5, 0), is_blocking));
    }

    #[test]
    fn los_adjacent_always_clear() {
        let is_blocking = |_: Hex| true; // Even if everything "blocks"
        assert!(line_of_sight(Hex::new(0, 0), Hex::new(1, 0), is_blocking));
    }

    #[test]
    fn los_elevated_flat_terrain_clear() {
        let elev = |_: Hex| 0;
        let block = |_: Hex| 0;
        assert!(line_of_sight_elevated(Hex::new(0, 0), Hex::new(5, 0), elev, block));
    }

    #[test]
    fn los_elevated_peak_blocks() {
        // Mountain peak in the middle blocks a line between two sea-level hexes.
        let elev = |h: Hex| if h.q == 3 && h.r == 0 { 500 } else { 0 };
        let block = |_: Hex| 0;
        assert!(!line_of_sight_elevated(Hex::new(0, 0), Hex::new(5, 0), elev, block));
    }

    #[test]
    fn los_elevated_high_shooter_sees_over_obstacle() {
        // Shooter stands on a tall mountain, obstacle is lower than the sight line.
        let elev = |h: Hex| if h.q == 0 { 1000 } else { 0 };
        let block = |h: Hex| if h.q == 3 && h.r == 0 { 100 } else { 0 };
        assert!(line_of_sight_elevated(Hex::new(0, 0), Hex::new(5, 0), elev, block));
    }
}
