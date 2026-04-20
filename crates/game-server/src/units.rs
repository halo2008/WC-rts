use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use game_core::{Hex, Terrain, Transit, TransitStatus, UnitType};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;
use crate::messages::{UnitSpawnPayload, WsMessage};
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_units).post(create_unit))
        .route("/{id}", get(get_unit))
        .route("/{id}/move", post(move_unit))
        .route("/{id}/transit", get(get_transit))
        .route("/{id}/cancel", post(cancel_transit))
        .route("/production", get(list_production).post(enqueue_production))
}

// ─── Response / request types ──────────────────────────────────────

#[derive(Serialize)]
struct UnitResponse {
    id: i64,
    unit_type: String,
    nation_id: Uuid,
    hex_q: i32,
    hex_r: i32,
    hp: i32,
    max_hp: i32,
    morale: f32,
    experience: f32,
}

#[derive(Deserialize)]
struct UnitListQuery {
    nation_id: Option<Uuid>,
    /// Optional bounding box in axial coords (inclusive).
    min_q: Option<i32>,
    max_q: Option<i32>,
    min_r: Option<i32>,
    max_r: Option<i32>,
}

#[derive(Deserialize)]
struct CreateUnitRequest {
    unit_type: String,
    nation_id: Uuid,
    hex_q: i32,
    hex_r: i32,
}

#[derive(Deserialize)]
struct MoveRequest {
    goal_q: i32,
    goal_r: i32,
    /// Grid width for wrap-around pathfinding.
    #[serde(default = "default_width")]
    width: i32,
}

fn default_width() -> i32 {
    1200
}

#[derive(Serialize)]
struct TransitResponse {
    unit_id: i64,
    status: String,
    progress_hex: f32,
    path: Vec<HexPoint>,
    current_hex: HexPoint,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct HexPoint {
    q: i32,
    r: i32,
}

// ─── Handlers ──────────────────────────────────────────────────────

async fn list_units(
    State(state): State<AppState>,
    Query(q): Query<UnitListQuery>,
) -> Result<Json<Vec<UnitResponse>>, StatusCode> {
    // Build the query dynamically — kept simple with `QueryBuilder`.
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT id, unit_type, nation_id, hex_q, hex_r, hp, max_hp, morale, experience \
         FROM units WHERE 1=1",
    );

    if let Some(nid) = q.nation_id {
        builder.push(" AND nation_id = ").push_bind(nid);
    }
    if let Some(v) = q.min_q {
        builder.push(" AND hex_q >= ").push_bind(v);
    }
    if let Some(v) = q.max_q {
        builder.push(" AND hex_q <= ").push_bind(v);
    }
    if let Some(v) = q.min_r {
        builder.push(" AND hex_r >= ").push_bind(v);
    }
    if let Some(v) = q.max_r {
        builder.push(" AND hex_r <= ").push_bind(v);
    }

    builder.push(" ORDER BY id LIMIT 5000");

    let rows = builder
        .build_query_as::<(i64, String, Uuid, i32, i32, i32, i32, f32, f32)>()
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("DB error listing units: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, ut, nid, hq, hr, hp, max_hp, mo, xp)| UnitResponse {
                id,
                unit_type: ut,
                nation_id: nid,
                hex_q: hq,
                hex_r: hr,
                hp,
                max_hp,
                morale: mo,
                experience: xp,
            })
            .collect(),
    ))
}

async fn get_unit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<UnitResponse>, StatusCode> {
    let row = fetch_unit_row(&state, id).await?;
    Ok(Json(row))
}

async fn fetch_unit_row(state: &AppState, id: i64) -> Result<UnitResponse, StatusCode> {
    let row = sqlx::query_as::<_, (i64, String, Uuid, i32, i32, i32, i32, f32, f32)>(
        "SELECT id, unit_type, nation_id, hex_q, hex_r, hp, max_hp, morale, experience \
         FROM units WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error fetching unit: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let (id, ut, nid, hq, hr, hp, max_hp, mo, xp) = row;
    Ok(UnitResponse {
        id,
        unit_type: ut,
        nation_id: nid,
        hex_q: hq,
        hex_r: hr,
        hp,
        max_hp,
        morale: mo,
        experience: xp,
    })
}

async fn create_unit(
    State(state): State<AppState>,
    Json(req): Json<CreateUnitRequest>,
) -> Result<Json<UnitResponse>, StatusCode> {
    let unit_type = UnitType::from_str(&req.unit_type).map_err(|_| StatusCode::BAD_REQUEST)?;
    let max_hp = unit_type.stats().max_hp;

    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO units (unit_type, nation_id, hex_q, hex_r, hp, max_hp) \
         VALUES ($1, $2, $3, $4, $5, $5) RETURNING id",
    )
    .bind(unit_type.as_str())
    .bind(req.nation_id)
    .bind(req.hex_q)
    .bind(req.hex_r)
    .bind(max_hp)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error creating unit: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Broadcast so other connected clients see the new unit without polling.
    let _ = state.tx.send(WsMessage::UnitSpawn(UnitSpawnPayload {
        id,
        unit_type: unit_type.as_str().to_string(),
        nation_id: req.nation_id,
        hex_q: req.hex_q,
        hex_r: req.hex_r,
        hp: max_hp,
        max_hp,
    }));

    fetch_unit_row(&state, id).await.map(Json)
}

async fn move_unit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(req): Json<MoveRequest>,
) -> Result<Json<TransitResponse>, StatusCode> {
    let unit = fetch_unit_row(&state, id).await?;
    let unit_type = UnitType::from_str(&unit.unit_type).map_err(|_| {
        tracing::error!("unit {} has invalid unit_type {}", id, unit.unit_type);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Pathfind against the live `hex_map` table — terrain comes from DB so the
    // server's view stays consistent with what's seeded in.
    let path = pathfind_on_map(&state, &unit, unit_type, req.goal_q, req.goal_r, req.width)
        .await?
        .ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;

    let transit = Transit::new(id as u64, unit_type, path.clone());
    let path_json = serde_json::to_value(
        path.iter()
            .map(|h| HexPoint { q: h.q, r: h.r })
            .collect::<Vec<_>>(),
    )
    .map_err(|e| {
        tracing::error!("serialize path: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Upsert — reissuing a move replaces any existing transit for this unit.
    sqlx::query(
        "INSERT INTO unit_transits (unit_id, path, progress_hex, base_speed, status, updated_at) \
         VALUES ($1, $2, 0.0, $3, 'Active', now()) \
         ON CONFLICT (unit_id) DO UPDATE SET \
            path = EXCLUDED.path, \
            progress_hex = 0.0, \
            base_speed = EXCLUDED.base_speed, \
            status = 'Active', \
            updated_at = now()",
    )
    .bind(id)
    .bind(&path_json)
    .bind(transit.base_speed)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error writing transit: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let current = transit.current_hex();
    Ok(Json(TransitResponse {
        unit_id: id,
        status: format_status(transit.status),
        progress_hex: transit.progress_hex,
        path: path.iter().map(|h| HexPoint { q: h.q, r: h.r }).collect(),
        current_hex: HexPoint { q: current.q, r: current.r },
    }))
}

async fn get_transit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<TransitResponse>, StatusCode> {
    let row = sqlx::query_as::<_, (i64, serde_json::Value, f32, f32, String)>(
        "SELECT unit_id, path, progress_hex, base_speed, status \
         FROM unit_transits WHERE unit_id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error fetching transit: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let (unit_id, path_json, progress_hex, _base_speed, status) = row;
    let path: Vec<HexPoint> = serde_json::from_value(path_json).map_err(|e| {
        tracing::error!("path json invalid: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let current_hex = interpolated_current(&path, progress_hex);

    Ok(Json(TransitResponse {
        unit_id,
        status,
        progress_hex,
        path,
        current_hex,
    }))
}

async fn cancel_transit(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    let affected = sqlx::query(
        "UPDATE unit_transits SET status = 'Cancelled', updated_at = now() \
         WHERE unit_id = $1 AND status IN ('Active', 'Blocked')",
    )
    .bind(id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error cancelling transit: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if affected.rows_affected() == 0 {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

// ─── Helpers ───────────────────────────────────────────────────────

fn format_status(s: TransitStatus) -> String {
    match s {
        TransitStatus::Active => "Active".into(),
        TransitStatus::Completed => "Completed".into(),
        TransitStatus::Cancelled => "Cancelled".into(),
        TransitStatus::Blocked => "Blocked".into(),
    }
}

fn interpolated_current(path: &[HexPoint], progress_hex: f32) -> HexPoint {
    if path.is_empty() {
        return HexPoint { q: 0, r: 0 };
    }
    let last = path.len() - 1;
    let idx = progress_hex.floor() as usize;
    if idx >= last {
        return path[last];
    }
    let frac = progress_hex - idx as f32;
    if frac >= 0.5 {
        path[idx + 1]
    } else {
        path[idx]
    }
}

// ─── Production queue ──────────────────────────────────────────────

#[derive(Deserialize)]
struct EnqueueProductionRequest {
    nation_id: Uuid,
    unit_type: String,
    spawn_hex_q: i32,
    spawn_hex_r: i32,
}

#[derive(Serialize)]
struct ProductionOrderResponse {
    id: i64,
    nation_id: Uuid,
    unit_type: String,
    spawn_hex_q: i32,
    spawn_hex_r: i32,
    progress: f32,
    total_time: f32,
    status: String,
}

#[derive(Deserialize)]
struct ProductionListQuery {
    nation_id: Option<Uuid>,
    status: Option<String>,
}

async fn enqueue_production(
    State(state): State<AppState>,
    Json(req): Json<EnqueueProductionRequest>,
) -> Result<Json<ProductionOrderResponse>, StatusCode> {
    let unit_type = UnitType::from_str(&req.unit_type).map_err(|_| StatusCode::BAD_REQUEST)?;
    let stats = unit_type.stats();

    // Charge the build cost upfront (classic RTS semantics). Reject if the
    // nation can't afford it; the strategic tick then advances progress for
    // free until completion.
    let cost_metals = stats.build_cost_metals;
    let cost_fuel = stats.build_cost_fuel;

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("DB tx begin: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let row = sqlx::query_as::<_, (f32, f32)>(
        "SELECT metals, fuel FROM nation_state WHERE nation_id = $1 FOR UPDATE",
    )
    .bind(req.nation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("DB tx fetch nation_state: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    if row.0 < cost_metals || row.1 < cost_fuel {
        return Err(StatusCode::PAYMENT_REQUIRED);
    }

    sqlx::query(
        "UPDATE nation_state SET metals = metals - $2, fuel = fuel - $3, updated_at = now() \
         WHERE nation_id = $1",
    )
    .bind(req.nation_id)
    .bind(cost_metals)
    .bind(cost_fuel)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("DB tx charge cost: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO production_queue \
            (nation_id, unit_type, spawn_hex_q, spawn_hex_r, progress, total_time, status) \
         VALUES ($1, $2, $3, $4, 0.0, $5, 'InProgress') RETURNING id",
    )
    .bind(req.nation_id)
    .bind(unit_type.as_str())
    .bind(req.spawn_hex_q)
    .bind(req.spawn_hex_r)
    .bind(stats.build_time_seconds)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("DB tx insert order: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tx.commit().await.map_err(|e| {
        tracing::error!("DB tx commit: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(ProductionOrderResponse {
        id,
        nation_id: req.nation_id,
        unit_type: unit_type.as_str().to_string(),
        spawn_hex_q: req.spawn_hex_q,
        spawn_hex_r: req.spawn_hex_r,
        progress: 0.0,
        total_time: stats.build_time_seconds,
        status: "InProgress".into(),
    }))
}

async fn list_production(
    State(state): State<AppState>,
    Query(q): Query<ProductionListQuery>,
) -> Result<Json<Vec<ProductionOrderResponse>>, StatusCode> {
    let mut builder = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT id, nation_id, unit_type, spawn_hex_q, spawn_hex_r, progress, total_time, status \
         FROM production_queue WHERE 1=1",
    );
    if let Some(nid) = q.nation_id {
        builder.push(" AND nation_id = ").push_bind(nid);
    }
    if let Some(s) = q.status {
        builder.push(" AND status = ").push_bind(s);
    }
    builder.push(" ORDER BY id DESC LIMIT 500");

    let rows = builder
        .build_query_as::<(i64, Uuid, String, i32, i32, f32, f32, String)>()
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("DB list production: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, nation_id, unit_type, sq, sr, prog, tot, status)| ProductionOrderResponse {
                id,
                nation_id,
                unit_type,
                spawn_hex_q: sq,
                spawn_hex_r: sr,
                progress: prog,
                total_time: tot,
                status,
            })
            .collect(),
    ))
}

/// Pathfind from `unit` to `(goal_q, goal_r)` using the live `hex_map` as
/// terrain source. Loads the relevant hexes into memory first — cheap enough
/// at 720k hexes total, expensive enough to deserve caching later.
async fn pathfind_on_map(
    state: &AppState,
    unit: &UnitResponse,
    unit_type: UnitType,
    goal_q: i32,
    goal_r: i32,
    width: i32,
) -> Result<Option<Vec<Hex>>, StatusCode> {
    // For now we load only the hexes in a bounding box around start+goal. If a
    // real path needs to wind around obstacles we may miss options — good
    // enough for initial movement, refined later.
    let radius = 80;
    let min_q = unit.hex_q.min(goal_q) - radius;
    let max_q = unit.hex_q.max(goal_q) + radius;
    let min_r = (unit.hex_r.min(goal_r) - radius).max(0);
    let max_r = unit.hex_r.max(goal_r) + radius;

    let rows = sqlx::query_as::<_, (i32, i32, String)>(
        "SELECT q, r, terrain FROM hex_map \
         WHERE q BETWEEN $1 AND $2 AND r BETWEEN $3 AND $4",
    )
    .bind(min_q)
    .bind(max_q)
    .bind(min_r)
    .bind(max_r)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error loading hex_map for pathfind: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Build a lookup. Missing hexes default to Plains — we never seeded them.
    let mut terrain_map = std::collections::HashMap::<(i32, i32), Terrain>::new();
    for (q, r, t) in rows {
        let terrain = Terrain::from_str(&t).unwrap_or(Terrain::Plains);
        terrain_map.insert((q, r), terrain);
    }

    let get_terrain = |h: Hex| -> Terrain {
        terrain_map
            .get(&(h.wrap(width).q, h.r))
            .copied()
            .unwrap_or(Terrain::Plains)
    };

    let is_passable = |h: Hex| unit_type.speed_on(get_terrain(h)).is_some();
    let move_cost = |h: Hex| {
        unit_type
            .speed_on(get_terrain(h))
            .map(|s| 1.0 / s)
            .unwrap_or(f32::INFINITY)
    };

    let start = Hex::new(unit.hex_q, unit.hex_r);
    let goal = Hex::new(goal_q, goal_r);
    Ok(game_core::astar(start, goal, is_passable, move_cost, Some(width)))
}
