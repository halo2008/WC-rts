//! Strategic-layer auto-resolve combat.
//!
//! When two or more nations end a tick on the same hex, we run a simple
//! stats-based damage exchange: each side's total attack × morale × numbers
//! inflicts losses on the other side, scaled by terrain cover. This is the
//! "quick" resolution — per-unit RTS combat on the battlefield instance is
//! Etap 4.5.B and lives in a different module.

use crate::hex::Hex;
use crate::military::UnitType;
use crate::terrain::Terrain;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Minimal unit snapshot used by `autoresolve`. Server constructs this from
/// the `units` row; we don't need the full `Unit` struct here.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct CombatUnit {
    pub id: u64,
    pub unit_type: UnitType,
    pub hp: i32,
    pub morale: f32,
}

/// Damage applied to a single unit after a tick of combat.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct UnitDamage {
    pub unit_id: u64,
    /// HP points removed. Clamped to unit's current HP by the caller.
    pub damage: i32,
    /// True if the damage reduced this unit to 0 HP.
    pub destroyed: bool,
}

/// Result of one tick of auto-resolve combat on a contested hex.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CombatOutcome {
    pub hex: Hex,
    pub side_a_losses: Vec<UnitDamage>,
    pub side_b_losses: Vec<UnitDamage>,
    pub side_a_total_damage: i32,
    pub side_b_total_damage: i32,
}

/// Cover bonus (0..=0.7) per terrain type. Attackers get no cover — only
/// whoever is "defending" (arbitrary in MVP: we give cover to the smaller
/// side so terrain genuinely helps the disadvantaged player).
pub fn terrain_cover(terrain: Terrain) -> f32 {
    match terrain {
        Terrain::Forest => 0.30,
        Terrain::Hills => 0.20,
        Terrain::Mountain => 0.35,
        Terrain::Urban => 0.40,
        Terrain::Tundra | Terrain::Ice => 0.05,
        _ => 0.0,
    }
}

/// Per-tick strength for a unit: `attack × morale`. `defense × morale` for the
/// defensive half.
fn unit_strength(u: &CombatUnit) -> (f32, f32) {
    let stats = u.unit_type.stats();
    let morale = u.morale.clamp(0.0, 1.5); // allow buffs up to 150%
    (stats.attack as f32 * morale, stats.defense as f32 * morale)
}

/// Distribute `total_damage` evenly across `units` using their current HP as
/// capacity. Overflow (a unit taking more than its HP) is capped at the unit's
/// HP and the rest is discarded — we don't re-target excess damage to other
/// units this tick. Keeps the math simple and avoids one lucky tank soaking
/// everything.
fn distribute_damage(units: &[CombatUnit], total_damage: i32) -> Vec<UnitDamage> {
    if units.is_empty() || total_damage <= 0 {
        return Vec::new();
    }

    let per_unit = total_damage / units.len() as i32;
    let remainder = total_damage % units.len() as i32;

    units
        .iter()
        .enumerate()
        .map(|(i, u)| {
            // First `remainder` units eat an extra HP each.
            let raw = per_unit + if (i as i32) < remainder { 1 } else { 0 };
            let capped = raw.min(u.hp).max(0);
            UnitDamage {
                unit_id: u.id,
                damage: capped,
                destroyed: capped >= u.hp && u.hp > 0,
            }
        })
        .collect()
}

/// Run one tick of auto-resolve combat at `hex` between two sides. Neither
/// side is inherently "attacker" or "defender"; we give the smaller army the
/// terrain cover bonus so the terrain helps whoever is outnumbered.
pub fn autoresolve(
    hex: Hex,
    side_a: &[CombatUnit],
    side_b: &[CombatUnit],
    terrain: Terrain,
) -> CombatOutcome {
    if side_a.is_empty() || side_b.is_empty() {
        return CombatOutcome {
            hex,
            side_a_losses: Vec::new(),
            side_b_losses: Vec::new(),
            side_a_total_damage: 0,
            side_b_total_damage: 0,
        };
    }

    let (a_atk, a_def): (f32, f32) = side_a.iter().map(unit_strength).fold((0.0, 0.0), |acc, x| {
        (acc.0 + x.0, acc.1 + x.1)
    });
    let (b_atk, b_def): (f32, f32) = side_b.iter().map(unit_strength).fold((0.0, 0.0), |acc, x| {
        (acc.0 + x.0, acc.1 + x.1)
    });

    // Whoever is smaller gets the cover bonus.
    let cover = terrain_cover(terrain);
    let (a_def_effective, b_def_effective) = if side_a.len() <= side_b.len() {
        (a_def * (1.0 + cover), b_def)
    } else {
        (a_def, b_def * (1.0 + cover))
    };

    // Damage coefficient: each point of effective attack translates into a
    // fraction of a point of damage to the enemy, then reduced by the enemy's
    // effective defence. Tuned so a full tick is a meaningful exchange but
    // doesn't one-shot anyone.
    const DAMAGE_COEFF: f32 = 0.40;
    let damage_to_b = ((a_atk * DAMAGE_COEFF) - (b_def_effective * 0.05)).max(1.0) as i32;
    let damage_to_a = ((b_atk * DAMAGE_COEFF) - (a_def_effective * 0.05)).max(1.0) as i32;

    CombatOutcome {
        hex,
        side_a_losses: distribute_damage(side_a, damage_to_a),
        side_b_losses: distribute_damage(side_b, damage_to_b),
        side_a_total_damage: damage_to_a,
        side_b_total_damage: damage_to_b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inf(id: u64, hp: i32) -> CombatUnit {
        CombatUnit {
            id,
            unit_type: UnitType::Infantry,
            hp,
            morale: 1.0,
        }
    }

    #[test]
    fn empty_side_zero_damage() {
        let outcome = autoresolve(Hex::new(0, 0), &[], &[inf(1, 100)], Terrain::Plains);
        assert_eq!(outcome.side_a_total_damage, 0);
        assert_eq!(outcome.side_b_total_damage, 0);
    }

    #[test]
    fn bigger_side_hurts_smaller_more() {
        let big = vec![inf(1, 1000), inf(2, 1000), inf(3, 1000)];
        let small = vec![inf(10, 1000)];
        let outcome = autoresolve(Hex::new(0, 0), &big, &small, Terrain::Plains);
        // The lone defender soaks more damage than each attacker does.
        assert!(outcome.side_b_total_damage > outcome.side_a_total_damage);
    }

    #[test]
    fn terrain_cover_helps_underdog() {
        let big = vec![inf(1, 1000), inf(2, 1000), inf(3, 1000)];
        let small = vec![inf(10, 1000)];
        let plains = autoresolve(Hex::new(0, 0), &big, &small, Terrain::Plains);
        let urban = autoresolve(Hex::new(0, 0), &big, &small, Terrain::Urban);
        // Urban cover reduces damage to the smaller side.
        assert!(urban.side_b_total_damage <= plains.side_b_total_damage);
    }

    #[test]
    fn damage_distributed_across_stack() {
        let stack = vec![inf(1, 1000), inf(2, 1000), inf(3, 1000)];
        let losses = distribute_damage(&stack, 30);
        assert_eq!(losses.len(), 3);
        assert_eq!(losses.iter().map(|l| l.damage).sum::<i32>(), 30);
    }

    #[test]
    fn low_hp_unit_marked_destroyed() {
        let stack = vec![inf(1, 5)];
        let losses = distribute_damage(&stack, 100);
        assert!(losses[0].destroyed);
        assert_eq!(losses[0].damage, 5);
    }
}
