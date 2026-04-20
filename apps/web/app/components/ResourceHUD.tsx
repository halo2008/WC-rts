"use client";

import type { Nation, NationState } from "../lib/api";

interface ResourceHUDProps {
  nation: Nation;
  state: NationState | null;
  onChangeNation: () => void;
}

function fmt(n: number): string {
  if (!Number.isFinite(n)) return "—";
  if (Math.abs(n) >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return n.toFixed(0);
}

export default function ResourceHUD({ nation, state, onChangeNation }: ResourceHUDProps) {
  return (
    <div className="absolute top-0 left-0 right-0 bg-black/75 text-white px-4 py-2 flex items-center gap-4 z-10 font-mono text-sm border-b border-zinc-700">
      <div className="flex items-center gap-2">
        <span
          className="w-4 h-4 rounded border border-white/20"
          style={{ backgroundColor: nation.color }}
          aria-hidden
        />
        <span className="font-semibold">{nation.name}</span>
        <span className="text-zinc-500 text-xs">{nation.code}</span>
      </div>

      <div className="h-6 w-px bg-zinc-700" />

      {state ? (
        <div className="flex items-center gap-5">
          <ResourceBadge label="⛽ Paliwo" value={state.stockpile.fuel} color="text-amber-300" />
          <ResourceBadge label="⚙ Metale" value={state.stockpile.metals} color="text-zinc-300" />
          <ResourceBadge label="🔬 Tech" value={state.stockpile.tech} color="text-sky-300" />
          <ResourceBadge label="🌾 Żywność" value={state.stockpile.food} color="text-lime-300" />
        </div>
      ) : (
        <span className="text-zinc-500">Ładowanie stanu…</span>
      )}

      {state && (
        <>
          <div className="h-6 w-px bg-zinc-700" />
          <div className="flex items-center gap-4 text-xs text-zinc-400">
            <span>
              Poparcie: <span className="text-white">{Math.round(state.war_support * 100)}%</span>
            </span>
            <span>
              Stabilność: <span className="text-white">{Math.round(state.stability * 100)}%</span>
            </span>
          </div>
        </>
      )}

      <div className="ml-auto">
        <button
          onClick={onChangeNation}
          className="text-xs text-zinc-400 hover:text-white underline underline-offset-2"
        >
          Zmień państwo
        </button>
      </div>
    </div>
  );
}

function ResourceBadge({
  label,
  value,
  color,
}: {
  label: string;
  value: number;
  color: string;
}) {
  return (
    <div>
      <span className="text-zinc-500 text-xs">{label}</span>{" "}
      <span className={color}>{fmt(value)}</span>
    </div>
  );
}
