"use client";

import { useEffect, useRef, useState } from "react";
import { Application, Graphics } from "pixi.js";
import { loadWasm, type WasmApi } from "../lib/wasm-loader";

// --- Constants ---
const GRID_WIDTH = 1200;
const GRID_HEIGHT = 600;
const MIN_HEX_SIZE = 2;
const MAX_HEX_SIZE = 20;
const DEFAULT_HEX_SIZE = 4;
const ZOOM_STEP = 1.15;
const DRAG_THRESHOLD = 5;
const SQRT3 = Math.sqrt(3);

// --- Types ---
interface HexCell {
  q: number;
  r: number;
  terrain: string;
  color: [number, number, number];
  elevation: number;
  px: number;
  py: number;
}

// --- Terrain data ---
const TERRAIN_MOVEMENT_COST: Record<string, number> = {
  DeepOcean: Infinity,
  Ocean: Infinity,
  Coast: 1.5,
  Plains: 1.0,
  Forest: 1.5,
  Hills: 2.0,
  Mountain: 3.0,
  Desert: 1.5,
  Tundra: 2.0,
  Urban: 1.2,
  Ice: 2.5,
};

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

// --- Component ---
interface HexMapProps {
  onEnterBattlefield?: (q: number, r: number, terrain: string, elevation: number) => void;
}

export default function HexMap({ onEnterBattlefield }: HexMapProps) {
  // DOM / PixiJS refs
  const containerRef = useRef<HTMLDivElement>(null);
  const appRef = useRef<Application | null>(null);
  const graphicsRef = useRef<Graphics | null>(null);
  const highlightRef = useRef<Graphics | null>(null);

  // Camera state (refs — no React re-renders)
  const camX = useRef(0);
  const camY = useRef(3000);
  const hexSize = useRef(DEFAULT_HEX_SIZE);

  // Interaction state (React state for UI, refs for closures)
  const [hoveredHex, setHoveredHex] = useState<HexCell | null>(null);
  const [selectedHex, setSelectedHex] = useState<HexCell | null>(null);
  const [fps, setFps] = useState(0);
  const [hexCount, setHexCount] = useState(0);
  const [zoomLevel, setZoomLevel] = useState(DEFAULT_HEX_SIZE);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const hoveredHexRef = useRef<HexCell | null>(null);
  const selectedHexRef = useRef<HexCell | null>(null);

  // Dirty flags
  const hexDirty = useRef(true);
  const highlightDirty = useRef(true);

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
        app.destroy(true);
        return;
      }

      containerRef.current!.appendChild(app.canvas as HTMLCanvasElement);
      appRef.current = app;

      // Two Graphics layers: hexes (bottom) + highlights (top)
      const graphics = new Graphics();
      const highlight = new Graphics();
      app.stage.addChild(graphics);
      app.stage.addChild(highlight);
      graphicsRef.current = graphics;
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

      /** Adjust screen X for wrap-around so hexes from the "other side"
       *  appear at the correct screen position. */
      function normalizeScreenX(px: number): number {
        const mapW = getMapPixelWidth();
        if (mapW <= 0) return px;
        if (px < -mapW / 2) return px + mapW;
        if (px > mapW / 2) return px - mapW;
        return px;
      }

      // ---- Drawing ----

      function drawHexes(): void {
        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;

        const cells: HexCell[] = wasm.get_viewport_hexes(
          camX.current,
          camY.current,
          vw,
          vh,
          hs
        );

        setHexCount(cells.length);
        graphics.clear();

        for (const cell of cells) {
          let px = cell.px - camX.current + vw / 2;
          const py = cell.py - camY.current + vh / 2;

          // Wrap-around adjustment
          px = normalizeScreenX(px);

          // Skip off-screen hexes
          if (px < -hs || px > vw + hs || py < -hs || py > vh + hs) continue;

          const [cr, cg, cb] = cell.color;
          const color =
            ((cr & 0xff) << 16) | ((cg & 0xff) << 8) | (cb & 0xff);

          drawHexPath(graphics, px, py, hs);
          graphics.fill({ color, alpha: 0.9 });
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
          let px = pos.x - camX.current + vw / 2;
          const py = pos.y - camY.current + vh / 2;
          px = normalizeScreenX(px);

          if (px > -hs && px < vw + hs) {
            drawHexPath(highlight, px, py, hs);
            highlight.stroke({ color: 0xffffff, alpha: 0.8, width: 2 });
          }
        }

        // Selected hex — yellow border
        if (selected) {
          const pos = hexToPixel(selected.q, selected.r, hs);
          let px = pos.x - camX.current + vw / 2;
          const py = pos.y - camY.current + vh / 2;
          px = normalizeScreenX(px);

          if (px > -hs && px < vw + hs) {
            drawHexPath(highlight, px, py, hs);
            highlight.stroke({ color: 0xffff00, alpha: 1.0, width: 2.5 });
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

          // Zoom-to-cursor: find hex under cursor before zoom
          const rect = canvas.getBoundingClientRect();
          const sx = e.clientX - rect.left;
          const sy = e.clientY - rect.top;
          const world = screenToWorld(sx, sy);
          const hexUnderCursor = wasm.pixel_to_hex_wrapped(
            world.x,
            world.y,
            oldSize,
            GRID_WIDTH
          );

          hexSize.current = newSize;
          setZoomLevel(newSize);

          // Compute new world position of that hex at new zoom
          const newPos = hexToPixel(
            hexUnderCursor.q,
            hexUnderCursor.r,
            newSize
          );

          // Adjust camera so the hex stays under cursor
          camX.current = newPos.x - (sx - app.screen.width / 2);
          camY.current = newPos.y - (sy - app.screen.height / 2);

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
          highlightDirty.current = true; // positions changed
          rendered = true;
        }

        if (highlightDirty.current) {
          highlightDirty.current = false;
          drawHighlights();
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
      if (appRef.current) {
        appRef.current.destroy(true);
        appRef.current = null;
      }
    };
  }, []);

  // ---- Info panel data ----
  const displayHex = selectedHex || hoveredHex;
  const displayTerrain = displayHex
    ? TERRAIN_NAMES[displayHex.terrain] || displayHex.terrain
    : null;
  const displayCost = displayHex
    ? TERRAIN_MOVEMENT_COST[displayHex.terrain] ?? "?"
    : null;

  return (
    <div className="relative w-full h-screen select-none">
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

      {/* Stats panel — top left */}
      <div className="absolute top-4 left-4 bg-black/70 text-white px-3 py-2 rounded text-sm font-mono pointer-events-none">
        <div>FPS: {fps}</div>
        <div>Hexes: {hexCount.toLocaleString()}</div>
        <div>Zoom: {zoomLevel.toFixed(1)}</div>
        <div className="text-zinc-400 mt-1">WASD / drag — pan</div>
        <div className="text-zinc-400">Scroll — zoom</div>
        <div className="text-zinc-400">Click — select hex</div>
        <div className="text-zinc-400">Dbl-click — enter battlefield</div>
      </div>

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
              {displayCost === Infinity ? "—" : displayCost}
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
