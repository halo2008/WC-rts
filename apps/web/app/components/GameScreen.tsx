"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import dynamic from "next/dynamic";
import {
  api,
  type GroupDto,
  type Nation,
  type NationState,
  type UnitDto,
} from "../lib/api";
import { useStrategicWs } from "../lib/use-strategic-ws";
import ResourceHUD from "./ResourceHUD";
import type {
  CapitalMarker,
  StrategicGroupMarker,
  StrategicUnitMarker,
} from "./HexMap";

const HexMap = dynamic(() => import("./HexMap"), { ssr: false });

interface GameScreenProps {
  nation: Nation;
  onEnterBattlefield: (
    q: number,
    r: number,
    terrain: string,
    elevation: number,
  ) => void;
  onChangeNation: () => void;
}

/// Client-side live position (fractional hex). Populated from WS deltas and
/// shown on top of the integer `hex_q/hex_r` from the /api/units fetch.
interface LivePosition {
  from_q: number;
  from_r: number;
  to_q: number;
  to_r: number;
  fraction: number;
  status: string;
}

const DEFAULT_ENEMY_COLOR: [number, number, number] = [160, 160, 160];

function hexToRgb(hex: string): [number, number, number] {
  const m = hex.replace("#", "").match(/^([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i);
  if (!m) return DEFAULT_ENEMY_COLOR;
  return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16)];
}

export default function GameScreen({
  nation,
  onEnterBattlefield,
  onChangeNation,
}: GameScreenProps) {
  const [state, setState] = useState<NationState | null>(null);
  const [units, setUnits] = useState<UnitDto[]>([]);
  const [groups, setGroups] = useState<GroupDto[]>([]);
  const [selectedUnitId, setSelectedUnitId] = useState<number | null>(null);
  const [selectedGroupId, setSelectedGroupId] = useState<number | null>(null);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [nationColors, setNationColors] = useState<Record<string, [number, number, number]>>({});
  const [capitals, setCapitals] = useState<CapitalMarker[]>([]);
  const livePositionsRef = useRef(new Map<number, LivePosition>());
  const groupLivePositionsRef = useRef(new Map<number, LivePosition>());
  const [liveTick, setLiveTick] = useState(0);

  // Initial fetch — nations (for colour map), state, units and groups.
  useEffect(() => {
    let cancelled = false;
    Promise.all([
      api.listNations(),
      api.getNationState(nation.id),
      api.listUnits(),
      api.listGroups(),
    ])
      .then(([allNations, st, us, gs]) => {
        if (cancelled) return;
        const colors: Record<string, [number, number, number]> = {};
        const caps: CapitalMarker[] = [];
        for (const n of allNations) {
          colors[n.id] = hexToRgb(n.color);
          if (n.capital_q != null && n.capital_r != null) {
            caps.push({
              nation_code: n.code,
              hex_q: n.capital_q,
              hex_r: n.capital_r,
              color: hexToRgb(n.color),
            });
          }
        }
        setNationColors(colors);
        setCapitals(caps);
        setState(st);
        setUnits(us);
        setGroups(gs);
      })
      .catch((e) => {
        console.error("fetch game data:", e);
        setStatusMessage(`Błąd pobrania danych: ${String(e)}`);
      });
    return () => {
      cancelled = true;
    };
  }, [nation.id]);

  // Subscribe to the unified strategic WebSocket.
  useStrategicWs((msg) => {
    switch (msg.type) {
      case "transit": {
        setUnits((prev) => {
          const idx = prev.findIndex((x) => x.id === msg.unit_id);
          if (idx < 0) return prev;
          if (
            prev[idx].hex_q === msg.current_q &&
            prev[idx].hex_r === msg.current_r
          ) {
            return prev;
          }
          const next = prev.slice();
          next[idx] = { ...prev[idx], hex_q: msg.current_q, hex_r: msg.current_r };
          return next;
        });
        if (msg.status === "Active" || msg.status === "Blocked") {
          livePositionsRef.current.set(msg.unit_id, {
            from_q: msg.from_q,
            from_r: msg.from_r,
            to_q: msg.to_q,
            to_r: msg.to_r,
            fraction: msg.fraction,
            status: msg.status,
          });
        } else {
          livePositionsRef.current.delete(msg.unit_id);
        }
        setLiveTick((t) => t + 1);
        break;
      }

      case "nation_state": {
        if (msg.nation_id === nation.id) {
          setState((prev) =>
            prev
              ? {
                  ...prev,
                  stockpile: {
                    fuel: msg.fuel,
                    metals: msg.metals,
                    tech: msg.tech,
                    food: msg.food,
                  },
                  war_support: msg.war_support,
                  stability: msg.stability,
                }
              : prev,
          );
        }
        break;
      }

      case "unit_spawn": {
        setUnits((prev) => {
          if (prev.some((u) => u.id === msg.id)) return prev;
          return [
            ...prev,
            {
              id: msg.id,
              unit_type: msg.unit_type,
              nation_id: msg.nation_id,
              hex_q: msg.hex_q,
              hex_r: msg.hex_r,
              hp: msg.hp,
              max_hp: msg.max_hp,
              morale: 1.0,
              experience: 0.0,
            },
          ];
        });
        break;
      }

      case "group_update": {
        setGroups((prev) => {
          const idx = prev.findIndex((g) => g.id === msg.id);
          const next: GroupDto = {
            id: msg.id,
            nation_id: msg.nation_id,
            name: msg.name,
            hex_q: msg.hex_q,
            hex_r: msg.hex_r,
            member_unit_ids: msg.member_unit_ids,
          };
          if (idx < 0) return [...prev, next];
          const out = prev.slice();
          out[idx] = next;
          return out;
        });
        break;
      }

      case "group_disbanded": {
        setGroups((prev) => prev.filter((g) => g.id !== msg.id));
        groupLivePositionsRef.current.delete(msg.id);
        if (selectedGroupId === msg.id) setSelectedGroupId(null);
        break;
      }

      case "combat": {
        // Brief toast so the player knows a fight happened. Own nation in the
        // fight gets a louder label.
        const mine = msg.nations_involved.includes(nation.id);
        const who = mine ? "⚔ Twoje jednostki walczą" : "⚔ Walka na";
        setStatusMessage(
          `${who} (${msg.hex_q}, ${msg.hex_r}) — poległo ${msg.total_dead}`,
        );
        break;
      }

      case "unit_death": {
        // Drop the dead unit from local state; live interp and selection too.
        setUnits((prev) => prev.filter((u) => u.id !== msg.unit_id));
        livePositionsRef.current.delete(msg.unit_id);
        if (selectedUnitId === msg.unit_id) setSelectedUnitId(null);
        // If the unit belonged to a group, yank it from the client-side member list
        // so the group marker's count updates without waiting for a server push.
        setGroups((prev) =>
          prev.map((g) =>
            g.member_unit_ids.includes(msg.unit_id)
              ? {
                  ...g,
                  member_unit_ids: g.member_unit_ids.filter((id) => id !== msg.unit_id),
                }
              : g,
          ),
        );
        setLiveTick((t) => t + 1);
        break;
      }

      case "group_transit": {
        // Snap both the group record and every member unit to the authoritative hex.
        setGroups((prev) => {
          const idx = prev.findIndex((g) => g.id === msg.group_id);
          if (idx < 0) return prev;
          if (
            prev[idx].hex_q === msg.current_q &&
            prev[idx].hex_r === msg.current_r
          ) {
            return prev;
          }
          const out = prev.slice();
          out[idx] = { ...prev[idx], hex_q: msg.current_q, hex_r: msg.current_r };
          return out;
        });
        setUnits((prev) => {
          const grp = groups.find((g) => g.id === msg.group_id);
          if (!grp) return prev;
          const memberSet = new Set(grp.member_unit_ids);
          let changed = false;
          const out = prev.map((u) => {
            if (!memberSet.has(u.id)) return u;
            if (u.hex_q === msg.current_q && u.hex_r === msg.current_r) return u;
            changed = true;
            return { ...u, hex_q: msg.current_q, hex_r: msg.current_r };
          });
          return changed ? out : prev;
        });
        if (msg.status === "Active" || msg.status === "Blocked") {
          groupLivePositionsRef.current.set(msg.group_id, {
            from_q: msg.from_q,
            from_r: msg.from_r,
            to_q: msg.to_q,
            to_r: msg.to_r,
            fraction: msg.fraction,
            status: msg.status,
          });
        } else {
          groupLivePositionsRef.current.delete(msg.group_id);
        }
        setLiveTick((t) => t + 1);
        break;
      }
    }
  });

  // Units currently bound to a group — they are rendered as part of the
  // group marker, not as individual dots.
  const unitIdsInGroups = useMemo(() => {
    const s = new Set<number>();
    for (const g of groups) for (const id of g.member_unit_ids) s.add(id);
    return s;
  }, [groups]);

  // Build the markers the map renders. Fold in live interpolation and
  // resolve per-nation colours. Units that belong to a group are hidden —
  // they show up as part of the group marker instead.
  const markers: StrategicUnitMarker[] = useMemo(() => {
    void liveTick;
    return units
      .filter((u) => !unitIdsInGroups.has(u.id))
      .map((u) => {
        const live = livePositionsRef.current.get(u.id);
        return {
          id: u.id,
          hex_q: u.hex_q,
          hex_r: u.hex_r,
          color: nationColors[u.nation_id] ?? DEFAULT_ENEMY_COLOR,
          interp: live
            ? {
                from_q: live.from_q,
                from_r: live.from_r,
                to_q: live.to_q,
                to_r: live.to_r,
                fraction: live.fraction,
              }
            : undefined,
        };
      });
  }, [units, liveTick, nationColors, unitIdsInGroups]);

  const groupMarkers: StrategicGroupMarker[] = useMemo(() => {
    void liveTick;
    return groups.map((g) => {
      const live = groupLivePositionsRef.current.get(g.id);
      return {
        id: g.id,
        hex_q: g.hex_q,
        hex_r: g.hex_r,
        color: nationColors[g.nation_id] ?? DEFAULT_ENEMY_COLOR,
        member_count: g.member_unit_ids.length,
        interp: live
          ? {
              from_q: live.from_q,
              from_r: live.from_r,
              to_q: live.to_q,
              to_r: live.to_r,
              fraction: live.fraction,
            }
          : undefined,
      };
    });
  }, [groups, liveTick, nationColors]);

  const selectedUnit = units.find((u) => u.id === selectedUnitId) ?? null;
  const selectedGroup = groups.find((g) => g.id === selectedGroupId) ?? null;

  const selectedGroupMembers = useMemo(() => {
    if (!selectedGroup) return [];
    const memberSet = new Set(selectedGroup.member_unit_ids);
    return units.filter((u) => memberSet.has(u.id));
  }, [selectedGroup, units]);

  const handleHexAction = useCallback(
    (q: number, r: number) => {
      // 1. If a group is selected — commanding a move order.
      if (selectedGroup && selectedGroup.nation_id === nation.id) {
        if (selectedGroup.hex_q === q && selectedGroup.hex_r === r) {
          setStatusMessage("Grupa już jest na tym heksie.");
          return;
        }
        setStatusMessage(`Ruch grupy #${selectedGroup.id} → (${q}, ${r})…`);
        api
          .moveGroup(selectedGroup.id, q, r)
          .then(() => {
            setStatusMessage(`Rozkaz dla grupy wysłany.`);
          })
          .catch((e) => setStatusMessage(`Błąd ruchu grupy: ${String(e)}`));
        return;
      }

      // 2. If a single unit is selected — normal move order.
      if (selectedUnit && selectedUnit.nation_id === nation.id) {
        if (selectedUnit.hex_q === q && selectedUnit.hex_r === r) {
          setStatusMessage("Jednostka już jest na tym heksie.");
          return;
        }
        setStatusMessage(`Ruch jednostki #${selectedUnit.id} → (${q}, ${r})…`);
        api
          .moveUnit(selectedUnit.id, q, r)
          .then((transit) => {
            setStatusMessage(
              `Ruch zatwierdzony (${transit.path.length - 1} heksów).`,
            );
          })
          .catch((e) => {
            setStatusMessage(`Błąd ruchu: ${String(e)}`);
          });
        return;
      }

      // 3. Nothing selected — try to select a group first, then a unit.
      const group = groups.find(
        (g) => g.hex_q === q && g.hex_r === r && g.nation_id === nation.id,
      );
      if (group) {
        setSelectedGroupId(group.id);
        setSelectedUnitId(null);
        setStatusMessage(
          `Wybrano grupę "${group.name}" (${group.member_unit_ids.length} jedn.). Kliknij cel aby ruszyć.`,
        );
        return;
      }

      const friendly = units.find(
        (u) =>
          u.hex_q === q &&
          u.hex_r === r &&
          u.nation_id === nation.id &&
          !unitIdsInGroups.has(u.id),
      );
      if (friendly) {
        setSelectedUnitId(friendly.id);
        setSelectedGroupId(null);
        setStatusMessage(
          `Wybrano #${friendly.id} ${friendly.unit_type} — kliknij cel aby ruszyć.`,
        );
        return;
      }
      const any = units.find((u) => u.hex_q === q && u.hex_r === r);
      if (any) {
        setStatusMessage(`Obca jednostka ${any.unit_type} na (${q}, ${r}).`);
        setSelectedUnitId(null);
        setSelectedGroupId(null);
      } else {
        setSelectedUnitId(null);
        setSelectedGroupId(null);
        setStatusMessage(null);
      }
    },
    [selectedUnit, selectedGroup, units, groups, nation.id, unitIdsInGroups],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setSelectedUnitId(null);
        setSelectedGroupId(null);
        setStatusMessage(null);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const cancelTransit = useCallback(() => {
    if (!selectedUnit) return;
    api
      .cancelTransit(selectedUnit.id)
      .then(() => {
        livePositionsRef.current.delete(selectedUnit.id);
        setLiveTick((t) => t + 1);
        setStatusMessage(`Ruch jednostki #${selectedUnit.id} anulowany.`);
      })
      .catch((e) => setStatusMessage(`Błąd anulowania: ${String(e)}`));
  }, [selectedUnit]);

  const cancelGroupTransit = useCallback(() => {
    if (!selectedGroup) return;
    api
      .cancelGroupTransit(selectedGroup.id)
      .then(() => {
        groupLivePositionsRef.current.delete(selectedGroup.id);
        setLiveTick((t) => t + 1);
        setStatusMessage(`Ruch grupy "${selectedGroup.name}" anulowany.`);
      })
      .catch((e) => setStatusMessage(`Błąd anulowania: ${String(e)}`));
  }, [selectedGroup]);

  const spawnTestUnit = useCallback(() => {
    if (!state) return;
    api
      .createUnit("Infantry", nation.id, state.capital_q, state.capital_r)
      .then((u) => {
        setUnits((prev) => (prev.some((x) => x.id === u.id) ? prev : [...prev, u]));
        setStatusMessage(`Utworzono piechotę w stolicy (${u.hex_q}, ${u.hex_r}).`);
      })
      .catch((e) => setStatusMessage(`Błąd tworzenia jednostki: ${String(e)}`));
  }, [nation.id, state]);

  const queueArmor = useCallback(() => {
    if (!state) return;
    api
      .enqueueProduction(nation.id, "Armor", state.capital_q, state.capital_r)
      .then((order) => {
        setStatusMessage(
          `Czołg w kolejce — ${order.total_time.toFixed(0)}s do ukończenia.`,
        );
      })
      .catch((e) => {
        const msg = String(e);
        if (msg.includes("402")) {
          setStatusMessage("Za mało zasobów (metal/paliwo) na produkcję czołgu.");
        } else {
          setStatusMessage(`Błąd kolejki: ${msg}`);
        }
      });
  }, [nation.id, state]);

  /// Group every ungrouped friendly unit on the selected unit's hex.
  /// Simplest MVP flow — "select a unit, hit Grupuj heks, all my free units
  /// on the same hex become an army group". Multi-select comes next.
  const groupHex = useCallback(() => {
    if (!selectedUnit) return;
    if (selectedUnit.nation_id !== nation.id) return;
    const candidates = units.filter(
      (u) =>
        u.hex_q === selectedUnit.hex_q &&
        u.hex_r === selectedUnit.hex_r &&
        u.nation_id === nation.id &&
        !unitIdsInGroups.has(u.id),
    );
    if (candidates.length < 2) {
      setStatusMessage("Potrzebujesz ≥ 2 wolnych jednostek na tym samym heksie.");
      return;
    }
    const defaultName = `Grupa ${groups.length + 1}`;
    api
      .createGroup(
        nation.id,
        defaultName,
        candidates.map((u) => u.id),
      )
      .then((g) => {
        setGroups((prev) => (prev.some((x) => x.id === g.id) ? prev : [...prev, g]));
        setSelectedUnitId(null);
        setSelectedGroupId(g.id);
        setStatusMessage(`Utworzono "${g.name}" (${g.member_unit_ids.length} jedn.).`);
      })
      .catch((e) => setStatusMessage(`Błąd grupy: ${String(e)}`));
  }, [selectedUnit, units, nation.id, unitIdsInGroups, groups.length]);

  const disbandGroup = useCallback(() => {
    if (!selectedGroup) return;
    api
      .disbandGroup(selectedGroup.id)
      .then(() => {
        setGroups((prev) => prev.filter((g) => g.id !== selectedGroup.id));
        setSelectedGroupId(null);
        setStatusMessage(`Grupa "${selectedGroup.name}" rozwiązana.`);
      })
      .catch((e) => setStatusMessage(`Błąd rozwiązania: ${String(e)}`));
  }, [selectedGroup]);

  const hexHasFreeUnits =
    selectedUnit &&
    units.filter(
      (u) =>
        u.hex_q === selectedUnit.hex_q &&
        u.hex_r === selectedUnit.hex_r &&
        u.nation_id === nation.id &&
        !unitIdsInGroups.has(u.id),
    ).length >= 2;

  return (
    <div className="relative w-full h-screen">
      <ResourceHUD nation={nation} state={state} onChangeNation={onChangeNation} />

      <div className="absolute inset-0 pt-10">
        <HexMap
          onEnterBattlefield={onEnterBattlefield}
          units={markers}
          groups={groupMarkers}
          capitals={capitals}
          selectedUnitId={selectedUnitId}
          selectedGroupId={selectedGroupId}
          onHexAction={handleHexAction}
        />
      </div>

      {selectedUnit && (
        <div className="absolute bottom-4 left-1/2 -translate-x-1/2 bg-black/85 text-white px-4 py-3 rounded-lg font-mono text-sm border border-zinc-700 min-w-[280px]">
          <div className="flex items-center justify-between mb-1">
            <span className="text-zinc-400 text-xs uppercase tracking-wider">
              Jednostka #{selectedUnit.id}
            </span>
            <button
              onClick={() => setSelectedUnitId(null)}
              className="text-zinc-500 hover:text-white text-xs"
              aria-label="Odznacz"
            >
              ✕
            </button>
          </div>
          <div className="text-lg">{selectedUnit.unit_type}</div>
          <div className="text-zinc-400 text-xs mt-1">
            Pozycja: ({selectedUnit.hex_q}, {selectedUnit.hex_r})
          </div>
          <div className="text-zinc-400 text-xs">
            HP: {selectedUnit.hp} / {selectedUnit.max_hp}
            {" · "}
            Morale: {(selectedUnit.morale * 100).toFixed(0)}%
          </div>
          {livePositionsRef.current.has(selectedUnit.id) && (
            <button
              onClick={cancelTransit}
              className="mt-2 w-full px-3 py-1.5 bg-red-600/80 hover:bg-red-500 text-white text-xs rounded"
            >
              Anuluj ruch
            </button>
          )}
          {hexHasFreeUnits && (
            <button
              onClick={groupHex}
              className="mt-2 w-full px-3 py-1.5 bg-sky-600/80 hover:bg-sky-500 text-white text-xs rounded"
              title="Łączy wszystkie wolne jednostki tego państwa stojące na tym heksie w jedną grupę."
            >
              🪖 Grupuj ten heks
            </button>
          )}
          <div className="mt-2 text-zinc-500 text-xs">
            Kliknij docelowy heks aby wydać rozkaz. Esc — odznacz.
          </div>
        </div>
      )}

      {selectedGroup && (
        <div className="absolute bottom-4 left-1/2 -translate-x-1/2 bg-black/85 text-white px-4 py-3 rounded-lg font-mono text-sm border border-sky-700 min-w-[320px]">
          <div className="flex items-center justify-between mb-1">
            <span className="text-sky-300 text-xs uppercase tracking-wider">
              Grupa #{selectedGroup.id}
            </span>
            <button
              onClick={() => setSelectedGroupId(null)}
              className="text-zinc-500 hover:text-white text-xs"
              aria-label="Odznacz"
            >
              ✕
            </button>
          </div>
          <div className="text-lg">{selectedGroup.name}</div>
          <div className="text-zinc-400 text-xs mt-1">
            Pozycja: ({selectedGroup.hex_q}, {selectedGroup.hex_r}) ·
            {" "}{selectedGroup.member_unit_ids.length} jedn.
          </div>
          <div className="mt-2 text-xs text-zinc-400 max-h-24 overflow-y-auto">
            {selectedGroupMembers.map((m) => (
              <div key={m.id}>
                #{m.id} {m.unit_type} — HP {m.hp}/{m.max_hp}
              </div>
            ))}
          </div>
          <div className="grid grid-cols-2 gap-2 mt-2">
            {groupLivePositionsRef.current.has(selectedGroup.id) && (
              <button
                onClick={cancelGroupTransit}
                className="px-3 py-1.5 bg-red-600/80 hover:bg-red-500 text-white text-xs rounded"
              >
                Anuluj ruch
              </button>
            )}
            <button
              onClick={disbandGroup}
              className="col-span-2 px-3 py-1.5 bg-zinc-700/80 hover:bg-zinc-600 text-white text-xs rounded"
            >
              Rozwiąż grupę
            </button>
          </div>
          <div className="mt-2 text-zinc-500 text-xs">
            Kliknij heks aby ruszyć całą grupę. Tempo grupy = tempo najwolniejszej jednostki.
          </div>
        </div>
      )}

      <div className="absolute top-14 right-4 flex flex-col gap-2 z-10">
        <button
          onClick={spawnTestUnit}
          className="px-3 py-2 bg-emerald-700/80 hover:bg-emerald-600 text-white text-sm rounded shadow-lg"
        >
          + Piechota (stolica)
        </button>
        <button
          onClick={queueArmor}
          className="px-3 py-2 bg-amber-700/80 hover:bg-amber-600 text-white text-sm rounded shadow-lg"
          title="Buduje w 160s — pobiera 400 metalu + 80 paliwa"
        >
          🛠 Produkuj czołg
        </button>
      </div>

      {statusMessage && (
        <div className="absolute top-12 left-1/2 -translate-x-1/2 bg-black/70 text-white px-3 py-1.5 rounded text-xs font-mono border border-zinc-700">
          {statusMessage}
        </div>
      )}
    </div>
  );
}
