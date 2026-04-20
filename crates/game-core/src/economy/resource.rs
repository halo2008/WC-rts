use serde::{Deserialize, Serialize};
use std::str::FromStr;
use ts_rs::TS;

/// The four strategic resources. The economy tick produces, consumes and
/// transfers these between nations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum Resource {
    Fuel,
    Metals,
    Tech,
    Food,
}

impl Resource {
    pub const ALL: [Resource; 4] = [
        Resource::Fuel,
        Resource::Metals,
        Resource::Tech,
        Resource::Food,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Resource::Fuel => "Fuel",
            Resource::Metals => "Metals",
            Resource::Tech => "Tech",
            Resource::Food => "Food",
        }
    }
}

impl FromStr for Resource {
    type Err = ParseResourceError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "Fuel" => Resource::Fuel,
            "Metals" => Resource::Metals,
            "Tech" => Resource::Tech,
            "Food" => Resource::Food,
            _ => return Err(ParseResourceError(s.to_string())),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ParseResourceError(pub String);

impl std::fmt::Display for ParseResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown resource: {}", self.0)
    }
}

impl std::error::Error for ParseResourceError {}

/// A nation's current reserves of each strategic resource.
/// Values are non-negative; going below zero clamps to zero and sets a deficit
/// flag for the next economy tick to react to.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct Stockpile {
    pub fuel: f32,
    pub metals: f32,
    pub tech: f32,
    pub food: f32,
}

impl Stockpile {
    pub fn new(fuel: f32, metals: f32, tech: f32, food: f32) -> Self {
        Self { fuel, metals, tech, food }
    }

    pub fn get(&self, r: Resource) -> f32 {
        match r {
            Resource::Fuel => self.fuel,
            Resource::Metals => self.metals,
            Resource::Tech => self.tech,
            Resource::Food => self.food,
        }
    }

    pub fn set(&mut self, r: Resource, value: f32) {
        let v = value.max(0.0);
        match r {
            Resource::Fuel => self.fuel = v,
            Resource::Metals => self.metals = v,
            Resource::Tech => self.tech = v,
            Resource::Food => self.food = v,
        }
    }

    /// Add `amount` of `r`. Negative amounts subtract but clamp at zero;
    /// returns true if the subtraction hit the floor (i.e. shortage).
    pub fn add(&mut self, r: Resource, amount: f32) -> bool {
        let current = self.get(r);
        let next = current + amount;
        if next < 0.0 {
            self.set(r, 0.0);
            true
        } else {
            self.set(r, next);
            false
        }
    }

    /// Attempt to pay `amount` of `r`. If reserves are insufficient, returns
    /// `false` and nothing is deducted.
    pub fn try_spend(&mut self, r: Resource, amount: f32) -> bool {
        if amount < 0.0 {
            return false;
        }
        if self.get(r) < amount {
            return false;
        }
        self.set(r, self.get(r) - amount);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_from_str_roundtrip() {
        for r in Resource::ALL {
            assert_eq!(Resource::from_str(r.as_str()).unwrap(), r);
        }
        assert!(Resource::from_str("Gold").is_err());
    }

    #[test]
    fn stockpile_clamps_at_zero() {
        let mut s = Stockpile::new(10.0, 0.0, 0.0, 0.0);
        let shortage = s.add(Resource::Fuel, -20.0);
        assert!(shortage);
        assert_eq!(s.fuel, 0.0);
    }

    #[test]
    fn stockpile_try_spend_rejects_overdraft() {
        let mut s = Stockpile::new(5.0, 0.0, 0.0, 0.0);
        assert!(!s.try_spend(Resource::Fuel, 10.0));
        assert_eq!(s.fuel, 5.0); // unchanged
        assert!(s.try_spend(Resource::Fuel, 3.0));
        assert_eq!(s.fuel, 2.0);
    }
}
