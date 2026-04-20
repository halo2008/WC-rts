pub mod hex;
pub mod terrain;
pub mod grid;
pub mod map_sample;
pub mod pathfinding;
pub mod spatial;
pub mod building;
pub mod battlefield;
pub mod economy;
pub mod military;
pub mod nation;
pub mod combat;

pub use hex::{Hex, hex_to_pixel, pixel_to_hex};
pub use terrain::{Terrain, ParseTerrainError};
pub use grid::{HexCell, StrategicGrid, ViewportCell};
pub use map_sample::{classify_pixel, sample_grid_from_map, terrain_elevation};
pub use pathfinding::{astar, astar_limited, reachable_hexes};
pub use spatial::{HexIndex, line_of_sight, line_of_sight_elevated};

// Building system
pub use building::{
    BuildingType, Building, BuildingId, BuildingCategory, ParseBuildingTypeError,
    ConstructionQueue, ConstructionOrder, ConstructionStatus,
    FortificationType, Fortification, DefenseBonus,
    ResourceDeposit, ExtractionBuilding, ExtractionRate, DepositType, ParseDepositTypeError,
};

// Battlefield system
pub use battlefield::{
    BattlefieldGenerator, BattlefieldConfig,
    BattlefieldInstance, BattlefieldStatus, BattlefieldCell,
};

// Economy (Etap 3)
pub use economy::{Resource, Stockpile, ParseResourceError};

// Military (Etap 3)
pub use military::{
    UnitType, Unit, UnitId, UnitDomain, Stack, ParseUnitTypeError, UnitStats,
    Transit, TransitStatus,
    UnitGroup, GroupId,
};

// Nations (Etap 3)
pub use nation::{Nation, NationState, BudgetAllocation};

// Combat (Etap 4 — auto-resolve)
pub use combat::{autoresolve, CombatOutcome, CombatUnit, UnitDamage, terrain_cover};
