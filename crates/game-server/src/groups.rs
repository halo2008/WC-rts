//! Unit group (army) endpoints. A group bundles several units into a single
//! entity that moves together and can be commanded as one. Group members live
//! in `units` (with `group_id` set); the group itself is `unit_groups`.
//!
//! Move path: `POST /api/groups/{id}/move` pathfinds using the slowest
//! member's terrain speed across the whole path — if any member can't cross a
//! tile the whole path is rejected.

use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use game_core::{Hex, Terrain, UnitGroup, UnitType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use uuid::Uuid;

use crate::messages::{GroupDisbandedPayload, GroupUpdatePayload, WsMessage};
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_groups).post(create_group))
        .route("/{id}", get(get_group))
        .route("/{id}/disband", post(disband_group))
        .route("/{id}/move", post(move_group))
        .route("/{id}/cancel", post(cancel_group_transit))
        .route("/{id}/add", post(add_units))
        .route("/{id}/remove", post(remove_units))
}

// ─── Response / request types ──────────────────────────────────────

#[derive(Serialize)]
pub struct GroupResponse {
    pub id: i64,
    pub nation_id: Uuid,
    pub name: String,
    pub hex_q: i32,
    pub hex_r: i32,
    pub member_unit_ids: Vec<i64>,
}

#[derive(Deserialize)]
struct GroupListQuery {
    nation_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct CreateGroupRequest {
    nation_id: Uuid,
    name: String,
    /// All unit IDs to add. They must belong to `nation_id`, sit on the same
    /// hex, and not already be in a different group.
    unit_ids: Vec<i64>,
}

#[derive(Deserialize)]
struct MoveGroupRequest {
    goal_q: i32,
    goal_r: i32,
    #[serde(default = "default_width")]
    width: i32,
}

fn default_width() -> i32 {
    1200
}

#[derive(Deserialize)]
struct ModifyMembersRequest {
    unit_ids: Vec<i64>,
}

// ─── Handlers ──────────────────────────────────────────────────────

async fn list_groups(
    State(state): State<AppState>,
    Query(q): Query<GroupListQuery>,
) -> Result<Json<Vec<GroupResponse>>, StatusCode> {
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT id, nation_id, name, hex_q, hex_r FROM unit_groups WHERE 1=1",
    );
    if let Some(nid) = q.nation_id {
        builder.push(" AND nation_id = ").push_bind(nid);
    }
    builder.push(" ORDER BY id");

    let rows = builder
        .build_query_as::<(i64, Uuid, String, i32, i32)>()
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;

    let ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
    let members = fetch_members(&state, &ids).await?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, nation_id, name, hex_q, hex_r)| GroupResponse {
                id,
                nation_id,
                name,
                hex_q,
                hex_r,
                member_unit_ids: members.get(&id).cloned().unwrap_or_default(),
            })
            .collect(),
    ))
}

async fn get_group(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<GroupResponse>, StatusCode> {
    let row = sqlx::query_as::<_, (i64, Uuid, String, i32, i32)>(
        "SELECT id, nation_id, name, hex_q, hex_r FROM unit_groups WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let members = fetch_members(&state, &[id]).await?;
    Ok(Json(GroupResponse {
        id: row.0,
        nation_id: row.1,
        name: row.2,
        hex_q: row.3,
        hex_r: row.4,
        member_unit_ids: members.get(&id).cloned().unwrap_or_default(),
    }))
}

async fn create_group(
    State(state): State<AppState>,
    Json(req): Json<CreateGroupRequest>,
) -> Result<Json<GroupResponse>, StatusCode> {
    if req.unit_ids.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Validate: all units exist, belong to the nation, sit on the same hex,
    // and aren't already in another group.
    let units = sqlx::query_as::<_, (i64, Uuid, i32, i32, Option<i64>)>(
        "SELECT id, nation_id, hex_q, hex_r, group_id FROM units WHERE id = ANY($1)",
    )
    .bind(&req.unit_ids)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    if units.len() != req.unit_ids.len() {
        return Err(StatusCode::NOT_FOUND);
    }
    let (hq, hr) = (units[0].2, units[0].3);
    for u in &units {
        if u.1 != req.nation_id {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        if u.2 != hq || u.3 != hr {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        if u.4.is_some() {
            return Err(StatusCode::CONFLICT);
        }
    }

    let mut tx = state.db.begin().await.map_err(db_err)?;

    let group_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO unit_groups (nation_id, name, hex_q, hex_r) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(req.nation_id)
    .bind(&req.name)
    .bind(hq)
    .bind(hr)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_err)?;

    sqlx::query("UPDATE units SET group_id = $1 WHERE id = ANY($2)")
        .bind(group_id)
        .bind(&req.unit_ids)
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;

    let response = GroupResponse {
        id: group_id,
        nation_id: req.nation_id,
        name: req.name,
        hex_q: hq,
        hex_r: hr,
        member_unit_ids: req.unit_ids.clone(),
    };

    let _ = state.tx.send(WsMessage::GroupUpdate(GroupUpdatePayload {
        id: response.id,
        nation_id: response.nation_id,
        name: response.name.clone(),
        hex_q: response.hex_q,
        hex_r: response.hex_r,
        member_unit_ids: response.member_unit_ids.clone(),
    }));

    Ok(Json(response))
}

async fn disband_group(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    // Members auto-release via ON DELETE SET NULL. Group transit cascades via
    // ON DELETE CASCADE.
    let affected = sqlx::query("DELETE FROM unit_groups WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    if affected.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    let _ = state
        .tx
        .send(WsMessage::GroupDisbanded(GroupDisbandedPayload { id }));
    Ok(StatusCode::NO_CONTENT)
}

async fn move_group(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<MoveGroupRequest>,
) -> Result<StatusCode, StatusCode> {
    let group = sqlx::query_as::<_, (i64, Uuid, i32, i32)>(
        "SELECT id, nation_id, hex_q, hex_r FROM unit_groups WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Gather member unit types — we pathfind against the *slowest* member.
    let members = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, unit_type FROM units WHERE group_id = $1",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    if members.is_empty() {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }

    let member_types: Vec<UnitType> = members
        .iter()
        .filter_map(|(_, ut)| UnitType::from_str(ut).ok())
        .collect();

    if member_types.len() != members.len() {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    // Fetch terrain across a bounding box to pathfind against.
    let radius = 80;
    let start_q = group.2;
    let start_r = group.3;
    let min_q = start_q.min(req.goal_q) - radius;
    let max_q = start_q.max(req.goal_q) + radius;
    let min_r = (start_r.min(req.goal_r) - radius).max(0);
    let max_r = start_r.max(req.goal_r) + radius;

    let rows = sqlx::query_as::<_, (i32, i32, String)>(
        "SELECT q, r, terrain FROM hex_map WHERE q BETWEEN $1 AND $2 AND r BETWEEN $3 AND $4",
    )
    .bind(min_q)
    .bind(max_q)
    .bind(min_r)
    .bind(max_r)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let mut terrain_map = HashMap::<(i32, i32), Terrain>::new();
    for (q, r, t) in rows {
        let terrain = Terrain::from_str(&t).unwrap_or(Terrain::Plains);
        terrain_map.insert((q, r), terrain);
    }

    let get_terrain = |h: Hex| -> Terrain {
        terrain_map
            .get(&(h.wrap(req.width).q, h.r))
            .copied()
            .unwrap_or(Terrain::Plains)
    };

    // Group is passable iff every member can pass; cost driven by the slowest.
    let member_types_cloned = member_types.clone();
    let is_passable = |h: Hex| UnitGroup::speed_on(&member_types_cloned, get_terrain(h)).is_some();
    let move_cost = |h: Hex| {
        UnitGroup::speed_on(&member_types, get_terrain(h))
            .map(|s| 1.0 / s)
            .unwrap_or(f32::INFINITY)
    };

    let start = Hex::new(start_q, start_r);
    let goal = Hex::new(req.goal_q, req.goal_r);
    let path = game_core::astar(start, goal, is_passable, move_cost, Some(req.width))
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;

    let base_speed = UnitGroup::speed_on(&member_types, get_terrain(start))
        .unwrap_or(0.25);

    let path_json = serde_json::to_value(
        path.iter()
            .map(|h| HexPoint { q: h.q, r: h.r })
            .collect::<Vec<_>>(),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query(
        "INSERT INTO group_transits (group_id, path, progress_hex, base_speed, status, updated_at) \
         VALUES ($1, $2, 0.0, $3, 'Active', now()) \
         ON CONFLICT (group_id) DO UPDATE SET \
            path = EXCLUDED.path, \
            progress_hex = 0.0, \
            base_speed = EXCLUDED.base_speed, \
            status = 'Active', \
            updated_at = now()",
    )
    .bind(id)
    .bind(&path_json)
    .bind(base_speed)
    .execute(&state.db)
    .await
    .map_err(db_err)?;

    Ok(StatusCode::ACCEPTED)
}

async fn cancel_group_transit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let affected = sqlx::query(
        "UPDATE group_transits SET status = 'Cancelled', updated_at = now() \
         WHERE group_id = $1 AND status IN ('Active', 'Blocked')",
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(db_err)?;

    if affected.rows_affected() == 0 {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

async fn add_units(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ModifyMembersRequest>,
) -> Result<Json<GroupResponse>, StatusCode> {
    if req.unit_ids.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let group = sqlx::query_as::<_, (i64, Uuid, String, i32, i32)>(
        "SELECT id, nation_id, name, hex_q, hex_r FROM unit_groups WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Candidate units must be same nation, same hex, no existing group.
    let units = sqlx::query_as::<_, (i64, Uuid, i32, i32, Option<i64>)>(
        "SELECT id, nation_id, hex_q, hex_r, group_id FROM units WHERE id = ANY($1)",
    )
    .bind(&req.unit_ids)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    if units.len() != req.unit_ids.len() {
        return Err(StatusCode::NOT_FOUND);
    }
    for u in &units {
        if u.1 != group.1 || u.2 != group.3 || u.3 != group.4 || u.4.is_some() {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    sqlx::query("UPDATE units SET group_id = $1 WHERE id = ANY($2)")
        .bind(id)
        .bind(&req.unit_ids)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    broadcast_and_return(&state, group).await
}

async fn remove_units(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<ModifyMembersRequest>,
) -> Result<Json<GroupResponse>, StatusCode> {
    sqlx::query("UPDATE units SET group_id = NULL WHERE group_id = $1 AND id = ANY($2)")
        .bind(id)
        .bind(&req.unit_ids)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    let group = sqlx::query_as::<_, (i64, Uuid, String, i32, i32)>(
        "SELECT id, nation_id, name, hex_q, hex_r FROM unit_groups WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or(StatusCode::NOT_FOUND)?;

    broadcast_and_return(&state, group).await
}

// ─── Helpers ───────────────────────────────────────────────────────

async fn broadcast_and_return(
    state: &AppState,
    group: (i64, Uuid, String, i32, i32),
) -> Result<Json<GroupResponse>, StatusCode> {
    let members = fetch_members(state, &[group.0]).await?;
    let member_unit_ids = members.get(&group.0).cloned().unwrap_or_default();

    let _ = state.tx.send(WsMessage::GroupUpdate(GroupUpdatePayload {
        id: group.0,
        nation_id: group.1,
        name: group.2.clone(),
        hex_q: group.3,
        hex_r: group.4,
        member_unit_ids: member_unit_ids.clone(),
    }));

    Ok(Json(GroupResponse {
        id: group.0,
        nation_id: group.1,
        name: group.2,
        hex_q: group.3,
        hex_r: group.4,
        member_unit_ids,
    }))
}

async fn fetch_members(
    state: &AppState,
    group_ids: &[i64],
) -> Result<HashMap<i64, Vec<i64>>, StatusCode> {
    if group_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query_as::<_, (i64, i64)>(
        "SELECT group_id, id FROM units WHERE group_id = ANY($1) ORDER BY id",
    )
    .bind(group_ids)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let mut map: HashMap<i64, Vec<i64>> = HashMap::new();
    for (gid, uid) in rows {
        map.entry(gid).or_default().push(uid);
    }
    Ok(map)
}

fn db_err(e: sqlx::Error) -> StatusCode {
    tracing::error!("DB error in groups: {}", e);
    StatusCode::INTERNAL_SERVER_ERROR
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct HexPoint {
    q: i32,
    r: i32,
}
