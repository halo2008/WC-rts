"use client";

import { useEffect, useState } from "react";
import { api, type Nation } from "../lib/api";

interface NationPickerProps {
  onSelect: (nation: Nation) => void;
}

const TIER_LABEL: Record<number, string> = {
  1: "Supermocarstwo",
  2: "Mocarstwo",
  3: "Państwo regionalne",
  4: "Państwo mniejsze",
};

export default function NationPicker({ onSelect }: NationPickerProps) {
  const [nations, setNations] = useState<Nation[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .listNations()
      .then((list) => {
        setNations(list);
        setLoading(false);
      })
      .catch((e) => {
        setError(String(e));
        setLoading(false);
      });
  }, []);

  return (
    <div className="min-h-screen bg-zinc-950 text-white flex items-center justify-center p-8">
      <div className="w-full max-w-4xl">
        <h1 className="text-4xl font-bold mb-2 text-center">Grand Strategy</h1>
        <p className="text-zinc-400 text-center mb-10">
          Wybierz państwo, którym chcesz pokierować
        </p>

        {loading && <p className="text-zinc-500 text-center">Ładowanie państw…</p>}

        {error && (
          <div className="text-red-400 text-center bg-red-950/40 border border-red-800 p-4 rounded">
            Błąd pobierania listy państw: {error}
            <div className="text-zinc-500 text-sm mt-2">
              Upewnij się że game-server działa na {process.env.NEXT_PUBLIC_GAME_API ?? "http://localhost:3000"}
            </div>
          </div>
        )}

        {!loading && !error && nations.length === 0 && (
          <p className="text-zinc-400 text-center">
            Brak państw w bazie. Uruchom migracje i seed.
          </p>
        )}

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          {nations.map((n) => (
            <button
              key={n.id}
              onClick={() => onSelect(n)}
              className="group text-left bg-zinc-900 hover:bg-zinc-800 border border-zinc-800 hover:border-zinc-600 rounded-lg p-5 transition-colors"
            >
              <div className="flex items-center gap-3 mb-2">
                <span
                  className="w-5 h-5 rounded border border-white/20"
                  style={{ backgroundColor: n.color }}
                  aria-hidden
                />
                <span className="text-xl font-semibold">{n.name}</span>
                <span className="ml-auto text-xs font-mono text-zinc-500 group-hover:text-zinc-300">
                  {n.code}
                </span>
              </div>
              <div className="text-sm text-zinc-400">
                {TIER_LABEL[n.tier] ?? `Tier ${n.tier}`} · {n.government_type}
              </div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
