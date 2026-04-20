/**
 * API base URLs. Override via `NEXT_PUBLIC_GAME_API` / `NEXT_PUBLIC_GAME_WS`
 * for staging / production. Defaults assume game-server on localhost:3000.
 */

export const API_BASE =
  process.env.NEXT_PUBLIC_GAME_API ?? "http://localhost:3000";
export const WS_BASE =
  process.env.NEXT_PUBLIC_GAME_WS ?? "ws://localhost:3000";

// ─── Types (mirror server JSON) ─────────────────────────────────────

export interface Nation {
  id: string;
  code: string;
  name: string;
  government_type: string;
  tier: number;
  color: string;
  capital_q: number | null;
  capital_r: number | null;
}

export interface NationState {
  nation_id: string;
  stockpile: {
    fuel: number;
    metals: number;
    tech: number;
    food: number;
  };
  budget: {
    military: number;
    economy: number;
    research: number;
    social: number;
    intel: number;
  };
  capital_q: number;
  capital_r: number;
  war_support: number;
  stability: number;
}

export interface UnitDto {
  id: number;
  unit_type: string;
  nation_id: string;
  hex_q: number;
  hex_r: number;
  hp: number;
  max_hp: number;
  morale: number;
  experience: number;
}

export interface HexPoint {
  q: number;
  r: number;
}

export interface TransitDto {
  unit_id: number;
  status: "Active" | "Completed" | "Cancelled" | "Blocked";
  progress_hex: number;
  path: HexPoint[];
  current_hex: HexPoint;
}

export interface TickUpdate {
  unit_id: number;
  status: string;
  progress_hex: number;
  from_q: number;
  from_r: number;
  to_q: number;
  to_r: number;
  fraction: number;
  current_q: number;
  current_r: number;
}

export interface NationStatePush {
  nation_id: string;
  fuel: number;
  metals: number;
  tech: number;
  food: number;
  war_support: number;
  stability: number;
}

export interface UnitSpawn {
  id: number;
  unit_type: string;
  nation_id: string;
  hex_q: number;
  hex_r: number;
  hp: number;
  max_hp: number;
}

export interface GroupDto {
  id: number;
  nation_id: string;
  name: string;
  hex_q: number;
  hex_r: number;
  member_unit_ids: number[];
}

export interface GroupUpdatePush extends GroupDto {}

export interface GroupDisbandedPush {
  id: number;
}

export interface GroupTickUpdate {
  group_id: number;
  status: string;
  progress_hex: number;
  from_q: number;
  from_r: number;
  to_q: number;
  to_r: number;
  fraction: number;
  current_q: number;
  current_r: number;
}

export interface CombatPush {
  hex_q: number;
  hex_r: number;
  nations_involved: string[];
  total_dead: number;
}

export interface UnitDeathPush {
  unit_id: number;
}

/// Tagged union matching the `WsMessage` enum on the server.
export type WsMessage =
  | ({ type: "transit" } & TickUpdate)
  | ({ type: "nation_state" } & NationStatePush)
  | ({ type: "unit_spawn" } & UnitSpawn)
  | ({ type: "group_update" } & GroupUpdatePush)
  | ({ type: "group_disbanded" } & GroupDisbandedPush)
  | ({ type: "group_transit" } & GroupTickUpdate)
  | ({ type: "combat" } & CombatPush)
  | ({ type: "unit_death" } & UnitDeathPush);

export interface ProductionOrder {
  id: number;
  nation_id: string;
  unit_type: string;
  spawn_hex_q: number;
  spawn_hex_r: number;
  progress: number;
  total_time: number;
  status: "InProgress" | "Completed" | "Cancelled";
}

// ─── Fetch helpers ──────────────────────────────────────────────────

async function apiFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  if (!res.ok) {
    throw new Error(`${init?.method ?? "GET"} ${path} → ${res.status}`);
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export const api = {
  listNations: () => apiFetch<Nation[]>("/api/nations"),
  getNationState: (id: string) => apiFetch<NationState>(`/api/nations/${id}/state`),
  listUnits: (nationId?: string) =>
    apiFetch<UnitDto[]>(
      nationId ? `/api/units?nation_id=${nationId}` : "/api/units",
    ),
  moveUnit: (unitId: number, goalQ: number, goalR: number, width = 1200) =>
    apiFetch<TransitDto>(`/api/units/${unitId}/move`, {
      method: "POST",
      body: JSON.stringify({ goal_q: goalQ, goal_r: goalR, width }),
    }),
  cancelTransit: (unitId: number) =>
    apiFetch<void>(`/api/units/${unitId}/cancel`, { method: "POST" }),
  getTransit: (unitId: number) =>
    apiFetch<TransitDto>(`/api/units/${unitId}/transit`),
  createUnit: (
    unitType: string,
    nationId: string,
    hexQ: number,
    hexR: number,
  ) =>
    apiFetch<UnitDto>(`/api/units`, {
      method: "POST",
      body: JSON.stringify({
        unit_type: unitType,
        nation_id: nationId,
        hex_q: hexQ,
        hex_r: hexR,
      }),
    }),
  listProduction: (nationId: string) =>
    apiFetch<ProductionOrder[]>(
      `/api/units/production?nation_id=${nationId}`,
    ),
  enqueueProduction: (
    nationId: string,
    unitType: string,
    spawnQ: number,
    spawnR: number,
  ) =>
    apiFetch<ProductionOrder>(`/api/units/production`, {
      method: "POST",
      body: JSON.stringify({
        nation_id: nationId,
        unit_type: unitType,
        spawn_hex_q: spawnQ,
        spawn_hex_r: spawnR,
      }),
    }),

  // ─── Groups (Etap 3 — HoI-style army groups) ──────────────────────
  listGroups: (nationId?: string) =>
    apiFetch<GroupDto[]>(
      nationId ? `/api/groups?nation_id=${nationId}` : `/api/groups`,
    ),
  createGroup: (nationId: string, name: string, unitIds: number[]) =>
    apiFetch<GroupDto>(`/api/groups`, {
      method: "POST",
      body: JSON.stringify({
        nation_id: nationId,
        name,
        unit_ids: unitIds,
      }),
    }),
  disbandGroup: (groupId: number) =>
    apiFetch<void>(`/api/groups/${groupId}/disband`, { method: "POST" }),
  moveGroup: (groupId: number, goalQ: number, goalR: number, width = 1200) =>
    apiFetch<void>(`/api/groups/${groupId}/move`, {
      method: "POST",
      body: JSON.stringify({ goal_q: goalQ, goal_r: goalR, width }),
    }),
  cancelGroupTransit: (groupId: number) =>
    apiFetch<void>(`/api/groups/${groupId}/cancel`, { method: "POST" }),
  addUnitsToGroup: (groupId: number, unitIds: number[]) =>
    apiFetch<GroupDto>(`/api/groups/${groupId}/add`, {
      method: "POST",
      body: JSON.stringify({ unit_ids: unitIds }),
    }),
  removeUnitsFromGroup: (groupId: number, unitIds: number[]) =>
    apiFetch<GroupDto>(`/api/groups/${groupId}/remove`, {
      method: "POST",
      body: JSON.stringify({ unit_ids: unitIds }),
    }),
};
