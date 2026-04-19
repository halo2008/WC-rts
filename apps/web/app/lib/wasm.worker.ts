/**
 * Web Worker for game-core WASM — offloads pathfinding, LOS checks,
 * and spatial queries from the main thread.
 *
 * Usage:
 *   const worker = new Worker(new URL('./wasm.worker.ts', import.meta.url), { type: 'module' });
 *   worker.postMessage({ type: 'init', width: 1200, height: 600 });
 *   worker.postMessage({ type: 'pathfind', startQ: 0, startR: 0, goalQ: 100, goalR: 50, width: 1200 });
 *   worker.onmessage = (e) => { ... };
 */

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

interface WasmModule {
  init_grid: (width: number, height: number) => void;
  get_viewport_hexes: (cam_x: number, cam_y: number, view_width: number, view_height: number, hex_size: number) => HexCell[];
  hex_distance: (q1: number, r1: number, q2: number, r2: number) => number;
  pixel_to_hex_wrapped: (px: number, py: number, hex_size: number, width: number) => { q: number; r: number };
  get_hex_info: (px: number, py: number, hex_size: number) => HexCell | null;
  get_hex_by_coords: (q: number, r: number) => HexCell | null;
  hex_neighbors: (q: number, r: number) => { q: number; r: number }[];
  pathfind: (start_q: number, start_r: number, goal_q: number, goal_r: number, width: number) => { q: number; r: number }[];
  los_check: (from_q: number, from_r: number, to_q: number, to_r: number) => boolean;
  reachable_hexes: (start_q: number, start_r: number, budget: number, width: number) => ReachableHex[];
  default: () => Promise<void>;
}

let wasm: WasmModule | null = null;

async function initWasm() {
  if (wasm) return;

  // @ts-expect-error — WASM loaded at runtime
  const wasmModule = await import(/* webpackIgnore: true */ "/wasm/game_core_wasm.js");
  await wasmModule.default();
  wasm = wasmModule as unknown as WasmModule;
}

// Message handler
self.onmessage = async (e: MessageEvent) => {
  const { type, id } = e.data;

  try {
    switch (type) {
      case "init": {
        await initWasm();
        if (wasm) {
          wasm.init_grid(e.data.width, e.data.height);
        }
        self.postMessage({ type: "init_done", id });
        break;
      }

      case "pathfind": {
        if (!wasm) {
          self.postMessage({ type: "error", id, error: "WASM not initialized" });
          break;
        }
        const path = wasm.pathfind(
          e.data.startQ,
          e.data.startR,
          e.data.goalQ,
          e.data.goalR,
          e.data.width
        );
        self.postMessage({ type: "pathfind_result", id, path });
        break;
      }

      case "los_check": {
        if (!wasm) {
          self.postMessage({ type: "error", id, error: "WASM not initialized" });
          break;
        }
        const hasLos = wasm.los_check(
          e.data.fromQ,
          e.data.fromR,
          e.data.toQ,
          e.data.toR
        );
        self.postMessage({ type: "los_result", id, hasLos });
        break;
      }

      case "reachable_hexes": {
        if (!wasm) {
          self.postMessage({ type: "error", id, error: "WASM not initialized" });
          break;
        }
        const reachable = wasm.reachable_hexes(
          e.data.startQ,
          e.data.startR,
          e.data.budget,
          e.data.width
        );
        self.postMessage({ type: "reachable_result", id, reachable });
        break;
      }

      case "get_viewport_hexes": {
        if (!wasm) {
          self.postMessage({ type: "error", id, error: "WASM not initialized" });
          break;
        }
        const hexes = wasm.get_viewport_hexes(
          e.data.camX,
          e.data.camY,
          e.data.viewWidth,
          e.data.viewHeight,
          e.data.hexSize
        );
        self.postMessage({ type: "viewport_result", id, hexes });
        break;
      }

      case "hex_distance": {
        if (!wasm) {
          self.postMessage({ type: "error", id, error: "WASM not initialized" });
          break;
        }
        const distance = wasm.hex_distance(
          e.data.q1,
          e.data.r1,
          e.data.q2,
          e.data.r2
        );
        self.postMessage({ type: "distance_result", id, distance });
        break;
      }

      default:
        self.postMessage({ type: "error", id, error: `Unknown message type: ${type}` });
    }
  } catch (err) {
    self.postMessage({ type: "error", id, error: String(err) });
  }
};

export {};
