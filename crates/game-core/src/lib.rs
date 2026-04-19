pub mod hex;
pub mod terrain;
pub mod grid;
pub mod pathfinding;
pub mod spatial;
pub mod building;
pub mod battlefield;

pub use hex::{Hex, hex_to_pixel, pixel_to_hex};
pub use terrain::Terrain;
pub use grid::{HexCell, StrategicGrid};
pub use pathfinding::{astar, astar_limited, reachable_hexes};
pub use spatial::{HexIndex, line_of_sight, line_of_sight_elevated};

// Building system
pub use building::{
    BuildingType, Building, BuildingId, BuildingCategory,
    ConstructionQueue, ConstructionOrder, ConstructionStatus,
    FortificationType, Fortification, DefenseBonus,
    ResourceDeposit, ExtractionBuilding, ExtractionRate, extraction::DepositType,
};

// Battlefield system
pub use battlefield::{
    BattlefieldGenerator, BattlefieldConfig,
    BattlefieldInstance, BattlefieldStatus, BattlefieldCell,
};
