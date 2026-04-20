use crate::economy::Stockpile;
use crate::hex::Hex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Static nation metadata — mirrors rows in the `nations` SQL table.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Nation {
    pub id: String,
    pub code: String,
    pub name: String,
    pub government_type: String,
    /// 1 = superpower, 4 = minor.
    pub tier: u8,
    /// CSS hex colour including leading '#'.
    pub color: String,
}

/// How a nation is allocating its tax income across strategic priorities.
/// The five sliders must sum to 1.0; the server normalises on write.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct BudgetAllocation {
    pub military: f32,
    pub economy: f32,
    pub research: f32,
    pub social: f32,
    pub intel: f32,
}

impl Default for BudgetAllocation {
    fn default() -> Self {
        Self {
            military: 0.35,
            economy: 0.30,
            research: 0.15,
            social: 0.15,
            intel: 0.05,
        }
    }
}

impl BudgetAllocation {
    pub fn sum(&self) -> f32 {
        self.military + self.economy + self.research + self.social + self.intel
    }

    /// Normalise so all five sliders sum to 1.0. No-op if already normal.
    pub fn normalized(self) -> Self {
        let s = self.sum();
        if s <= f32::EPSILON {
            return BudgetAllocation::default();
        }
        Self {
            military: self.military / s,
            economy: self.economy / s,
            research: self.research / s,
            social: self.social / s,
            intel: self.intel / s,
        }
    }
}

/// Live per-nation state. Separate from `Nation` because nation metadata is
/// static and state ticks at 5-minute cadence.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct NationState {
    pub nation_id: String,
    pub stockpile: Stockpile,
    pub budget: BudgetAllocation,
    /// Capital hex on the strategic grid — used as default spawn / rally point.
    pub capital: Hex,
    pub war_support: f32,
    pub stability: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_defaults_sum_to_one() {
        let b = BudgetAllocation::default();
        assert!((b.sum() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn budget_normalises_arbitrary_values() {
        let b = BudgetAllocation {
            military: 2.0,
            economy: 2.0,
            research: 0.0,
            social: 0.0,
            intel: 0.0,
        }
        .normalized();
        assert!((b.sum() - 1.0).abs() < 1e-4);
        assert!((b.military - 0.5).abs() < 1e-4);
    }

    #[test]
    fn budget_handles_zero_without_nan() {
        let b = BudgetAllocation {
            military: 0.0,
            economy: 0.0,
            research: 0.0,
            social: 0.0,
            intel: 0.0,
        }
        .normalized();
        assert!((b.sum() - 1.0).abs() < 1e-4);
    }
}
