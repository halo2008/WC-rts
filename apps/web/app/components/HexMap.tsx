"use client";

import { useEffect, useRef, useState } from "react";
import { Application, Container, Graphics, Text, TextStyle } from "pixi.js";
import { loadWasm, type HexCell, type WasmApi } from "../lib/wasm-loader";
import Minimap from "./Minimap";

// --- Constants ---
const GRID_WIDTH = 1200;
const GRID_HEIGHT = 600;
const MIN_HEX_SIZE = 2;
const MAX_HEX_SIZE = 20;
const DEFAULT_HEX_SIZE = 4;
const ZOOM_STEP = 1.15;
const DRAG_THRESHOLD = 5;
const SQRT3 = Math.sqrt(3);

/// Water terrains used for continent-edge outlining.
const WATER_TERRAINS = new Set(["DeepOcean", "Ocean"]);
/// Ice terrain — gets a bright white ice-wall outline in the render.
const ICE_TERRAIN = "Ice";

/// Neighbour offsets in pointy-top axial coords, ordered so `i`-th offset
/// corresponds to the hex edge between vertices `i` and `i+1` (starting at
/// 30° E-NE and going counter-clockwise). Used for edge detection.
const NEIGHBOR_DIRS: Array<[number, number]> = [
  [1, 0],    // E
  [1, -1],   // NE
  [0, -1],   // NW
  [-1, 0],   // W
  [-1, 1],   // SW
  [0, 1],    // SE
];

// Polish labels — kept on the frontend because they are i18n, not game state.
const TERRAIN_NAMES: Record<string, string> = {
  DeepOcean: "Głęboki Ocean",
  Ocean: "Ocean",
  Coast: "Wybrzeże",
  Plains: "Równiny",
  Forest: "Las",
  Hills: "Wzgórza",
  Mountain: "Góry",
  Desert: "Pustynia",
  Tundra: "Tundra",
  Urban: "Miasto",
  Ice: "Lód",
};

// --- Helper functions ---
function hexToPixel(
  q: number,
  r: number,
  size: number
): { x: number; y: number } {
  return {
    x: size * (SQRT3 * q + (SQRT3 / 2) * r),
    y: size * 1.5 * r,
  };
}

function drawHexPath(
  graphics: Graphics,
  cx: number,
  cy: number,
  size: number
) {
  graphics.beginPath();
  for (let i = 0; i < 6; i++) {
    const angle = (Math.PI / 180) * (60 * i - 30);
    const hx = cx + size * Math.cos(angle);
    const hy = cy + size * Math.sin(angle);
    if (i === 0) graphics.moveTo(hx, hy);
    else graphics.lineTo(hx, hy);
  }
  graphics.closePath();
}

/// Same as drawHexPath but DOES NOT call beginPath — use for batched fills
/// where we want one big path with many hex sub-paths, then a single fill().
/// In Pixi v8 `beginPath()` resets the whole path, so calling it per hex in a
/// batched loop throws away everything except the last hex.
function appendHexToPath(
  graphics: Graphics,
  cx: number,
  cy: number,
  size: number,
) {
  for (let i = 0; i < 6; i++) {
    const angle = (Math.PI / 180) * (60 * i - 30);
    const hx = cx + size * Math.cos(angle);
    const hy = cy + size * Math.sin(angle);
    if (i === 0) graphics.moveTo(hx, hy);
    else graphics.lineTo(hx, hy);
  }
  graphics.closePath();
}

// --- Component ---
export interface StrategicUnitMarker {
  id: number;
  hex_q: number;
  hex_r: number;
  color: [number, number, number];
  /// Optional live-interpolated position (fractional hex between two cells).
  /// If absent, we render at integer hex_q/hex_r.
  interp?: {
    from_q: number;
    from_r: number;
    to_q: number;
    to_r: number;
    fraction: number;
  };
}

/// An army group marker — bigger than a unit, carries a badge with the
/// member count. Rendered over the terrain layer.
export interface StrategicGroupMarker {
  id: number;
  hex_q: number;
  hex_r: number;
  color: [number, number, number];
  member_count: number;
  interp?: {
    from_q: number;
    from_r: number;
    to_q: number;
    to_r: number;
    fraction: number;
  };
}

/// Capital marker — renders the nation code as a flag-like label at its capital hex.
export interface CapitalMarker {
  nation_code: string;
  hex_q: number;
  hex_r: number;
  color: [number, number, number];
}

interface HexMapProps {
  onEnterBattlefield?: (q: number, r: number, terrain: string, elevation: number) => void;
  /// Units to draw on top of the terrain. Grouped into stacks per hex.
  units?: StrategicUnitMarker[];
  /// Army groups — drawn as larger markers with a count badge.
  groups?: StrategicGroupMarker[];
  /// National capitals — shown as coloured star markers with a label.
  capitals?: CapitalMarker[];
  /// Highlight this specific unit id with a selection ring.
  selectedUnitId?: number | null;
  /// Highlight this specific group id with a selection ring.
  selectedGroupId?: number | null;
  /// Fired on every non-drag click that resolved to a hex.
  onHexAction?: (q: number, r: number) => void;
}

export default function HexMap({
  onEnterBattlefield,
  units,
  groups,
  capitals,
  selectedUnitId,
  selectedGroupId,
  onHexAction,
}: HexMapProps) {
  // DOM / PixiJS refs
  const containerRef = useRef<HTMLDivElement>(null);
  const appRef = useRef<Application | null>(null);
  const graphicsRef = useRef<Graphics | null>(null);
  const highlightRef = useRef<Graphics | null>(null);
  const unitsLayerRef = useRef<Graphics | null>(null);
  const edgesLayerRef = useRef<Graphics | null>(null);
  // Latest viewport cells — cached so drawEdges() can look up terrain
  // without re-querying WASM every frame.
  const lastCellsRef = useRef<HexCell[]>([]);
  const lastPolarStrideRef = useRef<number>(1);

  // Props mirrored to refs so ticker closures always see latest values
  // without needing to re-run the setup effect.
  const unitsRef = useRef<StrategicUnitMarker[] | undefined>(units);
  const groupsRef = useRef<StrategicGroupMarker[] | undefined>(groups);
  const capitalsRef = useRef<CapitalMarker[] | undefined>(capitals);
  const selectedUnitIdRef = useRef<number | null | undefined>(selectedUnitId);
  const selectedGroupIdRef = useRef<number | null | undefined>(selectedGroupId);
  const onHexActionRef = useRef<typeof onHexAction>(onHexAction);
  const groupsLayerRef = useRef<Container | null>(null);
  const groupLabelPoolRef = useRef<Map<number, Text>>(new Map());
  const capitalsLayerRef = useRef<Container | null>(null);
  const capitalLabelPoolRef = useRef<Map<string, Text>>(new Map());

  // Camera state (refs — no React re-renders)
  const camX = useRef(0);
  const camY = useRef(3000);
  const hexSize = useRef(DEFAULT_HEX_SIZE);
  /// "flat" = scrollable rectangular map (classic strategy view).
  /// "polar" = UN-logo / flat-earth disc: north pole in the centre, south
  /// pole (ice wall) on the outer rim. In polar mode we ignore the camera
  /// and render the whole world centred on the viewport.
  const projection = useRef<"flat" | "polar">("flat");

  // Interaction state (React state for UI, refs for closures)
  const [hoveredHex, setHoveredHex] = useState<HexCell | null>(null);
  const [selectedHex, setSelectedHex] = useState<HexCell | null>(null);
  const [fps, setFps] = useState(0);
  const [hexCount, setHexCount] = useState(0);
  const [zoomLevel, setZoomLevel] = useState(DEFAULT_HEX_SIZE);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Camera state mirrored to React so the minimap can follow.
  const [camState, setCamState] = useState({ x: 0, y: 3000 });
  const [viewportSize, setViewportSize] = useState({ width: 0, height: 0 });
  const [showMinimap, setShowMinimap] = useState(true);
  const [projectionMode, setProjectionMode] = useState<"flat" | "polar">("flat");

  const hoveredHexRef = useRef<HexCell | null>(null);
  const selectedHexRef = useRef<HexCell | null>(null);

  // Dirty flags
  const hexDirty = useRef(true);
  const highlightDirty = useRef(true);
  const unitsDirty = useRef(true);
  const groupsDirty = useRef(true);
  const edgesDirty = useRef(true);
  const capitalsDirty = useRef(true);

  // Mouse state (refs)
  const mousePos = useRef({ x: 0, y: 0 });
  const mouseDownPos = useRef({ x: 0, y: 0 });
  const mouseIsDown = useRef(false);
  const isDragging = useRef(false);

  // WASM ref
  const wasmRef = useRef<WasmApi | null>(null);

  useEffect(() => {
    let mounted = true;
    const abortController = new AbortController();
    const { signal } = abortController;
    let resizeObserver: ResizeObserver | null = null;

    async function init() {
      let wasm: WasmApi;
      try {
        wasm = await loadWasm();
      } catch (e) {
        if (mounted) setError(String(e));
        console.error("WASM load failed:", e);
        return;
      }

      if (!mounted) return;
      wasmRef.current = wasm;

      wasm.init_grid(GRID_WIDTH, GRID_HEIGHT);

      const app = new Application();
      await app.init({
        background: "#0a1628",
        resizeTo: containerRef.current!,
        antialias: false,
        resolution: 1,
      });

      if (!mounted) {
        app.destroy({ removeView: true }, true);
        return;
      }

      // Clear any orphaned canvases left over from prior effect runs
      // (StrictMode double-invoke, HMR, React re-mount) before mounting ours.
      while (containerRef.current!.firstChild) {
        containerRef.current!.removeChild(containerRef.current!.firstChild);
      }
      containerRef.current!.appendChild(app.canvas as HTMLCanvasElement);
      appRef.current = app;

      // Six layers, bottom-up: terrain → edges → capitals → units → groups → highlights.
      // Capitals sit *under* units so a friendly infantry stack on the capital
      // hex doesn't get hidden by its own flag disc.
      const graphics = new Graphics();
      const edgesLayer = new Graphics();
      const capitalsLayer = new Container();
      const capitalsGraphics = new Graphics();
      capitalsLayer.addChild(capitalsGraphics);
      const unitsLayer = new Graphics();
      const groupsLayer = new Container();
      const groupsGraphics = new Graphics();
      groupsLayer.addChild(groupsGraphics);
      const highlight = new Graphics();
      app.stage.addChild(graphics);
      app.stage.addChild(edgesLayer);
      app.stage.addChild(capitalsLayer);
      app.stage.addChild(unitsLayer);
      app.stage.addChild(groupsLayer);
      app.stage.addChild(highlight);
      graphicsRef.current = graphics;
      edgesLayerRef.current = edgesLayer;
      unitsLayerRef.current = unitsLayer;
      groupsLayerRef.current = groupsLayer;
      capitalsLayerRef.current = capitalsLayer;
      highlightRef.current = highlight;

      // ---- Helper closures ----

      function getMapPixelWidth(): number {
        return SQRT3 * hexSize.current * GRID_WIDTH;
      }

      function wrapCamX(): void {
        const mapW = getMapPixelWidth();
        if (mapW <= 0) return;
        while (camX.current < 0) camX.current += mapW;
        while (camX.current >= mapW) camX.current -= mapW;
      }

      function screenToWorld(
        sx: number,
        sy: number
      ): { x: number; y: number } {
        return {
          x: camX.current + (sx - app.screen.width / 2),
          y: camY.current + (sy - app.screen.height / 2),
        };
      }

      /// World-space span of the grid, in pixels at current hex_size.
      function getWorldDims(): { w: number; h: number } {
        return {
          w: SQRT3 * hexSize.current * GRID_WIDTH,
          h: 1.5 * hexSize.current * GRID_HEIGHT,
        };
      }

      /// Polar projection parameters. In polar mode the grid is mapped onto
      /// a disc centred on screen; zoom scales the disc but doesn't pan.
      function polarParams(): {
        cx: number;
        cy: number;
        maxRadius: number;
        worldW: number;
        worldH: number;
      } {
        const { w, h } = getWorldDims();
        const vw = app.screen.width;
        const vh = app.screen.height;
        // Disc fills 92% of the smaller viewport dimension. Scales with zoom
        // so mouse-wheel still meaningfully zooms in polar mode.
        const base = Math.min(vw, vh) * 0.46;
        const zoomFactor = hexSize.current / DEFAULT_HEX_SIZE;
        return {
          cx: vw / 2,
          cy: vh / 2,
          maxRadius: base * zoomFactor,
          worldW: w,
          worldH: h,
        };
      }

      /// Project a world-space point into the polar disc. Returns screen (x, y)
      /// plus `localScale` — how big one `hex_size` unit is in screen px at this
      /// point of the disc, used to scale hex circles.
      function projectPolar(wx: number, wy: number): {
        x: number;
        y: number;
        localScale: number;
      } {
        const p = polarParams();
        const lat01 = Math.max(0, Math.min(1, wy / p.worldH));
        const lon01 = wx / p.worldW; // may be negative/ >1; theta wraps naturally
        const theta = lon01 * 2 * Math.PI;
        const rho = lat01 * p.maxRadius;
        return {
          x: p.cx + rho * Math.sin(theta),
          y: p.cy - rho * Math.cos(theta),
          // Local pixel density: average of radial + angular pitch.
          localScale:
            (p.maxRadius / GRID_HEIGHT + (rho * 2 * Math.PI) / GRID_WIDTH) / 2,
        };
      }

      /** All screen-X positions at which a shape of half-width `halfSpan`
       *  placed at world-X `worldX` would be visible. Returns one entry when
       *  the map is wider than the viewport, two or more when zoomed out far
       *  enough that the map tiles across the screen. */
      function wrappedScreenXs(worldX: number, halfSpan: number): number[] {
        const vw = app.screen.width;
        const mapW = getMapPixelWidth();
        const base = worldX - camX.current + vw / 2;
        if (mapW <= 0) return [base];

        // Reduce into [0, mapW), then walk left until we're past -halfSpan.
        let px = ((base % mapW) + mapW) % mapW;
        while (px > -halfSpan) px -= mapW;
        px += mapW;

        const out: number[] = [];
        const maxIter = Math.ceil(vw / Math.max(mapW, 1)) + 4;
        for (let i = 0; i < maxIter && px - halfSpan <= vw; i++) {
          out.push(px);
          px += mapW;
        }
        return out;
      }

      // ---- Drawing ----

      function drawHexes(): void {
        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;
        const proj = projection.current;

        let cells: HexCell[];
        if (proj === "polar") {
          // Query the whole grid via a huge fake viewport.
          const { w, h } = getWorldDims();
          cells = wasm.get_viewport_hexes(w / 2, h / 2, w + 1000, h + 1000, hs);
        } else {
          cells = wasm.get_viewport_hexes(
            camX.current,
            camY.current,
            vw,
            vh,
            hs,
          );
        }

        lastCellsRef.current = cells;
        setHexCount(cells.length);
        graphics.clear();

        // Batch by colour: ONE fill() per unique terrain colour instead of
        // one per hex. 10x–100x fewer GPU state changes.
        const buckets = new Map<number, HexCell[]>();
        for (const cell of cells) {
          const [cr, cg, cb] = cell.color;
          const key = ((cr & 0xff) << 16) | ((cg & 0xff) << 8) | (cb & 0xff);
          let list = buckets.get(key);
          if (!list) {
            list = [];
            buckets.set(key, list);
          }
          list.push(cell);
        }

        if (proj === "polar") {
          const { cx, cy, maxRadius } = polarParams();
          // Outer dark ring — explicit ice-wall around the disc.
          graphics.circle(cx, cy, maxRadius * 1.03);
          graphics.fill({ color: 0x0b2a4a, alpha: 0.9 });

          // Adaptive stride: skip cells when the disc is small enough that
          // neighbours collapse onto the same pixel. Keeps the circle count
          // around 30-60k regardless of grid size.
          const stride = Math.max(1, Math.floor((GRID_HEIGHT * 800) / (maxRadius * 200)));

          for (const [color, bucket] of buckets) {
            graphics.beginPath();
            for (let i = 0; i < bucket.length; i += stride) {
              const cell = bucket[i];
              const p = projectPolar(cell.px, cell.py);
              const rad = Math.max(1.5, p.localScale * 0.9 * stride);
              graphics.circle(p.x, p.y, rad);
            }
            graphics.fill({ color, alpha: 0.95 });
          }
          lastPolarStrideRef.current = stride;
          return;
        }
        lastPolarStrideRef.current = 1;

        for (const [color, bucket] of buckets) {
          graphics.beginPath();
          for (const cell of bucket) {
            const py = cell.py - camY.current + vh / 2;
            if (py < -hs || py > vh + hs) continue;
            for (const px of wrappedScreenXs(cell.px, hs)) {
              appendHexToPath(graphics, px, py, hs);
            }
          }
          graphics.fill({ color, alpha: 0.95 });
        }
      }

      /// Outline continents (land/water) and ice walls. Two path batches —
      /// one stroke() per colour — to keep GPU state changes cheap.
      function drawEdges(): void {
        const edges = edgesLayerRef.current;
        if (!edges) return;
        edges.clear();

        const cells = lastCellsRef.current;
        if (cells.length === 0) return;

        // Polar view: the outer dark ring already conveys the ice wall, and
        // 720k×6 neighbour checks would freeze the tab. Skip — edges come
        // back automatically when you switch back to flat.
        if (projection.current === "polar") return;

        const terrainBy = new Map<string, string>();
        for (const c of cells) terrainBy.set(`${c.q},${c.r}`, c.terrain);

        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;

        // Collect segments per colour bucket, then issue one stroke per bucket.
        type Seg = [number, number, number, number];
        const coastSegs: Seg[] = [];
        const iceSegs: Seg[] = [];

        const pushSegment = (
          p1x: number,
          p1y: number,
          p2x: number,
          p2y: number,
          bucket: Seg[],
        ) => {
          const dx = p2x - p1x;
          const dy = p2y - p1y;
          const len = Math.hypot(dx, dy);
          if (len < 0.01) return;
          const mx = (p1x + p2x) / 2;
          const my = (p1y + p2y) / 2;
          const half = Math.max(1.5, len * 0.5);
          const nx = -dy / len;
          const ny = dx / len;
          bucket.push([
            mx - nx * half,
            my - ny * half,
            mx + nx * half,
            my + ny * half,
          ]);
        };

        for (const cell of cells) {
          const isWater = WATER_TERRAINS.has(cell.terrain);
          const isIce = cell.terrain === ICE_TERRAIN;

          for (const [dq, dr] of NEIGHBOR_DIRS) {
            const nq = ((cell.q + dq) % GRID_WIDTH + GRID_WIDTH) % GRID_WIDTH;
            const nr = cell.r + dr;
            const nTerrain = terrainBy.get(`${nq},${nr}`);
            if (!nTerrain) continue;

            const nIsWater = WATER_TERRAINS.has(nTerrain);
            const nIsIce = nTerrain === ICE_TERRAIN;

            const iceEdge = isIce !== nIsIce;
            const coastEdge = !iceEdge && isWater !== nIsWater;
            if (!iceEdge && !coastEdge) continue;

            // Only draw each edge once — from the smaller-index cell.
            if (`${cell.q},${cell.r}` > `${nq},${nr}`) continue;

            const nWorldX = SQRT3 * hs * (nq + nr / 2);
            const nWorldY = 1.5 * hs * nr;
            const bucket = iceEdge ? iceSegs : coastSegs;

            const sxs1 = wrappedScreenXs(cell.px, hs);
            const sxs2 = wrappedScreenXs(nWorldX, hs);
            const py1 = cell.py - camY.current + vh / 2;
            const py2 = nWorldY - camY.current + vh / 2;
            if (py1 < -hs || py1 > vh + hs) continue;
            for (const px1 of sxs1) {
              let best = sxs2[0];
              let bestDist = Math.abs(best - px1);
              for (const px2 of sxs2) {
                const d = Math.abs(px2 - px1);
                if (d < bestDist) {
                  best = px2;
                  bestDist = d;
                }
              }
              if (bestDist < vw) {
                pushSegment(px1, py1, best, py2, bucket);
              }
            }
          }
        }

        const width = Math.max(1.5, hs * 0.45);

        if (coastSegs.length > 0) {
          edges.beginPath();
          for (const [x1, y1, x2, y2] of coastSegs) {
            edges.moveTo(x1, y1);
            edges.lineTo(x2, y2);
          }
          edges.stroke({ color: 0x0a0a0a, alpha: 0.85, width });
        }

        if (iceSegs.length > 0) {
          edges.beginPath();
          for (const [x1, y1, x2, y2] of iceSegs) {
            edges.moveTo(x1, y1);
            edges.lineTo(x2, y2);
          }
          edges.stroke({ color: 0xe0f7ff, alpha: 0.95, width });
        }
      }

      function drawUnits(): void {
        unitsLayer.clear();
        const list = unitsRef.current;
        if (!list || list.length === 0) return;

        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;
        const proj = projection.current;
        // Floor: 5 px so individual units are visible even at the lowest zoom.
        const radius = Math.max(5, hs * 0.55);
        const selectedId = selectedUnitIdRef.current;

        // Group units per hex for stack counting.
        const stacks = new Map<string, { x: number; y: number; units: StrategicUnitMarker[] }>();
        for (const u of list) {
          // Pick rendering position: interpolated between hexes if available.
          let worldX: number;
          let worldY: number;
          if (u.interp) {
            const a = hexToPixel(u.interp.from_q, u.interp.from_r, hs);
            const b = hexToPixel(u.interp.to_q, u.interp.to_r, hs);
            const f = Math.max(0, Math.min(1, u.interp.fraction));
            worldX = a.x + (b.x - a.x) * f;
            worldY = a.y + (b.y - a.y) * f;
          } else {
            const p = hexToPixel(u.hex_q, u.hex_r, hs);
            worldX = p.x;
            worldY = p.y;
          }

          const positions: Array<{ x: number; y: number }> = [];
          if (proj === "polar") {
            const p = projectPolar(worldX, worldY);
            positions.push({ x: p.x, y: p.y });
          } else {
            const py = worldY - camY.current + vh / 2;
            if (py < -radius || py > vh + radius) continue;
            for (const px of wrappedScreenXs(worldX, radius)) {
              positions.push({ x: px, y: py });
            }
          }

          for (const pos of positions) {
            const key = `${Math.round(pos.x)},${Math.round(pos.y)}`;
            const entry = stacks.get(key);
            if (entry) {
              entry.units.push(u);
            } else {
              stacks.set(key, { x: pos.x, y: pos.y, units: [u] });
            }
          }
        }

        for (const { x, y, units: stackUnits } of stacks.values()) {
          const head = stackUnits[0];
          const [r, g, b] = head.color;
          const color = ((r & 0xff) << 16) | ((g & 0xff) << 8) | (b & 0xff);

          unitsLayer.circle(x, y, radius);
          unitsLayer.fill({ color, alpha: 0.95 });
          unitsLayer.stroke({ color: 0x000000, alpha: 0.6, width: 1 });

          // Selection ring — matches a stack if it contains the selected unit.
          if (selectedId && stackUnits.some((u) => u.id === selectedId)) {
            unitsLayer.circle(x, y, radius + 3);
            unitsLayer.stroke({ color: 0xfde047, alpha: 1, width: 2 });
          }
        }
      }

      function drawGroups(): void {
        const groupsGfx = groupsLayer.children[0] as Graphics;
        groupsGfx.clear();

        const list = groupsRef.current;
        const labelPool = groupLabelPoolRef.current;
        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;
        // Floor: 9 px so groups stay visible even when you zoom way out.
        const radius = Math.max(9, hs * 0.9);
        const selectedId = selectedGroupIdRef.current;

        // Track which labels survived this frame; prune stale ones after.
        const seen = new Set<number>();

        const proj = projection.current;

        if (list && list.length > 0) {
          for (const g of list) {
            let worldX: number;
            let worldY: number;
            if (g.interp) {
              const a = hexToPixel(g.interp.from_q, g.interp.from_r, hs);
              const b = hexToPixel(g.interp.to_q, g.interp.to_r, hs);
              const f = Math.max(0, Math.min(1, g.interp.fraction));
              worldX = a.x + (b.x - a.x) * f;
              worldY = a.y + (b.y - a.y) * f;
            } else {
              const p = hexToPixel(g.hex_q, g.hex_r, hs);
              worldX = p.x;
              worldY = p.y;
            }

            const positions: Array<{ x: number; y: number }> = [];
            if (proj === "polar") {
              const p = projectPolar(worldX, worldY);
              positions.push({ x: p.x, y: p.y });
            } else {
              const py = worldY - camY.current + vh / 2;
              if (py < -radius || py > vh + radius) continue;
              for (const px of wrappedScreenXs(worldX, radius)) {
                positions.push({ x: px, y: py });
              }
            }

            const [cr, cg, cb] = g.color;
            const color = ((cr & 0xff) << 16) | ((cg & 0xff) << 8) | (cb & 0xff);

            let labelX: number | null = null;
            let labelY: number | null = null;
            for (const pos of positions) {
              if (labelX === null) {
                labelX = pos.x;
                labelY = pos.y;
              }
              // Filled circle + white outline to separate from unit markers.
              groupsGfx.circle(pos.x, pos.y, radius);
              groupsGfx.fill({ color, alpha: 0.95 });
              groupsGfx.stroke({ color: 0xffffff, alpha: 0.9, width: 1.5 });
              groupsGfx.circle(pos.x, pos.y, radius * 0.45);
              groupsGfx.stroke({ color: 0xffffff, alpha: 0.8, width: 1 });
              if (selectedId === g.id) {
                groupsGfx.circle(pos.x, pos.y, radius + 3);
                groupsGfx.stroke({ color: 0xfde047, alpha: 1, width: 2 });
              }
            }

            if (labelX === null || labelY === null) continue;

            const text = `×${g.member_count}`;
            let label = labelPool.get(g.id);
            if (!label) {
              label = new Text({
                text,
                style: new TextStyle({
                  fontFamily: "monospace",
                  fontSize: 11,
                  fill: 0xffffff,
                  stroke: { color: 0x000000, width: 3 },
                  fontWeight: "bold",
                }),
              });
              label.anchor.set(0.5, 0.5);
              groupsLayer.addChild(label);
              labelPool.set(g.id, label);
            }
            label.text = text;
            label.x = labelX;
            label.y = labelY + radius + 6;
            label.visible = true;
            seen.add(g.id);
          }
        }

        // Hide or drop labels for groups that are no longer present.
        for (const [id, label] of labelPool) {
          if (!seen.has(id)) {
            label.visible = false;
          }
        }
      }

      /// Render national capitals — white-rimmed coloured star + 3-letter code
      /// label. Always shown on top of terrain, visible at every zoom level.
      function drawCapitals(): void {
        const layer = capitalsLayerRef.current;
        if (!layer) return;
        const gfx = layer.children[0] as Graphics;
        gfx.clear();

        const list = capitalsRef.current;
        const pool = capitalLabelPoolRef.current;
        const seen = new Set<string>();

        if (!list || list.length === 0) {
          for (const [, label] of pool) label.visible = false;
          return;
        }

        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;
        const proj = projection.current;
        // Capitals stay readable regardless of zoom — bigger floor than hexes.
        const starRadius = Math.max(7, hs * 1.2);

        for (const cap of list) {
          const worldX = SQRT3 * hs * cap.hex_q + (SQRT3 / 2) * hs * cap.hex_r;
          const worldY = 1.5 * hs * cap.hex_r;

          let positions: Array<{ x: number; y: number }> = [];
          if (proj === "polar") {
            const p = projectPolar(worldX, worldY);
            positions.push({ x: p.x, y: p.y });
          } else {
            const py = worldY - camY.current + vh / 2;
            if (py < -starRadius || py > vh + starRadius) continue;
            for (const px of wrappedScreenXs(worldX, starRadius)) {
              positions.push({ x: px, y: py });
            }
          }

          const [cr, cg, cb] = cap.color;
          const color = ((cr & 0xff) << 16) | ((cg & 0xff) << 8) | (cb & 0xff);

          let labelX: number | null = null;
          for (const pos of positions) {
            if (labelX === null) labelX = pos.x;
            // Double-ring: outer white frame, inner nation colour disc.
            gfx.circle(pos.x, pos.y, starRadius + 2);
            gfx.fill({ color: 0xffffff, alpha: 1 });
            gfx.circle(pos.x, pos.y, starRadius);
            gfx.fill({ color, alpha: 1 });
            // Small inner star dot.
            gfx.circle(pos.x, pos.y, Math.max(2, starRadius * 0.3));
            gfx.fill({ color: 0xffffff, alpha: 0.9 });
          }

          if (labelX === null) continue;

          let label = pool.get(cap.nation_code);
          if (!label) {
            label = new Text({
              text: cap.nation_code,
              style: new TextStyle({
                fontFamily: "monospace",
                fontSize: 12,
                fill: 0xffffff,
                stroke: { color: 0x000000, width: 3 },
                fontWeight: "bold",
              }),
            });
            label.anchor.set(0.5, 1.0);
            layer.addChild(label);
            pool.set(cap.nation_code, label);
          }
          label.x = labelX;
          label.y = positions[0].y - starRadius - 4;
          label.visible = true;
          seen.add(cap.nation_code);
        }

        for (const [key, label] of pool) {
          if (!seen.has(key)) label.visible = false;
        }
      }

      function drawHighlights(): void {
        highlight.clear();

        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;

        const hovered = hoveredHexRef.current;
        const selected = selectedHexRef.current;

        // Hovered hex — white border (skip if same as selected)
        if (
          hovered &&
          !(
            selected &&
            hovered.q === selected.q &&
            hovered.r === selected.r
          )
        ) {
          const pos = hexToPixel(hovered.q, hovered.r, hs);
          const py = pos.y - camY.current + vh / 2;
          if (py > -hs && py < vh + hs) {
            for (const px of wrappedScreenXs(pos.x, hs)) {
              drawHexPath(highlight, px, py, hs);
              highlight.stroke({ color: 0xffffff, alpha: 0.8, width: 2 });
            }
          }
        }

        // Selected hex — yellow border
        if (selected) {
          const pos = hexToPixel(selected.q, selected.r, hs);
          const py = pos.y - camY.current + vh / 2;
          if (py > -hs && py < vh + hs) {
            for (const px of wrappedScreenXs(pos.x, hs)) {
              drawHexPath(highlight, px, py, hs);
              highlight.stroke({ color: 0xffff00, alpha: 1.0, width: 2.5 });
            }
          }
        }
      }

      // ---- Keyboard ----

      const keys = new Set<string>();
      window.addEventListener(
        "keydown",
        (e) => keys.add(e.key.toLowerCase()),
        { signal }
      );
      window.addEventListener(
        "keyup",
        (e) => keys.delete(e.key.toLowerCase()),
        { signal }
      );

      // ---- Mouse events ----

      const canvas = app.canvas as HTMLCanvasElement;

      // Prevent context menu on right-click
      canvas.addEventListener(
        "contextmenu",
        (e) => e.preventDefault(),
        { signal }
      );

      canvas.addEventListener(
        "mousedown",
        (e) => {
          mouseDownPos.current = { x: e.clientX, y: e.clientY };
          mousePos.current = { x: e.clientX, y: e.clientY };
          mouseIsDown.current = true;
          isDragging.current = false;
        },
        { signal }
      );

      // Window-level mousemove for drag (continues outside canvas)
      window.addEventListener(
        "mousemove",
        (e) => {
          if (!mouseIsDown.current) return;

          const dx = e.clientX - mouseDownPos.current.x;
          const dy = e.clientY - mouseDownPos.current.y;

          if (
            !isDragging.current &&
            (Math.abs(dx) > DRAG_THRESHOLD ||
              Math.abs(dy) > DRAG_THRESHOLD)
          ) {
            isDragging.current = true;
          }

          if (isDragging.current) {
            camX.current -= e.clientX - mousePos.current.x;
            camY.current -= e.clientY - mousePos.current.y;
            mousePos.current = { x: e.clientX, y: e.clientY };
            wrapCamX();
            hexDirty.current = true;
            highlightDirty.current = true;
          }
        },
        { signal }
      );

      // Canvas-level mousemove for hover
      canvas.addEventListener(
        "mousemove",
        (e) => {
          if (isDragging.current) return;

          const rect = canvas.getBoundingClientRect();
          const sx = e.clientX - rect.left;
          const sy = e.clientY - rect.top;

          const world = screenToWorld(sx, sy);
          const hexCoords = wasm.pixel_to_hex_wrapped(
            world.x,
            world.y,
            hexSize.current,
            GRID_WIDTH
          );
          const hexInfo = wasm.get_hex_by_coords(hexCoords.q, hexCoords.r);

          if (hexInfo != null) {
            if (
              !hoveredHexRef.current ||
              hoveredHexRef.current.q !== hexInfo.q ||
              hoveredHexRef.current.r !== hexInfo.r
            ) {
              hoveredHexRef.current = hexInfo;
              setHoveredHex(hexInfo);
              highlightDirty.current = true;
            }
          } else if (hoveredHexRef.current) {
            hoveredHexRef.current = null;
            setHoveredHex(null);
            highlightDirty.current = true;
          }
        },
        { signal }
      );

      // Canvas mouseup — handle click (if not drag)
      canvas.addEventListener(
        "mouseup",
        (e) => {
          if (mouseIsDown.current && !isDragging.current) {
            const rect = canvas.getBoundingClientRect();
            const sx = e.clientX - rect.left;
            const sy = e.clientY - rect.top;
            const world = screenToWorld(sx, sy);
            const hexCoords = wasm.pixel_to_hex_wrapped(
              world.x,
              world.y,
              hexSize.current,
              GRID_WIDTH
            );
            const hexInfo = wasm.get_hex_by_coords(
              hexCoords.q,
              hexCoords.r
            );

            if (hexInfo != null) {
              if (
                selectedHexRef.current &&
                selectedHexRef.current.q === hexInfo.q &&
                selectedHexRef.current.r === hexInfo.r
              ) {
                // Deselect
                selectedHexRef.current = null;
                setSelectedHex(null);
              } else {
                selectedHexRef.current = hexInfo;
                setSelectedHex(hexInfo);
              }
              highlightDirty.current = true;
              // Fire external action callback — parent decides what to do
              // (select a unit, issue a move order, etc.).
              onHexActionRef.current?.(hexInfo.q, hexInfo.r);
            }
          }
          mouseIsDown.current = false;
          isDragging.current = false;
        },
        { signal }
      );

      // Window mouseup — reset drag state if released outside canvas
      window.addEventListener(
        "mouseup",
        () => {
          mouseIsDown.current = false;
          isDragging.current = false;
        },
        { signal }
      );

      canvas.addEventListener(
        "mouseleave",
        () => {
          if (!isDragging.current) {
            hoveredHexRef.current = null;
            setHoveredHex(null);
            highlightDirty.current = true;
          }
        },
        { signal }
      );

      // ---- Double-click: enter battlefield ----
      canvas.addEventListener(
        "dblclick",
        (e) => {
          if (!onEnterBattlefield) return;
          const rect = canvas.getBoundingClientRect();
          const sx = e.clientX - rect.left;
          const sy = e.clientY - rect.top;
          const world = screenToWorld(sx, sy);
          const hexCoords = wasm.pixel_to_hex_wrapped(
            world.x,
            world.y,
            hexSize.current,
            GRID_WIDTH
          );
          const hexInfo = wasm.get_hex_by_coords(hexCoords.q, hexCoords.r);
          if (hexInfo) {
            onEnterBattlefield(hexInfo.q, hexInfo.r, hexInfo.terrain, hexInfo.elevation);
          }
        },
        { signal }
      );

      // ---- Zoom (scroll wheel) ----

      canvas.addEventListener(
        "wheel",
        (e) => {
          e.preventDefault();

          const oldSize = hexSize.current;
          const direction = e.deltaY < 0 ? 1 : -1;
          const newSize = Math.max(
            MIN_HEX_SIZE,
            Math.min(
              MAX_HEX_SIZE,
              oldSize * Math.pow(ZOOM_STEP, direction)
            )
          );

          if (newSize === oldSize) return;

          // Zoom-to-cursor: keep the pixel under the cursor anchored.
          // Computed in raw world space (not via a wrapped hex) so copies
          // past the anti-meridian don't snap the camera back to the
          // canonical side.
          const rect = canvas.getBoundingClientRect();
          const sx = e.clientX - rect.left;
          const sy = e.clientY - rect.top;
          const world = screenToWorld(sx, sy);
          const scale = newSize / oldSize;

          hexSize.current = newSize;
          setZoomLevel(newSize);

          camX.current = world.x * scale - (sx - app.screen.width / 2);
          camY.current = world.y * scale - (sy - app.screen.height / 2);

          wrapCamX();
          hexDirty.current = true;
          highlightDirty.current = true;
        },
        { signal, passive: false }
      );

      // ---- Resize ----

      resizeObserver = new ResizeObserver(() => {
        hexDirty.current = true;
        highlightDirty.current = true;
      });
      if (containerRef.current) {
        resizeObserver.observe(containerRef.current);
      }

      // ---- Ticker ----

      let renderFrameCount = 0;
      let lastFpsTime = performance.now();

      app.ticker.add(() => {
        // Keyboard movement
        const speed = 8;
        let moved = false;
        if (keys.has("w") || keys.has("arrowup")) {
          camY.current -= speed;
          moved = true;
        }
        if (keys.has("s") || keys.has("arrowdown")) {
          camY.current += speed;
          moved = true;
        }
        if (keys.has("a") || keys.has("arrowleft")) {
          camX.current -= speed;
          moved = true;
        }
        if (keys.has("d") || keys.has("arrowright")) {
          camX.current += speed;
          moved = true;
        }

        if (moved) {
          wrapCamX();
          hexDirty.current = true;
          highlightDirty.current = true;
        }

        let rendered = false;

        if (hexDirty.current) {
          hexDirty.current = false;
          drawHexes();
          edgesDirty.current = true;
          unitsDirty.current = true;
          groupsDirty.current = true;
          capitalsDirty.current = true;
          highlightDirty.current = true;
          rendered = true;

          setCamState({ x: camX.current, y: camY.current });
          setViewportSize({ width: app.screen.width, height: app.screen.height });
        }

        if (edgesDirty.current) {
          edgesDirty.current = false;
          drawEdges();
          rendered = true;
        }

        if (unitsDirty.current) {
          unitsDirty.current = false;
          drawUnits();
          rendered = true;
        }

        if (groupsDirty.current) {
          groupsDirty.current = false;
          drawGroups();
          rendered = true;
        }

        if (capitalsDirty.current) {
          capitalsDirty.current = false;
          drawCapitals();
          rendered = true;
        }

        if (highlightDirty.current) {
          highlightDirty.current = false;
          if (projection.current === "polar") {
            highlight.clear();
          } else {
            drawHighlights();
          }
          rendered = true;
        }

        // FPS counter — only counts frames where actual rendering happened
        if (rendered) {
          renderFrameCount++;
        }

        const now = performance.now();
        if (now - lastFpsTime > 1000) {
          setFps(
            Math.round((renderFrameCount * 1000) / (now - lastFpsTime))
          );
          renderFrameCount = 0;
          lastFpsTime = now;
        }
      });

      setLoading(false);
    }

    init();

    return () => {
      mounted = false;
      abortController.abort();
      if (resizeObserver) resizeObserver.disconnect();
      // Drop cached Text labels — the Container is destroyed with the app,
      // but we don't want the stale Text refs to survive across remounts.
      groupLabelPoolRef.current.clear();
      if (appRef.current) {
        const canvas = appRef.current.canvas as HTMLCanvasElement | null;
        // Pixi v8: destroy({ removeView: true }, true) detaches the canvas
        // and destroys all children. Belt-and-braces: also manually remove
        // from DOM in case StrictMode / HMR leaves it behind.
        appRef.current.destroy({ removeView: true }, true);
        if (canvas && canvas.parentNode) canvas.parentNode.removeChild(canvas);
        appRef.current = null;
      }
      // Clear container just in case prior runs orphaned a canvas.
      if (containerRef.current) {
        while (containerRef.current.firstChild) {
          containerRef.current.removeChild(containerRef.current.firstChild);
        }
      }
    };
  }, []);

  // ---- Info panel data ----
  const displayHex = selectedHex || hoveredHex;
  const displayTerrain = displayHex
    ? TERRAIN_NAMES[displayHex.terrain] || displayHex.terrain
    : null;
  const displayCost = displayHex ? displayHex.movement_cost : null;

  // Click on minimap → jump camera to that world position.
  const handleMinimapNavigate = (worldX: number, worldY: number) => {
    camX.current = worldX;
    camY.current = worldY;
    hexDirty.current = true;
    highlightDirty.current = true;
  };

  // M toggles the minimap overlay; P toggles polar (flat-earth) projection.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key.toLowerCase() === "m") {
        setShowMinimap((v) => !v);
      }
      if (e.key.toLowerCase() === "p") {
        const next = projection.current === "flat" ? "polar" : "flat";
        projection.current = next;
        setProjectionMode(next);
        hexDirty.current = true;
        edgesDirty.current = true;
        unitsDirty.current = true;
        groupsDirty.current = true;
        capitalsDirty.current = true;
        highlightDirty.current = true;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Keep ticker closures in sync with the latest props without tearing down
  // the Pixi application every render.
  useEffect(() => {
    unitsRef.current = units;
    unitsDirty.current = true;
  }, [units]);

  useEffect(() => {
    groupsRef.current = groups;
    groupsDirty.current = true;
  }, [groups]);

  useEffect(() => {
    capitalsRef.current = capitals;
    capitalsDirty.current = true;
  }, [capitals]);

  useEffect(() => {
    selectedUnitIdRef.current = selectedUnitId ?? null;
    unitsDirty.current = true;
  }, [selectedUnitId]);

  useEffect(() => {
    selectedGroupIdRef.current = selectedGroupId ?? null;
    groupsDirty.current = true;
  }, [selectedGroupId]);

  useEffect(() => {
    onHexActionRef.current = onHexAction;
  }, [onHexAction]);

  return (
    <div className="relative w-full h-full select-none">
      {/* Error overlay */}
      {error && (
        <div className="absolute inset-0 flex items-center justify-center bg-black text-red-400 z-20">
          <div className="text-center">
            <div className="text-2xl font-bold mb-2">WASM Error</div>
            <div className="text-sm max-w-lg">{error}</div>
          </div>
        </div>
      )}

      {/* Loading overlay */}
      {loading && !error && (
        <div className="absolute inset-0 flex items-center justify-center bg-black text-white z-10">
          <div className="text-center">
            <div className="text-2xl font-bold mb-2">Loading hex grid...</div>
            <div className="text-zinc-400">Initializing WASM + PixiJS</div>
          </div>
        </div>
      )}

      {/* PixiJS canvas container */}
      <div ref={containerRef} className="w-full h-full" />

      {/* Stats panel — top left, offset below the top HUD bar */}
      <div className="absolute top-14 left-4 bg-black/70 text-white px-3 py-2 rounded text-sm font-mono pointer-events-none">
        <div>FPS: {fps}</div>
        <div>Hexes: {hexCount.toLocaleString()}</div>
        <div>Zoom: {zoomLevel.toFixed(1)}</div>
        <div className="pointer-events-auto">
          <button
            onClick={() => {
              const next = projection.current === "flat" ? "polar" : "flat";
              projection.current = next;
              setProjectionMode(next);
              hexDirty.current = true;
              edgesDirty.current = true;
              unitsDirty.current = true;
              groupsDirty.current = true;
              capitalsDirty.current = true;
              highlightDirty.current = true;
            }}
            className="text-left hover:text-cyan-200"
          >
            Widok:{" "}
            <span className="text-cyan-300">
              {projectionMode === "polar" ? "🌍 tarcza (ONZ)" : "🗺 płaska"}
            </span>
          </button>
        </div>
        <div className="text-zinc-400 mt-1">WASD / drag — pan</div>
        <div className="text-zinc-400">Scroll — zoom</div>
        <div className="text-zinc-400">Click — select hex</div>
        <div className="text-zinc-400">Dbl-click — enter battlefield</div>
        <div className="text-zinc-400">M — toggle minimap</div>
        <div className="text-zinc-400">P — przełącz widok</div>
      </div>

      {/* Minimap — bottom left */}
      {!loading && !error && showMinimap && (
        <div className="absolute bottom-4 left-4 pointer-events-auto">
          <Minimap
            camX={camState.x}
            camY={camState.y}
            hexSize={zoomLevel}
            viewWidth={viewportSize.width}
            viewHeight={viewportSize.height}
            onNavigate={handleMinimapNavigate}
          />
        </div>
      )}

      {/* Info panel — bottom right */}
      {displayHex && (
        <div className="absolute bottom-4 right-4 bg-black/80 text-white px-4 py-3 rounded-lg font-mono text-sm min-w-[200px]">
          <div className="text-zinc-400 text-xs mb-2 uppercase tracking-wider">
            Hex Info
          </div>
          <div>
            q: <span className="text-emerald-400">{displayHex.q}</span>, r:{" "}
            <span className="text-emerald-400">{displayHex.r}</span>
          </div>
          <div>
            Teren: <span className="text-amber-300">{displayTerrain}</span>
          </div>
          <div>
            Elewacja:{" "}
            <span className="text-sky-300">
              {displayHex.elevation.toFixed(1)}m
            </span>
          </div>
          <div>
            Koszt ruchu:{" "}
            <span className="text-orange-300">
              {displayCost === null
                ? "?"
                : !Number.isFinite(displayCost)
                  ? "—"
                  : displayCost.toFixed(1)}
            </span>
          </div>
          {onEnterBattlefield && displayHex.terrain !== "DeepOcean" && displayHex.terrain !== "Ocean" && (
            <button
              onClick={() => onEnterBattlefield(displayHex.q, displayHex.r, displayHex.terrain, displayHex.elevation)}
              className="mt-2 w-full px-3 py-1.5 bg-amber-600/80 hover:bg-amber-500 text-white text-xs rounded transition-colors pointer-events-auto"
            >
              🔍 Enter Battlefield
            </button>
          )}
        </div>
      )}
    </div>
  );
}
