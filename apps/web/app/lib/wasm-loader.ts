interface HexCell {
  q: number;
  r: number;
  terrain: string;
  color: [number, number, number];
  elevation: number;
  px: number;
  py: number;
}

interface ReachableHex {
  q: number;
  r: number;
  cost: number;
}

interface BattlefieldCell {
  q: number;
  r: number;
  terrain: string;
  color: [number, number, number];
  elevation: number;
  forest_density: number;
  has_road: boolean;
  has_river: boolean;
  building_id: number | null;
  deposit: string | null;
  cover_bonus: number;
  movement_cost: number;
  px: number;
  py: number;
}

interface BattlefieldInfo {
  strategic_q: number;
  strategic_r: number;
  width: number;
  height: number;
  seed: number;
  status: string;
  macro_terrain: string;
  buildings_count: number;
  deposits_count: number;
}

interface BuildingData {
  id: number;
  building_type: string;
  hex_q: number;
  hex_r: number;
  level: number;
  hp: number;
  max_hp: number;
  is_active: boolean;
  power_balance: number;
  hp_fraction: number;
  color: [number, number, number];
  label: string;
}

interface DepositData {
  deposit_type: string;
  hex_q: number;
  hex_r: number;
  richness: number;
}

interface BuildingTypeInfo {
  name: string;
  category: string;
  build_time: number;
  max_level: number;
  base_hp: number;
  power_balance: number;
  is_fortification: boolean;
  is_extraction: boolean;
  color: [number, number, number];
  label: string;
}

export interface WasmApi {
  // Strategic map (Etap 1)
  init_grid: (width: number, height: number) => void;
  get_viewport_hexes: (
    cam_x: number,
    cam_y: number,
    view_width: number,
    view_height: number,
    hex_size: number
  ) => HexCell[];
  hex_distance: (q1: number, r1: number, q2: number, r2: number) => number;
  pixel_to_hex_wrapped: (px: number, py: number, hex_size: number, width: number) => { q: number; r: number };
  get_hex_info: (px: number, py: number, hex_size: number) => HexCell | null;
  get_hex_by_coords: (q: number, r: number) => HexCell | null;
  hex_neighbors: (q: number, r: number) => { q: number; r: number }[];
  pathfind: (start_q: number, start_r: number, goal_q: number, goal_r: number, width: number) => { q: number; r: number }[];
  los_check: (from_q: number, from_r: number, to_q: number, to_r: number) => boolean;
  reachable_hexes: (start_q: number, start_r: number, budget: number, width: number) => ReachableHex[];

  // Battlefield (Etap 2)
  init_battlefield: (strategic_q: number, strategic_r: number, size: number, seed: number, macro_terrain: string, macro_elevation: number) => BattlefieldInfo;
  get_battlefield_viewport: (cam_x: number, cam_y: number, view_width: number, view_height: number, hex_size: number) => BattlefieldCell[];
  place_building: (building_type: string, hex_q: number, hex_r: number) => number;
  get_building_info: (hex_q: number, hex_r: number) => BuildingData[] | null;
  get_all_buildings: () => BuildingData[] | null;
  get_all_deposits: () => DepositData[] | null;
  get_battlefield_info: () => BattlefieldInfo | null;
  get_building_types: () => BuildingTypeInfo[];
}

let wasmReady: WasmApi | null = null;

export async function loadWasm(): Promise<WasmApi> {
  if (wasmReady) return wasmReady;

  // @ts-expect-error — WASM glue loaded at runtime from public/
  const wasmModule = await import(/* webpackIgnore: true */ "/wasm/game_core_wasm.js");

  // --target web: domyślny export = init function, reszta = funkcje po init
  await wasmModule.default();

  wasmReady = {
    init_grid: wasmModule.init_grid as (w: number, h: number) => void,
    get_viewport_hexes: (cam_x, cam_y, vw, vh, hs) =>
      wasmModule.get_viewport_hexes(cam_x, cam_y, vw, vh, hs) as unknown as HexCell[],
    hex_distance: wasmModule.hex_distance as (
      q1: number,
      r1: number,
      q2: number,
      r2: number
    ) => number,
    pixel_to_hex_wrapped: (px, py, hs, w) =>
      wasmModule.pixel_to_hex_wrapped(px, py, hs, w) as unknown as { q: number; r: number },
    get_hex_info: (px, py, hs) =>
      wasmModule.get_hex_info(px, py, hs) as unknown as HexCell | null,
    get_hex_by_coords: (q, r) =>
      wasmModule.get_hex_by_coords(q, r) as unknown as HexCell | null,
    hex_neighbors: (q, r) =>
      wasmModule.hex_neighbors(q, r) as unknown as { q: number; r: number }[],
    // Etap 1 bindings
    pathfind: (start_q, start_r, goal_q, goal_r, width) =>
      wasmModule.pathfind(start_q, start_r, goal_q, goal_r, width) as unknown as { q: number; r: number }[],
    los_check: (from_q, from_r, to_q, to_r) =>
      wasmModule.los_check(from_q, from_r, to_q, to_r) as boolean,
    reachable_hexes: (start_q, start_r, budget, width) =>
      wasmModule.reachable_hexes(start_q, start_r, budget, width) as unknown as ReachableHex[],

    // Etap 2 — Battlefield bindings
    init_battlefield: (sq, sr, size, seed, terrain, elev) =>
      wasmModule.init_battlefield(sq, sr, size, seed, terrain, elev) as unknown as BattlefieldInfo,
    get_battlefield_viewport: (cam_x, cam_y, vw, vh, hs) =>
      wasmModule.get_battlefield_viewport(cam_x, cam_y, vw, vh, hs) as unknown as BattlefieldCell[],
    place_building: (bt, hq, hr) =>
      wasmModule.place_building(bt, hq, hr) as number,
    get_building_info: (hq, hr) =>
      wasmModule.get_building_info(hq, hr) as unknown as BuildingData[] | null,
    get_all_buildings: () =>
      wasmModule.get_all_buildings() as unknown as BuildingData[] | null,
    get_all_deposits: () =>
      wasmModule.get_all_deposits() as unknown as DepositData[] | null,
    get_battlefield_info: () =>
      wasmModule.get_battlefield_info() as unknown as BattlefieldInfo | null,
    get_building_types: () =>
      wasmModule.get_building_types() as unknown as BuildingTypeInfo[],
  };

  return wasmReady;
}

// Re-export types for use in components
export type {
  HexCell,
  BattlefieldCell,
  BattlefieldInfo,
  BuildingData,
  DepositData,
  BuildingTypeInfo,
};
