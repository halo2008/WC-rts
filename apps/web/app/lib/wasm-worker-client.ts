/**
 * Promise-based client for the WASM Web Worker.
 *
 * Matches the message protocol defined in `wasm.worker.ts`. Use for
 * off-main-thread pathfinding, reachable-hex queries, and LOS checks on the
 * strategic map. Viewport queries stay on the main thread — they run every
 * frame and the postMessage round-trip dwarfs the WASM call itself.
 */

export interface HexPoint {
  q: number;
  r: number;
}

export interface ReachableHex extends HexPoint {
  cost: number;
}

type PendingCall = {
  resolve: (value: unknown) => void;
  reject: (err: Error) => void;
};

export class WasmWorkerClient {
  private worker: Worker;
  private pending = new Map<number, PendingCall>();
  private nextId = 1;

  constructor() {
    this.worker = new Worker(
      new URL("./wasm.worker.ts", import.meta.url),
      { type: "module" },
    );
    this.worker.onmessage = this.handleMessage;
    this.worker.onerror = (e) => {
      console.error("WASM worker error:", e);
    };
  }

  private handleMessage = (e: MessageEvent) => {
    const { id, type, error } = e.data;
    const call = this.pending.get(id);
    if (!call) return;
    this.pending.delete(id);

    if (type === "error") {
      call.reject(new Error(String(error)));
      return;
    }

    // Each response type carries its payload under a well-known key.
    switch (type) {
      case "init_done":
        call.resolve(undefined);
        break;
      case "pathfind_result":
        call.resolve(e.data.path);
        break;
      case "los_result":
        call.resolve(e.data.hasLos);
        break;
      case "reachable_result":
        call.resolve(e.data.reachable);
        break;
      case "viewport_result":
        call.resolve(e.data.hexes);
        break;
      case "distance_result":
        call.resolve(e.data.distance);
        break;
      default:
        call.reject(new Error(`Unknown response type: ${type}`));
    }
  };

  private send<T>(payload: Record<string, unknown>): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: resolve as (v: unknown) => void,
        reject,
      });
      this.worker.postMessage({ id, ...payload });
    });
  }

  init(width: number, height: number): Promise<void> {
    return this.send({ type: "init", width, height });
  }

  pathfind(
    startQ: number,
    startR: number,
    goalQ: number,
    goalR: number,
    width: number,
  ): Promise<HexPoint[]> {
    return this.send({ type: "pathfind", startQ, startR, goalQ, goalR, width });
  }

  losCheck(fromQ: number, fromR: number, toQ: number, toR: number): Promise<boolean> {
    return this.send({ type: "los_check", fromQ, fromR, toQ, toR });
  }

  reachableHexes(
    startQ: number,
    startR: number,
    budget: number,
    width: number,
  ): Promise<ReachableHex[]> {
    return this.send({
      type: "reachable_hexes",
      startQ,
      startR,
      budget,
      width,
    });
  }

  hexDistance(q1: number, r1: number, q2: number, r2: number): Promise<number> {
    return this.send({ type: "hex_distance", q1, r1, q2, r2 });
  }

  terminate(): void {
    this.worker.terminate();
    for (const call of this.pending.values()) {
      call.reject(new Error("worker terminated"));
    }
    this.pending.clear();
  }
}
