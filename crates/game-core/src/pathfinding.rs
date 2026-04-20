use crate::hex::Hex;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Ordering;

/// A* pathfinding on a hex grid with wrap-around support.
///
/// `is_passable` — closure returning true if a hex can be traversed.
/// `movement_cost` — closure returning the cost to enter a hex (1.0 = normal).
/// `width` — map width for wrap-around; if None, no wrapping.
pub fn astar(
    start: Hex,
    goal: Hex,
    is_passable: impl Fn(Hex) -> bool,
    movement_cost: impl Fn(Hex) -> f32,
    width: Option<i32>,
) -> Option<Vec<Hex>> {
    if !is_passable(start) || !is_passable(goal) {
        return None;
    }

    if start == goal {
        return Some(vec![start]);
    }

    let heuristic = |hex: Hex| -> f32 {
        if let Some(w) = width {
            hex.distance_wrap(goal, w) as f32
        } else {
            hex.distance(goal) as f32
        }
    };

    let mut open = BinaryHeap::new();
    let mut g_score: HashMap<Hex, f32> = HashMap::new();
    let mut came_from: HashMap<Hex, Hex> = HashMap::new();
    let mut closed: HashSet<Hex> = HashSet::new();

    g_score.insert(start, 0.0);
    open.push(Node {
        hex: start,
        f: heuristic(start),
    });

    while let Some(current) = open.pop() {
        if current.hex == goal {
            return Some(reconstruct_path(&came_from, goal));
        }

        if closed.contains(&current.hex) {
            continue;
        }
        closed.insert(current.hex);

        let neighbors = if let Some(w) = width {
            current.hex.neighbors_wrap(w)
        } else {
            current.hex.neighbors()
        };

        for neighbor in neighbors {
            if closed.contains(&neighbor) || !is_passable(neighbor) {
                continue;
            }

            let cost = movement_cost(neighbor);
            if cost == f32::INFINITY {
                continue;
            }

            let tentative_g = g_score[&current.hex] + cost;

            let is_better = match g_score.get(&neighbor) {
                Some(&existing) => tentative_g < existing,
                None => true,
            };

            if is_better {
                came_from.insert(neighbor, current.hex);
                g_score.insert(neighbor, tentative_g);
                let f = tentative_g + heuristic(neighbor);
                open.push(Node { hex: neighbor, f });
            }
        }
    }

    None // No path found
}

/// A* with a max search radius. Returns None if no path within radius.
pub fn astar_limited(
    start: Hex,
    goal: Hex,
    is_passable: impl Fn(Hex) -> bool,
    movement_cost: impl Fn(Hex) -> f32,
    width: Option<i32>,
    max_radius: i32,
) -> Option<Vec<Hex>> {
    let max_dist = if let Some(w) = width {
        start.distance_wrap(goal, w)
    } else {
        start.distance(goal)
    };

    if max_dist > max_radius {
        return None;
    }

    astar(start, goal, is_passable, movement_cost, width)
}

/// Get all hexes reachable from `start` within a given movement budget.
/// Returns a map of hex → cost to reach it.
pub fn reachable_hexes(
    start: Hex,
    budget: f32,
    is_passable: impl Fn(Hex) -> bool,
    movement_cost: impl Fn(Hex) -> f32,
    width: Option<i32>,
) -> HashMap<Hex, f32> {
    let mut reachable: HashMap<Hex, f32> = HashMap::new();
    let mut open = BinaryHeap::new();

    reachable.insert(start, 0.0);
    open.push(Node {
        hex: start,
        f: 0.0,
    });

    while let Some(current) = open.pop() {
        let current_cost = reachable[&current.hex];

        // Skip if we've found a better path since this was pushed
        if current.f > current_cost {
            continue;
        }

        let neighbors = if let Some(w) = width {
            current.hex.neighbors_wrap(w)
        } else {
            current.hex.neighbors()
        };

        for neighbor in neighbors {
            if !is_passable(neighbor) {
                continue;
            }

            let cost = movement_cost(neighbor);
            if cost == f32::INFINITY {
                continue;
            }

            let new_cost = current_cost + cost;
            if new_cost > budget {
                continue;
            }

            let is_better = match reachable.get(&neighbor) {
                Some(&existing) => new_cost < existing,
                None => true,
            };

            if is_better {
                reachable.insert(neighbor, new_cost);
                open.push(Node {
                    hex: neighbor,
                    f: new_cost,
                });
            }
        }
    }

    reachable
}

fn reconstruct_path(came_from: &HashMap<Hex, Hex>, goal: Hex) -> Vec<Hex> {
    let mut path = vec![goal];
    let mut current = goal;
    while let Some(&prev) = came_from.get(&current) {
        path.push(prev);
        current = prev;
    }
    path.reverse();
    path
}

/// Priority queue node (max-heap, so we reverse for min-heap via Ord).
#[derive(Debug, Clone)]
struct Node {
    hex: Hex,
    f: f32,
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.f == other.f
    }
}

impl Eq for Node {}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse order for min-heap (BinaryHeap is max-heap by default)
        other.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::Terrain;

    fn terrain_at(hex: Hex) -> Terrain {
        // Simple test terrain: everything is plains except a wall at (3, 0)
        if hex.q == 3 && hex.r == 0 {
            Terrain::Mountain
        } else {
            Terrain::Plains
        }
    }

    fn is_passable(hex: Hex) -> bool {
        terrain_at(hex).is_passable_land()
    }

    fn move_cost(hex: Hex) -> f32 {
        terrain_at(hex).movement_cost()
    }

    #[test]
    fn astar_straight_line() {
        let start = Hex::new(0, 0);
        let goal = Hex::new(5, 0);
        let path = astar(start, goal, is_passable, move_cost, None);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(*path.first().unwrap(), start);
        assert_eq!(*path.last().unwrap(), goal);
    }

    #[test]
    fn astar_avoids_obstacle() {
        let start = Hex::new(0, 0);
        let goal = Hex::new(5, 0);
        let path = astar(start, goal, is_passable, move_cost, None);
        assert!(path.is_some());
        let path = path.unwrap();
        // Path should not go through (3, 0) which is Mountain
        assert!(!path.iter().any(|h| h.q == 3 && h.r == 0));
    }

    #[test]
    fn astar_no_path() {
        // Create an impassable wall
        let is_passable = |hex: Hex| hex.r != 0;
        let move_cost = |hex: Hex| if hex.r == 0 { f32::INFINITY } else { 1.0 };
        let start = Hex::new(0, 0);
        let goal = Hex::new(5, 0);
        // Both start and goal are on the impassable row
        let result = astar(start, goal, is_passable, move_cost, None);
        assert!(result.is_none());
    }

    #[test]
    fn astar_same_hex() {
        let start = Hex::new(3, 3);
        let path = astar(start, start, is_passable, move_cost, None);
        assert_eq!(path, Some(vec![start]));
    }

    #[test]
    fn astar_wrap_around() {
        let width = 10;
        let is_passable = |_: Hex| true;
        let move_cost = |_: Hex| 1.0;
        let start = Hex::new(9, 0);
        let goal = Hex::new(1, 0);
        let path = astar(start, goal, is_passable, move_cost, Some(width));
        assert!(path.is_some());
        let path = path.unwrap();
        // With wrap, path should be short: 9 → 0 → 1
        assert!(path.len() <= 3);
    }

    #[test]
    fn reachable_within_budget() {
        let is_passable = |_: Hex| true;
        let move_cost = |_: Hex| 1.0;
        let start = Hex::new(0, 0);
        let reachable = reachable_hexes(start, 2.0, is_passable, move_cost, None);
        // Should include start (cost 0) and neighbors within 2 moves
        assert!(reachable.contains_key(&start));
        assert!(reachable.len() > 1);
    }

    #[test]
    fn reachable_respects_terrain() {
        let is_passable = |hex: Hex| hex.q != 3;
        let move_cost = |hex: Hex| if hex.q == 3 { f32::INFINITY } else { 1.0 };
        let start = Hex::new(0, 0);
        let reachable = reachable_hexes(start, 10.0, is_passable, move_cost, None);
        // No hex with q=3 should be reachable
        assert!(!reachable.keys().any(|h| h.q == 3));
    }

    #[test]
    fn astar_wrap_prefers_shortcut_around_world() {
        // Width 100: direct 97→3 is distance 94, wrap path is distance 6.
        let width = 100;
        let path = astar(Hex::new(97, 0), Hex::new(3, 0), |_| true, |_| 1.0, Some(width))
            .expect("path must exist");
        // Path must fit the wrap budget comfortably (6 hops + start = 7).
        assert!(path.len() <= 8, "wrap path too long: {}", path.len());
    }

    #[test]
    fn astar_respects_polar_clamp() {
        // Impassable if off-grid (r < 0 or r >= height). Simulates polar clamp.
        let height = 10;
        let is_passable = move |h: Hex| h.r >= 0 && h.r < height;
        let move_cost = |_: Hex| 1.0;
        let start = Hex::new(5, 0);
        let goal = Hex::new(5, 9);
        let path = astar(start, goal, is_passable, move_cost, None)
            .expect("must find a path inside the band");
        // No hex in the path crosses the pole.
        assert!(path.iter().all(|h| h.r >= 0 && h.r < height));
    }

    #[test]
    fn astar_wall_of_mountains_routes_around() {
        // Row r=3 is a wall except for a single passable hex at (10, 3).
        let is_passable = |h: Hex| h.r != 3 || h.q == 10;
        let move_cost = |h: Hex| if !is_passable(h) { f32::INFINITY } else { 1.0 };
        let start = Hex::new(0, 0);
        let goal = Hex::new(0, 6);
        let path = astar(start, goal, is_passable, move_cost, None)
            .expect("path must exist through the gap");
        // Path must include the gap at (10, 3).
        assert!(path.iter().any(|h| h.q == 10 && h.r == 3));
    }
}
