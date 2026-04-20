"use client";

import { useEffect, useRef, useCallback } from "react";
import { Application, Graphics } from "pixi.js";
import { loadWasm, type WasmApi } from "../lib/wasm-loader";

const GRID_WIDTH = 1200;
const GRID_HEIGHT = 600;
const MINIMAP_SCALE = 0.15; // Each hex = ~1px at this scale

interface MinimapProps {
  camX: number;
  camY: number;
  hexSize: number;
  viewWidth: number;
  viewHeight: number;
  onNavigate: (x: number, y: number) => void;
}

export default function Minimap({
  camX,
  camY,
  hexSize,
  viewWidth,
  viewHeight,
  onNavigate,
}: MinimapProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const appRef = useRef<Application | null>(null);
  const viewportRectRef = useRef<Graphics | null>(null);
  const wasmRef = useRef<WasmApi | null>(null);

  const mapPixelWidth = GRID_WIDTH * Math.sqrt(3) * MINIMAP_SCALE;
  const mapPixelHeight = GRID_HEIGHT * 1.5 * MINIMAP_SCALE;

  // Initialize minimap
  useEffect(() => {
    let mounted = true;

    async function init() {
      let wasm: WasmApi;
      try {
        wasm = await loadWasm();
      } catch (e) {
        console.error("Minimap WASM load failed:", e);
        return;
      }

      if (!mounted) return;
      wasmRef.current = wasm;

      const app = new Application();
      await app.init({
        width: mapPixelWidth,
        height: mapPixelHeight,
        background: 0x1a1a2e,
        antialias: false,
        resolution: 1,
      });

      if (!mounted) {
        app.destroy(true);
        return;
      }

      containerRef.current?.appendChild(app.canvas);
      appRef.current = app;

      // Draw terrain overview — sample every Nth hex for performance
      const terrainGraphics = new Graphics();
      const step = 4; // Sample every 4th hex for minimap
      const miniHexSize = MINIMAP_SCALE;

      for (let r = 0; r < GRID_HEIGHT; r += step) {
        for (let q = 0; q < GRID_WIDTH; q += step) {
          const cell = wasm.get_hex_by_coords(q, r);
          if (!cell) continue;

          const px = miniHexSize * (Math.sqrt(3) * q + (Math.sqrt(3) / 2) * r);
          const py = miniHexSize * 1.5 * r;

          const [cr, cg, cb] = cell.color;
          const color = (cr << 16) | (cg << 8) | cb;
          terrainGraphics.rect(px, py, step * miniHexSize * Math.sqrt(3), step * miniHexSize * 1.5);
          terrainGraphics.fill(color);
        }
      }

      app.stage.addChild(terrainGraphics);

      // Viewport rectangle
      const viewportRect = new Graphics();
      app.stage.addChild(viewportRect);
      viewportRectRef.current = viewportRect;
    }

    init();

    return () => {
      mounted = false;
      if (appRef.current) {
        appRef.current.destroy(true);
        appRef.current = null;
      }
    };
  }, [mapPixelWidth, mapPixelHeight]);

  // Update viewport rectangle. camX/camY in HexMap is the viewport *center* in
  // world pixels, so we shift by half the viewport when drawing the frame.
  useEffect(() => {
    const rect = viewportRectRef.current;
    if (!rect) return;

    const scale = MINIMAP_SCALE / hexSize;
    const vw = viewWidth * scale;
    const vh = viewHeight * scale;
    const vx = camX * scale - vw / 2;
    const vy = camY * scale - vh / 2;

    rect.clear();
    rect.rect(vx, vy, vw, vh);
    rect.stroke({ color: 0xffffff, width: 1, alpha: 0.9 });
  }, [camX, camY, hexSize, viewWidth, viewHeight]);

  // Click to navigate — the clicked world pixel becomes the new camera centre.
  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const clickX = e.clientX - rect.left;
      const clickY = e.clientY - rect.top;

      const scale = MINIMAP_SCALE / hexSize;
      const worldX = clickX / scale;
      const worldY = clickY / scale;

      onNavigate(worldX, worldY);
    },
    [hexSize, onNavigate]
  );

  return (
    <div
      className="relative border border-gray-600 rounded overflow-hidden cursor-crosshair"
      style={{ width: mapPixelWidth, height: mapPixelHeight }}
      ref={containerRef}
      onClick={handleClick}
    />
  );
}
