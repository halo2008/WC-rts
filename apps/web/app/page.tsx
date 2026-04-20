"use client";

import { useCallback, useEffect, useState } from "react";
import dynamic from "next/dynamic";
import NationPicker from "./components/NationPicker";
import type { Nation } from "./lib/api";

const GameScreen = dynamic(() => import("./components/GameScreen"), { ssr: false });
const BattlefieldMap = dynamic(() => import("./components/BattlefieldMap"), { ssr: false });

interface BattlefieldTarget {
  q: number;
  r: number;
  terrain: string;
  elevation: number;
}

const NATION_STORAGE_KEY = "wc-rts:selected-nation";

export default function Home() {
  const [nation, setNation] = useState<Nation | null>(null);
  const [hydrated, setHydrated] = useState(false);
  const [battlefield, setBattlefield] = useState<BattlefieldTarget | null>(null);

  // Restore previously selected nation from localStorage on mount.
  useEffect(() => {
    try {
      const raw = localStorage.getItem(NATION_STORAGE_KEY);
      if (raw) setNation(JSON.parse(raw) as Nation);
    } catch {
      // Ignore parse errors; user will re-pick.
    }
    setHydrated(true);
  }, []);

  const handlePickNation = useCallback((n: Nation) => {
    setNation(n);
    try {
      localStorage.setItem(NATION_STORAGE_KEY, JSON.stringify(n));
    } catch {
      // Storage disabled — that's fine, the pick lives for this session.
    }
  }, []);

  const handleChangeNation = useCallback(() => {
    setNation(null);
    setBattlefield(null);
    try {
      localStorage.removeItem(NATION_STORAGE_KEY);
    } catch {
      // ignore
    }
  }, []);

  const handleEnterBattlefield = useCallback(
    (q: number, r: number, terrain: string, elevation: number) => {
      setBattlefield({ q, r, terrain, elevation });
    },
    [],
  );

  const handleExitBattlefield = useCallback(() => {
    setBattlefield(null);
  }, []);

  if (!hydrated) {
    return <div className="min-h-screen bg-zinc-950" />;
  }

  if (!nation) {
    return <NationPicker onSelect={handlePickNation} />;
  }

  if (battlefield) {
    return (
      <BattlefieldMap
        strategicQ={battlefield.q}
        strategicR={battlefield.r}
        macroTerrain={battlefield.terrain}
        macroElevation={battlefield.elevation}
        onExit={handleExitBattlefield}
      />
    );
  }

  return (
    <GameScreen
      nation={nation}
      onEnterBattlefield={handleEnterBattlefield}
      onChangeNation={handleChangeNation}
    />
  );
}
