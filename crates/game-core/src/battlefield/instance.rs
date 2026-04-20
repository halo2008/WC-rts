use serde::{Deserialize, Serialize};
use ts_rs::TS;
use crate::hex::Hex;
use crate::terrain::Terrain;
use crate::building::types::Building;
use crate::building::construction::ConstructionQueue;
use crate::building::extraction::{ResourceDeposit, DepositType};

/// Status of a battlefield instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum BattlefieldStatus {
    /// No combat — calm, tick at 1Hz.
    Calm,
    /// Active combat — tick at 20Hz.
    Active,
}

/// A single cell in the battlefield grid.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BattlefieldCell {
    pub hex: Hex,
    pub terrain: Terrain,
    pub elevation: i32,
    /// Forest density 0.0..1.0.
    pub forest_density: f32,
    pub has_road: bool,
    pub has_river: bool,
    /// Building ID if a building is placed here.
    pub building_id: Option<u64>,
    /// Resource deposit on this hex.
    pub deposit: Option<DepositType>,
}

impl BattlefieldCell {
    /// Movement cost for this cell, accounting for roads and rivers.
    pub fn movement_cost(&self) -> f32 {
        let base = self.terrain.movement_cost();
        if base == f32::INFINITY {
            return f32::INFINITY;
        }
        let mut cost = base;
        if self.has_road {
            cost *= 0.5; // Roads halve movement cost
        }
        if self.has_river {
            cost *= 1.5; // River crossing is slower
        }
        cost
    }

    /// Cover bonus from terrain and forest.
    pub fn cover_bonus(&self) -> f32 {
        let mut cover: f32 = 0.0;
        if self.terrain == Terrain::Forest || self.forest_density > 0.5 {
            cover += 0.3;
        }
        if self.terrain == Terrain::Urban {
            cover += 0.4;
        }
        if self.terrain == Terrain::Hills {
            cover += 0.2;
        }
        if self.terrain == Terrain::Mountain {
            cover += 0.3;
        }
        cover.min(0.7)
    }
}

/// A battlefield instance — the tactical map for a single strategic hex.
/// Lazy-generated when a player first zooms in, and persists afterwards.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BattlefieldInstance {
    /// Strategic hex coordinates this battlefield belongs to.
    pub strategic_q: i32,
    pub strategic_r: i32,
    /// Battlefield grid dimensions.
    pub width: i32,
    pub height: i32,
    /// Seed used for generation (for deterministic regeneration).
    pub seed: u64,
    /// All cells in the battlefield grid.
    pub cells: Vec<BattlefieldCell>,
    /// Current status.
    pub status: BattlefieldStatus,
    /// Buildings placed on this battlefield.
    pub buildings: Vec<Building>,
    /// Resource deposits.
    pub deposits: Vec<ResourceDeposit>,
    /// Construction queue.
    pub construction_queue: ConstructionQueue,
    /// Macro terrain from strategic map.
    pub macro_terrain: Terrain,
    /// Number of viewers currently watching this battlefield.
    pub viewer_count: u32,
}

impl BattlefieldInstance {
    /// Create a new battlefield instance.
    pub fn new(
        strategic_q: i32,
        strategic_r: i32,
        width: i32,
        height: i32,
        seed: u64,
        cells: Vec<BattlefieldCell>,
        deposits: Vec<(i32, i32, DepositType, f32)>,
        macro_terrain: Terrain,
    ) -> Self {
        // Assign deposits to cells
        let mut cells = cells;
        for (dq, dr, dtype, _richness) in &deposits {
            if let Some(cell) = cells.iter_mut().find(|c| c.hex.q == *dq && c.hex.r == *dr) {
                cell.deposit = Some(*dtype);
            }
        }

        let resource_deposits: Vec<ResourceDeposit> = deposits
            .into_iter()
            .map(|(q, r, dtype, richness)| ResourceDeposit::new(dtype, q, r, richness))
            .collect();

        Self {
            strategic_q,
            strategic_r,
            width,
            height,
            seed,
            cells,
            status: BattlefieldStatus::Calm,
            buildings: Vec::new(),
            deposits: resource_deposits,
            construction_queue: ConstructionQueue::new(3),
            macro_terrain,
            viewer_count: 0,
        }
    }

    /// Get a cell by hex coordinates.
    pub fn get_cell(&self, q: i32, r: i32) -> Option<&BattlefieldCell> {
        if q < 0 || q >= self.width || r < 0 || r >= self.height {
            return None;
        }
        let idx = (r * self.width + q) as usize;
        self.cells.get(idx)
    }

    /// Get a mutable cell by hex coordinates.
    pub fn get_cell_mut(&mut self, q: i32, r: i32) -> Option<&mut BattlefieldCell> {
        if q < 0 || q >= self.width || r < 0 || r >= self.height {
            return None;
        }
        let idx = (r * self.width + q) as usize;
        self.cells.get_mut(idx)
    }

    /// Get viewport cells for rendering.
    pub fn viewport(
        &self,
        cam_x: f32,
        cam_y: f32,
        view_width: f32,
        view_height: f32,
        hex_size: f32,
    ) -> Vec<&BattlefieldCell> {
        let sq3 = 3.0_f32.sqrt();
        let hex_w = sq3 * hex_size;
        // Pointy-top: row pitch is 1.5 * size, not 2.0.
        let row_h = 1.5 * hex_size;

        let min_r = ((cam_y - view_height / 2.0) / row_h).floor() as i32 - 1;
        let max_r = ((cam_y + view_height / 2.0) / row_h).ceil() as i32 + 1;

        let mut result = Vec::new();
        for r in min_r.max(0)..=max_r.min(self.height - 1) {
            // Row r is shifted right by r/2 columns in pointy-top.
            let offset = r as f32 / 2.0;
            let min_q = ((cam_x - view_width / 2.0) / hex_w - offset).floor() as i32 - 1;
            let max_q = ((cam_x + view_width / 2.0) / hex_w - offset).ceil() as i32 + 1;
            for q in min_q.max(0)..=max_q.min(self.width - 1) {
                if let Some(cell) = self.get_cell(q, r) {
                    let (px, py) = crate::hex::hex_to_pixel(cell.hex, hex_size);
                    if px >= cam_x - view_width / 2.0 - hex_w
                        && px <= cam_x + view_width / 2.0 + hex_w
                        && py >= cam_y - view_height / 2.0 - row_h
                        && py <= cam_y + view_height / 2.0 + row_h
                    {
                        result.push(cell);
                    }
                }
            }
        }
        result
    }

    /// Place a building on the battlefield. Returns the building ID or None if hex occupied.
    pub fn place_building(&mut self, building: Building) -> Option<u64> {
        let q = building.hex.q;
        let r = building.hex.r;

        // Check if hex already has a building
        if self.buildings.iter().any(|b| b.hex.q == q && b.hex.r == r) {
            return None;
        }

        // Check if terrain is buildable
        if let Some(cell) = self.get_cell(q, r) {
            if !cell.terrain.is_passable_land() {
                return None;
            }
        } else {
            return None;
        }

        let id = building.id;
        self.buildings.push(building);

        // Mark cell as having a building
        if let Some(cell) = self.get_cell_mut(q, r) {
            cell.building_id = Some(id);
        }

        Some(id)
    }

    /// Remove a building by ID.
    pub fn remove_building(&mut self, building_id: u64) -> bool {
        if let Some(pos) = self.buildings.iter().position(|b| b.id == building_id) {
            let building = self.buildings.remove(pos);
            // Clear cell
            if let Some(cell) = self.get_cell_mut(building.hex.q, building.hex.r) {
                cell.building_id = None;
            }
            true
        } else {
            false
        }
    }

    /// Get a building by ID.
    pub fn get_building(&self, id: u64) -> Option<&Building> {
        self.buildings.iter().find(|b| b.id == id)
    }

    /// Get a mutable building by ID.
    pub fn get_building_mut(&mut self, id: u64) -> Option<&mut Building> {
        self.buildings.iter_mut().find(|b| b.id == id)
    }

    /// Get buildings at a specific hex.
    pub fn get_buildings_at(&self, q: i32, r: i32) -> Vec<&Building> {
        self.buildings.iter().filter(|b| b.hex.q == q && b.hex.r == r).collect()
    }

    /// Tick the construction queue and place completed buildings.
    pub fn tick_construction(&mut self, dt: f32) {
        let completed = self.construction_queue.tick(dt);
        for building in completed {
            self.place_building(building);
        }
    }

    /// Calculate total power balance for all buildings.
    pub fn power_balance(&self) -> i32 {
        self.buildings.iter().map(|b| b.effective_power()).sum()
    }

    /// Add a viewer (player zoomed in).
    pub fn add_viewer(&mut self) {
        self.viewer_count += 1;
    }

    /// Remove a viewer (player zoomed out).
    pub fn remove_viewer(&mut self) {
        if self.viewer_count > 0 {
            self.viewer_count -= 1;
        }
    }

    /// Whether this instance should be kept in memory.
    pub fn is_active(&self) -> bool {
        self.viewer_count > 0 || self.status == BattlefieldStatus::Active
    }

    /// Get resource deposits on this battlefield.
    pub fn get_deposits(&self) -> &[ResourceDeposit] {
        &self.deposits
    }
}
