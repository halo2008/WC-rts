"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import { Application, Graphics } from "pixi.js";
import { loadWasm, type WasmApi, type BattlefieldCell } from "../lib/wasm-loader";

// --- Constants ---
const DEFAULT_HEX_SIZE = 8;
const MIN_HEX_SIZE = 3;
const MAX_HEX_SIZE = 24;
const ZOOM_STEP = 1.15;
const DRAG_THRESHOLD = 5;
const SQRT3 = Math.sqrt(3);

// --- Types ---
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

// --- Helper functions ---
function hexToPixel(q: number, r: number, size: number): { x: number; y: number } {
  return {
    x: size * (SQRT3 * q + (SQRT3 / 2) * r),
    y: size * 1.5 * r,
  };
}

function drawHexPath(graphics: Graphics, cx: number, cy: number, size: number) {
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

function pixelToHex(px: number, py: number, size: number): { q: number; r: number } {
  const q = (SQRT3 / 3 * px - 1 / 3 * py) / size;
  const r = (2 / 3 * py) / size;
  const s = -q - r;
  let rq = Math.round(q);
  let rr = Math.round(r);
  const rs = Math.round(s);
  const dq = Math.abs(rq - q);
  const dr = Math.abs(rr - r);
  const ds = Math.abs(rs - s);
  if (dq > dr && dq > ds) rq = -rr - rs;
  else if (dr > ds) rr = -rq - rs;
  return { q: rq, r: rr };
}

// --- Component ---
interface BattlefieldMapProps {
  strategicQ: number;
  strategicR: number;
  macroTerrain: string;
  macroElevation: number;
  onExit: () => void;
}

export default function BattlefieldMap({
  strategicQ,
  strategicR,
  macroTerrain,
  macroElevation,
  onExit,
}: BattlefieldMapProps) {
  // DOM / PixiJS refs
  const containerRef = useRef<HTMLDivElement>(null);
  const appRef = useRef<Application | null>(null);
  const terrainLayerRef = useRef<Graphics | null>(null);
  const buildingLayerRef = useRef<Graphics | null>(null);
  const highlightLayerRef = useRef<Graphics | null>(null);

  // Camera state
  const camX = useRef(0);
  const camY = useRef(0);
  const hexSize = useRef(DEFAULT_HEX_SIZE);

  // Interaction state
  const [hoveredHex, setHoveredHex] = useState<{ q: number; r: number } | null>(null);
  const [selectedHex, setSelectedHex] = useState<{ q: number; r: number } | null>(null);
  const [fps, setFps] = useState(0);
  const [hexCount, setHexCount] = useState(0);
  const [zoomLevel, setZoomLevel] = useState(DEFAULT_HEX_SIZE);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showBuildMenu, setShowBuildMenu] = useState(false);
  const [buildingTypes, setBuildingTypes] = useState<BuildingTypeInfo[]>([]);

  // Refs for closures
  const hoveredHexRef = useRef<{ q: number; r: number } | null>(null);
  const selectedHexRef = useRef<{ q: number; r: number } | null>(null);
  const wasmRef = useRef<WasmApi | null>(null);

  // Dirty flags
  const hexDirty = useRef(true);
  const highlightDirty = useRef(true);

  // Mouse state
  const mousePos = useRef({ x: 0, y: 0 });
  const mouseDownPos = useRef({ x: 0, y: 0 });
  const mouseIsDown = useRef(false);
  const isDragging = useRef(false);

  // Building placement handler
  const handleBuild = useCallback((buildingType: string) => {
    if (!wasmRef.current || !selectedHex) return;
    const result = wasmRef.current.place_building(buildingType, selectedHex.q, selectedHex.r);
    if (result >= 0) {
      hexDirty.current = true;
      setShowBuildMenu(false);
    }
  }, [selectedHex]);

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
        return;
      }

      if (!mounted) return;
      wasmRef.current = wasm;

      // Initialize battlefield
      const seed = Math.abs(strategicQ * 10000 + strategicR);
      wasm.init_battlefield(strategicQ, strategicR, 64, seed, macroTerrain, macroElevation);

      // Cache building types
      const types = wasm.get_building_types();
      setBuildingTypes(types);

      const app = new Application();
      await app.init({
        background: "#1a1a2e",
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

      // Layers
      const terrainLayer = new Graphics();
      const buildingLayer = new Graphics();
      const highlightLayer = new Graphics();
      app.stage.addChild(terrainLayer);
      app.stage.addChild(buildingLayer);
      app.stage.addChild(highlightLayer);

      terrainLayerRef.current = terrainLayer;
      buildingLayerRef.current = buildingLayer;
      highlightLayerRef.current = highlightLayer;

      // Center camera on battlefield center
      const center = hexToPixel(32, 32, hexSize.current);
      camX.current = center.x;
      camY.current = center.y;

      // ---- Drawing ----

      function drawBattlefield(): void {
        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;

        const cells: BattlefieldCell[] = wasm.get_battlefield_viewport(
          camX.current,
          camY.current,
          vw,
          vh,
          hs
        );

        setHexCount(cells.length);
        const terrain = terrainLayerRef.current!;
        terrain.clear();

        for (const cell of cells) {
          const pos = hexToPixel(cell.q, cell.r, hs);
          const px = pos.x - camX.current + vw / 2;
          const py = pos.y - camY.current + vh / 2;

          if (px < -hs * 2 || px > vw + hs * 2 || py < -hs * 2 || py > vh + hs * 2) continue;

          const [cr, cg, cb] = cell.color;
          const color = ((cr & 0xff) << 16) | ((cg & 0xff) << 8) | (cb & 0xff);

          drawHexPath(terrain, px, py, hs);
          terrain.fill({ color, alpha: 0.85 });

          // Road indicator
          if (cell.has_road) {
            terrain.rect(px - hs * 0.3, py - 1, hs * 0.6, 2);
            terrain.fill({ color: 0x8b7355, alpha: 0.6 });
          }

          // River indicator
          if (cell.has_river) {
            terrain.rect(px - 1, py - hs * 0.3, 2, hs * 0.6);
            terrain.fill({ color: 0x4488cc, alpha: 0.7 });
          }

          // Deposit indicator
          if (cell.deposit) {
            const depositColor = cell.deposit === "Metals" ? 0xb48c3c :
              cell.deposit === "Oil" ? 0x333333 :
              cell.deposit === "Farmland" ? 0x228b22 :
              0x006400;
            terrain.circle(px + hs * 0.3, py - hs * 0.3, hs * 0.15);
            terrain.fill({ color: depositColor, alpha: 0.8 });
          }
        }
      }

      function drawBuildings(): void {
        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;
        const buildings = wasm.get_all_buildings();
        const layer = buildingLayerRef.current!;
        layer.clear();

        if (!buildings) return;

        for (const b of buildings) {
          const pos = hexToPixel(b.hex_q, b.hex_r, hs);
          const px = pos.x - camX.current + vw / 2;
          const py = pos.y - camY.current + vh / 2;

          if (px < -hs * 2 || px > vw + hs * 2 || py < -hs * 2 || py > vh + hs * 2) continue;

          const [cr, cg, cb] = b.color;
          const color = ((cr & 0xff) << 16) | ((cg & 0xff) << 8) | (cb & 0xff);

          // Building square
          const size = hs * 0.6;
          layer.rect(px - size / 2, py - size / 2, size, size);
          layer.fill({ color, alpha: 0.9 });
          layer.stroke({ color: 0xffffff, width: 1, alpha: 0.5 });

          // HP bar
          if (b.hp_fraction < 1.0) {
            const barW = size;
            const barH = 3;
            const barY = py - size / 2 - 5;
            layer.rect(px - barW / 2, barY, barW, barH);
            layer.fill({ color: 0x333333, alpha: 0.7 });
            layer.rect(px - barW / 2, barY, barW * b.hp_fraction, barH);
            layer.fill({
              color: b.hp_fraction > 0.5 ? 0x00ff00 : b.hp_fraction > 0.25 ? 0xffff00 : 0xff0000,
              alpha: 0.9,
            });
          }
        }
      }

      function drawHighlights(): void {
        const layer = highlightLayerRef.current!;
        layer.clear();

        const vw = app.screen.width;
        const vh = app.screen.height;
        const hs = hexSize.current;

        // Hovered hex
        if (hoveredHexRef.current) {
          const pos = hexToPixel(hoveredHexRef.current.q, hoveredHexRef.current.r, hs);
          const px = pos.x - camX.current + vw / 2;
          const py = pos.y - camY.current + vh / 2;
          drawHexPath(layer, px, py, hs);
          layer.stroke({ color: 0xffffff, alpha: 0.6, width: 1.5 });
        }

        // Selected hex
        if (selectedHexRef.current) {
          const pos = hexToPixel(selectedHexRef.current.q, selectedHexRef.current.r, hs);
          const px = pos.x - camX.current + vw / 2;
          const py = pos.y - camY.current + vh / 2;
          drawHexPath(layer, px, py, hs);
          layer.stroke({ color: 0xffff00, alpha: 1.0, width: 2 });
        }
      }

      // ---- Mouse events ----

      const canvas = app.canvas as HTMLCanvasElement;
      canvas.addEventListener("contextmenu", (e) => e.preventDefault(), { signal });

      canvas.addEventListener("mousedown", (e) => {
        mouseDownPos.current = { x: e.clientX, y: e.clientY };
        mousePos.current = { x: e.clientX, y: e.clientY };
        mouseIsDown.current = true;
        isDragging.current = false;
      }, { signal });

      window.addEventListener("mousemove", (e) => {
        if (!mouseIsDown.current) return;
        const dx = e.clientX - mouseDownPos.current.x;
        const dy = e.clientY - mouseDownPos.current.y;
        if (!isDragging.current && (Math.abs(dx) > DRAG_THRESHOLD || Math.abs(dy) > DRAG_THRESHOLD)) {
          isDragging.current = true;
        }
        if (isDragging.current) {
          camX.current -= e.clientX - mousePos.current.x;
          camY.current -= e.clientY - mousePos.current.y;
          mousePos.current = { x: e.clientX, y: e.clientY };
          hexDirty.current = true;
          highlightDirty.current = true;
        }
      }, { signal });

      canvas.addEventListener("mousemove", (e) => {
        if (isDragging.current) return;
        const rect = canvas.getBoundingClientRect();
        const sx = e.clientX - rect.left;
        const sy = e.clientY - rect.top;
        const worldX = camX.current + (sx - app.screen.width / 2);
        const worldY = camY.current + (sy - app.screen.height / 2);
        const hexCoords = pixelToHex(worldX, worldY, hexSize.current);

        if (!hoveredHexRef.current || hoveredHexRef.current.q !== hexCoords.q || hoveredHexRef.current.r !== hexCoords.r) {
          hoveredHexRef.current = hexCoords;
          setHoveredHex(hexCoords);
          highlightDirty.current = true;
        }
      }, { signal });

      canvas.addEventListener("mouseup", (e) => {
        if (mouseIsDown.current && !isDragging.current) {
          const rect = canvas.getBoundingClientRect();
          const sx = e.clientX - rect.left;
          const sy = e.clientY - rect.top;
          const worldX = camX.current + (sx - app.screen.width / 2);
          const worldY = camY.current + (sy - app.screen.height / 2);
          const hexCoords = pixelToHex(worldX, worldY, hexSize.current);

          if (selectedHexRef.current && selectedHexRef.current.q === hexCoords.q && selectedHexRef.current.r === hexCoords.r) {
            selectedHexRef.current = null;
            setSelectedHex(null);
            setShowBuildMenu(false);
          } else {
            selectedHexRef.current = hexCoords;
            setSelectedHex(hexCoords);
            setShowBuildMenu(true);
          }
          highlightDirty.current = true;
        }
        mouseIsDown.current = false;
        isDragging.current = false;
      }, { signal });

      window.addEventListener("mouseup", () => {
        mouseIsDown.current = false;
        isDragging.current = false;
      }, { signal });

      canvas.addEventListener("mouseleave", () => {
        if (!isDragging.current) {
          hoveredHexRef.current = null;
          setHoveredHex(null);
          highlightDirty.current = true;
        }
      }, { signal });

      // Zoom
      canvas.addEventListener("wheel", (e) => {
        e.preventDefault();
        const oldSize = hexSize.current;
        const direction = e.deltaY < 0 ? 1 : -1;
        const newSize = Math.max(MIN_HEX_SIZE, Math.min(MAX_HEX_SIZE, oldSize * Math.pow(ZOOM_STEP, direction)));
        if (newSize === oldSize) return;

        const rect = canvas.getBoundingClientRect();
        const sx = e.clientX - rect.left;
        const sy = e.clientY - rect.top;
        const world = { x: camX.current + (sx - app.screen.width / 2), y: camY.current + (sy - app.screen.height / 2) };
        const hexUnder = pixelToHex(world.x, world.y, oldSize);

        hexSize.current = newSize;
        setZoomLevel(newSize);

        const newPos = hexToPixel(hexUnder.q, hexUnder.r, newSize);
        camX.current = newPos.x - (sx - app.screen.width / 2);
        camY.current = newPos.y - (sy - app.screen.height / 2);

        hexDirty.current = true;
        highlightDirty.current = true;
      }, { signal, passive: false });

      // Keyboard
      const keys = new Set<string>();
      window.addEventListener("keydown", (e) => {
        keys.add(e.key.toLowerCase());
        if (e.key === "Escape") onExit();
      }, { signal });
      window.addEventListener("keyup", (e) => keys.delete(e.key.toLowerCase()), { signal });

      // Resize
      resizeObserver = new ResizeObserver(() => {
        hexDirty.current = true;
        highlightDirty.current = true;
      });
      if (containerRef.current) resizeObserver.observe(containerRef.current);

      // Ticker
      let renderFrameCount = 0;
      let lastFpsTime = performance.now();

      app.ticker.add(() => {
        const speed = 8;
        let moved = false;
        if (keys.has("w") || keys.has("arrowup")) { camY.current -= speed; moved = true; }
        if (keys.has("s") || keys.has("arrowdown")) { camY.current += speed; moved = true; }
        if (keys.has("a") || keys.has("arrowleft")) { camX.current -= speed; moved = true; }
        if (keys.has("s") || keys.has("arrowdown")) { camY.current += speed; moved = true; }
        if (keys.has("a") || keys.has("arrowleft")) { camX.current -= speed; moved = true; }
        if (keys.has("d") || keys.has("arrowright")) { camX.current += speed; moved = true; }

        if (moved) {
          hexDirty.current = true;
          highlightDirty.current = true;
        }

        let rendered = false;

        if (hexDirty.current) {
          hexDirty.current = false;
          drawBattlefield();
          drawBuildings();
          highlightDirty.current = true;
          rendered = true;
        }

        if (highlightDirty.current) {
          highlightDirty.current = false;
          drawHighlights();
          rendered = true;
        }

        if (rendered) renderFrameCount++;
        const now = performance.now();
        if (now - lastFpsTime > 1000) {
          setFps(Math.round((renderFrameCount * 1000) / (now - lastFpsTime)));
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
  }, [strategicQ, strategicR, macroTerrain, macroElevation, onExit]);

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

      {/* Loading */}
      {loading && !error && (
        <div className="absolute inset-0 flex items-center justify-center bg-[#1a1a2e] text-white z-10">
          <div className="text-center">
            <div className="text-2xl font-bold mb-2">Loading battlefield...</div>
            <div className="text-zinc-400">Hex ({strategicQ}, {strategicR})</div>
          </div>
        </div>
      )}

      {/* PixiJS canvas */}
      <div ref={containerRef} className="w-full h-full" />

      {/* Stats — top left */}
      <div className="absolute top-4 left-4 bg-black/70 text-white px-3 py-2 rounded text-sm font-mono pointer-events-none">
        <div>FPS: {fps}</div>
        <div>Hexes: {hexCount}</div>
        <div>Zoom: {zoomLevel.toFixed(1)}</div>
        <div className="text-zinc-400 mt-1">ESC — back to map</div>
      </div>

      {/* Breadcrumb — top center */}
      <div className="absolute top-4 left-1/2 -translate-x-1/2 bg-black/70 text-white px-4 py-2 rounded text-sm font-mono pointer-events-none">
        <span className="text-zinc-400">World</span>
        <span className="text-zinc-500 mx-2">{'>'}</span>
        <span className="text-amber-300">Hex ({strategicQ}, {strategicR})</span>
        <span className="text-zinc-500 mx-2">|</span>
        <span className="text-emerald-400">{macroTerrain}</span>
      </div>

      {/* Build menu — bottom center */}
      {showBuildMenu && selectedHex && (
        <div className="absolute bottom-4 left-1/2 -translate-x-1/2 bg-black/85 text-white px-4 py-3 rounded-lg font-mono text-sm pointer-events-auto max-w-2xl">
          <div className="text-zinc-400 text-xs mb-2 uppercase tracking-wider">
            Build at ({selectedHex.q}, {selectedHex.r})
          </div>
          <div className="flex flex-wrap gap-2">
            {buildingTypes
              .filter((bt) => bt.category === "Infrastructure")
              .map((bt) => (
                <button
                  key={bt.name}
                  onClick={() => handleBuild(bt.name)}
                  className="px-2 py-1 rounded text-xs hover:bg-white/20 transition-colors"
                  style={{
                    borderLeft: `3px solid rgb(${bt.color[0]}, ${bt.color[1]}, ${bt.color[2]})`,
                  }}
                  title={`${bt.label} — HP: ${bt.base_hp}, Power: ${bt.power_balance > 0 ? "+" : ""}${bt.power_balance}`}
                >
                  {bt.label}
                </button>
              ))}
          </div>
          <div className="flex flex-wrap gap-2 mt-2">
            {buildingTypes
              .filter((bt) => bt.category === "Defense")
              .map((bt) => (
                <button
                  key={bt.name}
                  onClick={() => handleBuild(bt.name)}
                  className="px-2 py-1 rounded text-xs hover:bg-white/20 transition-colors"
                  style={{
                    borderLeft: `3px solid rgb(${bt.color[0]}, ${bt.color[1]}, ${bt.color[2]})`,
                  }}
                  title={`${bt.label} — HP: ${bt.base_hp}`}
                >
                  {bt.label}
                </button>
              ))}
          </div>
          <div className="flex flex-wrap gap-2 mt-2">
            {buildingTypes
              .filter((bt) => bt.category === "Extraction")
              .map((bt) => (
                <button
                  key={bt.name}
                  onClick={() => handleBuild(bt.name)}
                  className="px-2 py-1 rounded text-xs hover:bg-white/20 transition-colors"
                  style={{
                    borderLeft: `3px solid rgb(${bt.color[0]}, ${bt.color[1]}, ${bt.color[2]})`,
                  }}
                  title={`${bt.label} — HP: ${bt.base_hp}`}
                >
                  {bt.label}
                </button>
              ))}
          </div>
          <button
            onClick={() => setShowBuildMenu(false)}
            className="mt-2 text-zinc-500 text-xs hover:text-white"
          >
            Close [ESC]
          </button>
        </div>
      )}

      {/* Hex info — bottom right */}
      {hoveredHex && (
        <div className="absolute bottom-4 right-4 bg-black/80 text-white px-4 py-3 rounded-lg font-mono text-sm pointer-events-none min-w-[180px]">
          <div className="text-zinc-400 text-xs mb-2 uppercase tracking-wider">Hex Info</div>
          <div>
            q: <span className="text-emerald-400">{hoveredHex.q}</span>, r:{" "}
            <span className="text-emerald-400">{hoveredHex.r}</span>
          </div>
        </div>
      )}
    </div>
  );
}
