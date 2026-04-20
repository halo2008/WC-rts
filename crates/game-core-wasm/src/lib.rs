use wasm_bindgen::prelude::*;
use game_core::{Hex, StrategicGrid, HexCell, hex_to_pixel, pixel_to_hex};
use game_core::{astar, line_of_sight};
use game_core::Terrain;
use game_core::{
    BattlefieldGenerator, BattlefieldConfig, BattlefieldInstance,
    BuildingType, Building,
};
use std::cell::RefCell;
use std::str::FromStr;
use std::sync::OnceLock;

/// Global strategic grid stored in WASM memory. Set-once; OnceLock is fine here.
static GRID: OnceLock<StrategicGrid> = OnceLock::new();

thread_local! {
    /// Current battlefield instance. Wrapped in `RefCell<Option<_>>` so we can
    /// replace it on every `init_battlefield` call (zooming into a different hex).
    static BATTLEFIELD: RefCell<Option<BattlefieldInstance>> = const { RefCell::new(None) };
}

fn get_grid() -> &'static StrategicGrid {
    GRID.get().expect("grid not initialized — call init_grid first")
}

/// Borrow the current battlefield for reading. Returns `None` if uninitialized.
fn with_battlefield<T>(f: impl FnOnce(&BattlefieldInstance) -> T) -> Option<T> {
    BATTLEFIELD.with(|bf| bf.borrow().as_ref().map(f))
}

/// Borrow the current battlefield for mutation. Returns `None` if uninitialized.
fn with_battlefield_mut<T>(f: impl FnOnce(&mut BattlefieldInstance) -> T) -> Option<T> {
    BATTLEFIELD.with(|bf| bf.borrow_mut().as_mut().map(f))
}

// ─── Strategic Map Bindings (Etap 1) ────────────────────────────────

#[wasm_bindgen]
pub fn init_grid(width: i32, height: i32) {
    GRID.get_or_init(|| StrategicGrid::new(width, height));
}

/// Initialise the strategic grid by sampling a flat-earth PNG decoded in JS
/// (expects packed RGB bytes, row-major, length = `iw * ih * 3`). Same
/// classification as the server seed tool — hex terrain in the browser
/// matches what the server stores in `hex_map`.
#[wasm_bindgen]
pub fn init_grid_from_map(width: i32, height: i32, pixels: &[u8], iw: u32, ih: u32) {
    GRID.get_or_init(|| StrategicGrid::from_map_rgb(width, height, pixels, iw, ih));
}

#[wasm_bindgen]
pub fn get_viewport_hexes(
    cam_x: f32,
    cam_y: f32,
    view_width: f32,
    view_height: f32,
    hex_size: f32,
) -> JsValue {
    let grid = get_grid();
    let cells = grid.viewport(cam_x, cam_y, view_width, view_height, hex_size);
    let js_cells: Vec<JsHexCell> = cells
        .iter()
        .map(|vc| JsHexCell::from_viewport(vc.cell, vc.render_q, hex_size))
        .collect();
    serde_wasm_bindgen::to_value(&js_cells).unwrap()
}

#[wasm_bindgen]
pub fn hex_distance(q1: i32, r1: i32, q2: i32, r2: i32) -> i32 {
    Hex::new(q1, r1).distance(Hex::new(q2, r2))
}

#[wasm_bindgen]
pub fn hex_neighbors(q: i32, r: i32) -> JsValue {
    let hex = Hex::new(q, r);
    let neighbors = hex.neighbors();
    let js: Vec<JsHex> = neighbors.iter().map(|h| JsHex { q: h.q, r: h.r }).collect();
    serde_wasm_bindgen::to_value(&js).unwrap()
}

#[wasm_bindgen]
pub fn pixel_to_hex_wrapped(px: f32, py: f32, hex_size: f32, width: i32) -> JsValue {
    let hex = pixel_to_hex(px, py, hex_size);
    let wrapped = hex.wrap(width);
    let js = JsHex { q: wrapped.q, r: wrapped.r };
    serde_wasm_bindgen::to_value(&js).unwrap()
}

#[wasm_bindgen]
pub fn get_hex_info(px: f32, py: f32, hex_size: f32) -> JsValue {
    let grid = get_grid();
    if let Some(cell) = grid.get_hex_at_pixel(px, py, hex_size) {
        let js_cell = JsHexCell::from_cell(cell, hex_size);
        serde_wasm_bindgen::to_value(&js_cell).unwrap()
    } else {
        JsValue::NULL
    }
}

#[wasm_bindgen]
pub fn get_hex_by_coords(q: i32, r: i32) -> JsValue {
    let grid = get_grid();
    if let Some(cell) = grid.get(q, r) {
        let js_cell = JsHexCell::from_cell(cell, 4.0); // default hex_size
        serde_wasm_bindgen::to_value(&js_cell).unwrap()
    } else {
        JsValue::NULL
    }
}

/// A* pathfinding on the strategic map.
/// Returns array of {q, r} hexes from start to goal, or empty array if no path.
#[wasm_bindgen]
pub fn pathfind(
    start_q: i32,
    start_r: i32,
    goal_q: i32,
    goal_r: i32,
    width: i32,
) -> JsValue {
    let grid = get_grid();
    let start = Hex::new(start_q, start_r);
    let goal = Hex::new(goal_q, goal_r);

    let is_passable = |hex: Hex| -> bool {
        grid.get(hex.q, hex.r)
            .map(|c| c.terrain.is_passable_land())
            .unwrap_or(false)
    };

    let move_cost = |hex: Hex| -> f32 {
        grid.get(hex.q, hex.r)
            .map(|c| c.terrain.movement_cost())
            .unwrap_or(f32::INFINITY)
    };

    let path = astar(start, goal, is_passable, move_cost, Some(width));
    let js_path: Vec<JsHex> = path
        .unwrap_or_default()
        .into_iter()
        .map(|h| JsHex { q: h.q, r: h.r })
        .collect();
    serde_wasm_bindgen::to_value(&js_path).unwrap()
}

/// Line-of-sight check between two hexes on the strategic map.
/// Returns true if LOS is clear (no mountains blocking).
#[wasm_bindgen]
pub fn los_check(from_q: i32, from_r: i32, to_q: i32, to_r: i32) -> bool {
    let grid = get_grid();
    let from = Hex::new(from_q, from_r);
    let to = Hex::new(to_q, to_r);

    let is_blocking = |hex: Hex| -> bool {
        grid.get(hex.q, hex.r)
            .map(|c| matches!(c.terrain, Terrain::Mountain))
            .unwrap_or(false)
    };

    line_of_sight(from, to, is_blocking)
}

/// Get all hexes reachable from a given hex within a movement budget.
/// Returns array of {q, r, cost} objects.
#[wasm_bindgen]
pub fn reachable_hexes(
    start_q: i32,
    start_r: i32,
    budget: f32,
    width: i32,
) -> JsValue {
    let grid = get_grid();
    let start = Hex::new(start_q, start_r);

    let is_passable = |hex: Hex| -> bool {
        grid.get(hex.q, hex.r)
            .map(|c| c.terrain.is_passable_land())
            .unwrap_or(false)
    };

    let move_cost = |hex: Hex| -> f32 {
        grid.get(hex.q, hex.r)
            .map(|c| c.terrain.movement_cost())
            .unwrap_or(f32::INFINITY)
    };

    let reachable = game_core::reachable_hexes(start, budget, is_passable, move_cost, Some(width));
    let js_reachable: Vec<JsReachableHex> = reachable
        .into_iter()
        .map(|(h, cost)| JsReachableHex { q: h.q, r: h.r, cost })
        .collect();
    serde_wasm_bindgen::to_value(&js_reachable).unwrap()
}

// ─── Battlefield Bindings (Etap 2) ─────────────────────────────────

/// Initialize (or replace) the active battlefield instance for a strategic hex.
/// Generates the battlefield terrain deterministically from the seed.
#[wasm_bindgen]
pub fn init_battlefield(
    strategic_q: i32,
    strategic_r: i32,
    size: i32,
    seed: u32,
    macro_terrain: &str,
    macro_elevation: i32,
) -> JsValue {
    let terrain = Terrain::from_str(macro_terrain).unwrap_or(Terrain::Plains);
    let config = BattlefieldConfig {
        width: size,
        height: size,
        seed: seed as u64,
        strategic_q,
        strategic_r,
        macro_terrain: terrain,
        macro_elevation,
    };

    let instance = BattlefieldGenerator::generate(config);

    let info = JsBattlefieldInfo {
        strategic_q,
        strategic_r,
        width: instance.width,
        height: instance.height,
        seed: instance.seed,
        status: format!("{:?}", instance.status),
        macro_terrain: terrain.as_str().to_string(),
        buildings_count: instance.buildings.len(),
        deposits_count: instance.deposits.len(),
    };

    BATTLEFIELD.with(|bf| {
        *bf.borrow_mut() = Some(instance);
    });

    serde_wasm_bindgen::to_value(&info).unwrap()
}

/// Get battlefield viewport cells for rendering.
#[wasm_bindgen]
pub fn get_battlefield_viewport(
    cam_x: f32,
    cam_y: f32,
    view_width: f32,
    view_height: f32,
    hex_size: f32,
) -> JsValue {
    with_battlefield(|instance| {
        let cells = instance.viewport(cam_x, cam_y, view_width, view_height, hex_size);
        let js_cells: Vec<JsBattlefieldCell> = cells
            .iter()
            .map(|c| JsBattlefieldCell::from_cell(c, hex_size))
            .collect();
        serde_wasm_bindgen::to_value(&js_cells).unwrap()
    })
    .unwrap_or(JsValue::NULL)
}

/// Place a building on the battlefield. Returns building ID or -1 on failure.
#[wasm_bindgen]
pub fn place_building(
    building_type: &str,
    hex_q: i32,
    hex_r: i32,
) -> i64 {
    let Ok(bt) = BuildingType::from_str(building_type) else {
        return -1;
    };

    with_battlefield_mut(|instance| {
        let next_id = instance.buildings.len() as u64 + 1;
        let building = Building::new(next_id, bt, Hex::new(hex_q, hex_r));
        match instance.place_building(building) {
            Some(id) => id as i64,
            None => -1,
        }
    })
    .unwrap_or(-1)
}

/// Get building info at a specific hex on the battlefield.
#[wasm_bindgen]
pub fn get_building_info(hex_q: i32, hex_r: i32) -> JsValue {
    with_battlefield(|instance| {
        let buildings = instance.get_buildings_at(hex_q, hex_r);
        if buildings.is_empty() {
            return JsValue::NULL;
        }
        let js_buildings: Vec<JsBuilding> = buildings
            .iter()
            .map(|b| JsBuilding::from_building(b))
            .collect();
        serde_wasm_bindgen::to_value(&js_buildings).unwrap()
    })
    .unwrap_or(JsValue::NULL)
}

/// Get all buildings on the current battlefield.
#[wasm_bindgen]
pub fn get_all_buildings() -> JsValue {
    with_battlefield(|instance| {
        let js_buildings: Vec<JsBuilding> = instance
            .buildings
            .iter()
            .map(|b| JsBuilding::from_building(b))
            .collect();
        serde_wasm_bindgen::to_value(&js_buildings).unwrap()
    })
    .unwrap_or(JsValue::NULL)
}

/// Get all resource deposits on the current battlefield.
#[wasm_bindgen]
pub fn get_all_deposits() -> JsValue {
    with_battlefield(|instance| {
        let js_deposits: Vec<JsDeposit> = instance
            .deposits
            .iter()
            .map(|d| JsDeposit {
                deposit_type: d.deposit_type.as_str().to_string(),
                hex_q: d.hex_q,
                hex_r: d.hex_r,
                richness: d.richness,
            })
            .collect();
        serde_wasm_bindgen::to_value(&js_deposits).unwrap()
    })
    .unwrap_or(JsValue::NULL)
}

/// Get battlefield metadata.
#[wasm_bindgen]
pub fn get_battlefield_info() -> JsValue {
    with_battlefield(|instance| {
        let info = JsBattlefieldInfo {
            strategic_q: instance.strategic_q,
            strategic_r: instance.strategic_r,
            width: instance.width,
            height: instance.height,
            seed: instance.seed,
            status: format!("{:?}", instance.status),
            macro_terrain: instance.macro_terrain.as_str().to_string(),
            buildings_count: instance.buildings.len(),
            deposits_count: instance.deposits.len(),
        };
        serde_wasm_bindgen::to_value(&info).unwrap()
    })
    .unwrap_or(JsValue::NULL)
}

/// Get the list of all available building types with their properties.
#[wasm_bindgen]
pub fn get_building_types() -> JsValue {
    let js_types: Vec<JsBuildingTypeInfo> = BuildingType::ALL
        .iter()
        .copied()
        .map(|bt| JsBuildingTypeInfo {
            name: bt.as_str().to_string(),
            category: format!("{:?}", bt.category()),
            build_time: bt.build_time(),
            max_level: bt.max_level(),
            base_hp: bt.base_hp(),
            power_balance: bt.power_balance(),
            is_fortification: bt.is_fortification(),
            is_extraction: bt.is_extraction(),
            color: bt.color_rgb(),
            label: bt.label().to_string(),
        })
        .collect();
    serde_wasm_bindgen::to_value(&js_types).unwrap()
}

// ─── JS Serialization Types ────────────────────────────────────────

#[derive(serde::Serialize)]
struct JsHex {
    q: i32,
    r: i32,
}

#[derive(serde::Serialize)]
struct JsReachableHex {
    q: i32,
    r: i32,
    cost: f32,
}

#[derive(serde::Serialize)]
struct JsHexCell {
    q: i32,
    r: i32,
    terrain: String,
    color: [u8; 3],
    elevation: i32,
    movement_cost: f32,
    px: f32,
    py: f32,
}

impl JsHexCell {
    fn from_cell(cell: &HexCell, hex_size: f32) -> Self {
        Self::from_viewport(cell, cell.hex.q, hex_size)
    }

    /// Build a JsHexCell for a viewport render, using `render_q` (possibly
    /// outside `[0, width)`) so the pixel position reflects the correct
    /// wrap-around copy. `q`/`r` stay canonical so the frontend can still
    /// match hexes by identity.
    fn from_viewport(cell: &HexCell, render_q: i32, hex_size: f32) -> Self {
        let (px, py) = hex_to_pixel(Hex::new(render_q, cell.hex.r), hex_size);
        let (r, g, b) = cell.terrain.color_rgb();
        JsHexCell {
            q: cell.hex.q,
            r: cell.hex.r,
            terrain: cell.terrain.as_str().to_string(),
            color: [r, g, b],
            elevation: cell.elevation,
            movement_cost: cell.terrain.movement_cost(),
            px,
            py,
        }
    }
}

#[derive(serde::Serialize)]
struct JsBattlefieldInfo {
    strategic_q: i32,
    strategic_r: i32,
    width: i32,
    height: i32,
    seed: u64,
    status: String,
    macro_terrain: String,
    buildings_count: usize,
    deposits_count: usize,
}

#[derive(serde::Serialize)]
struct JsBattlefieldCell {
    q: i32,
    r: i32,
    terrain: String,
    color: [u8; 3],
    elevation: i32,
    forest_density: f32,
    has_road: bool,
    has_river: bool,
    building_id: Option<u64>,
    deposit: Option<String>,
    cover_bonus: f32,
    movement_cost: f32,
    px: f32,
    py: f32,
}

impl JsBattlefieldCell {
    fn from_cell(cell: &game_core::BattlefieldCell, hex_size: f32) -> Self {
        let (px, py) = hex_to_pixel(cell.hex, hex_size);
        let (r, g, b) = cell.terrain.color_rgb();
        JsBattlefieldCell {
            q: cell.hex.q,
            r: cell.hex.r,
            terrain: cell.terrain.as_str().to_string(),
            color: [r, g, b],
            elevation: cell.elevation,
            forest_density: cell.forest_density,
            has_road: cell.has_road,
            has_river: cell.has_river,
            building_id: cell.building_id,
            deposit: cell.deposit.map(|d| d.as_str().to_string()),
            cover_bonus: cell.cover_bonus(),
            movement_cost: cell.movement_cost(),
            px,
            py,
        }
    }
}

#[derive(serde::Serialize)]
struct JsBuilding {
    id: u64,
    building_type: String,
    hex_q: i32,
    hex_r: i32,
    level: u8,
    hp: i32,
    max_hp: i32,
    is_active: bool,
    power_balance: i32,
    hp_fraction: f32,
    color: (u8, u8, u8),
    label: String,
}

impl JsBuilding {
    fn from_building(b: &Building) -> Self {
        JsBuilding {
            id: b.id,
            building_type: b.building_type.as_str().to_string(),
            hex_q: b.hex.q,
            hex_r: b.hex.r,
            level: b.level,
            hp: b.hp,
            max_hp: b.max_hp,
            is_active: b.is_active,
            power_balance: b.effective_power(),
            hp_fraction: b.hp_fraction(),
            color: b.building_type.color_rgb(),
            label: b.building_type.label().to_string(),
        }
    }
}

#[derive(serde::Serialize)]
struct JsDeposit {
    deposit_type: String,
    hex_q: i32,
    hex_r: i32,
    richness: f32,
}

#[derive(serde::Serialize)]
struct JsBuildingTypeInfo {
    name: String,
    category: String,
    build_time: f32,
    max_level: u8,
    base_hp: i32,
    power_balance: i32,
    is_fortification: bool,
    is_extraction: bool,
    color: (u8, u8, u8),
    label: String,
}
