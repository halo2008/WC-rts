pub mod types;
pub mod construction;
pub mod fortification;
pub mod extraction;

pub use types::{BuildingType, Building, BuildingId, BuildingCategory, ParseBuildingTypeError};
pub use construction::{ConstructionQueue, ConstructionOrder, ConstructionStatus};
pub use fortification::{FortificationType, Fortification, DefenseBonus};
pub use extraction::{ResourceDeposit, ExtractionBuilding, ExtractionRate, DepositType, ParseDepositTypeError};
