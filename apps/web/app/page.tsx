"use client";

import { useState, useCallback } from "react";
import dynamic from "next/dynamic";

const HexMap = dynamic(() => import("./components/HexMap"), { ssr: false });
const BattlefieldMap = dynamic(() => import("./components/BattlefieldMap"), { ssr: false });

interface BattlefieldTarget {
  q: number;
  r: number;
  terrain: string;
  elevation: number;
}

export default function Home() {
  const [battlefield, setBattlefield] = useState<BattlefieldTarget | null>(null);

  const handleEnterBattlefield = useCallback((q: number, r: number, terrain: string, elevation: number) => {
    setBattlefield({ q, r, terrain, elevation });
  }, []);

  const handleExitBattlefield = useCallback(() => {
    setBattlefield(null);
  }, []);

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

  return <HexMap onEnterBattlefield={handleEnterBattlefield} />;
}
