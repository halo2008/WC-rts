use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    response::Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/hex/{q}/{r}", get(get_hex))
        .route("/region", get(get_region))
        .route("/tiles/{lod}/{x}/{y}", get(get_tile))
}

// ─── Handlers ────────────────────────────────────────────────────────

#[derive(Serialize)]
struct HexResponse {
    q: i32,
    r: i32,
    terrain: String,
    elevation: i32,
    nation_id: Option<Uuid>,
    resource: Option<String>,
    infrastructure_level: i16,
}

async fn get_hex(
    State(state): State<AppState>,
    Path((q, r)): Path<(i32, i32)>,
) -> Result<Json<HexResponse>, StatusCode> {
    let row = sqlx::query_as::<_, (i32, i32, String, i32, Option<Uuid>, Option<String>, i16)>(
        "SELECT q, r, terrain, elevation, nation_id, resource, infrastructure_level \
         FROM hex_map WHERE q = $1 AND r = $2",
    )
    .bind(q)
    .bind(r)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error fetching hex: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let (q, r, terrain, elevation, nation_id, resource, infrastructure_level) = row;
    Ok(Json(HexResponse {
        q,
        r,
        terrain,
        elevation,
        nation_id,
        resource,
        infrastructure_level,
    }))
}

#[derive(Deserialize)]
struct RegionQuery {
    q: i32,
    r: i32,
    radius: Option<i32>,
}

#[derive(Serialize)]
struct RegionResponse {
    center_q: i32,
    center_r: i32,
    radius: i32,
    hexes: Vec<HexResponse>,
}

/// Query a hex neighborhood. `radius` is clamped to 50 to bound query size.
/// Uses cube-distance filtering in SQL so oceans on the far side of the wrap
/// aren't pulled in by a naive bounding-box search.
async fn get_region(
    State(state): State<AppState>,
    Query(query): Query<RegionQuery>,
) -> Result<Json<RegionResponse>, StatusCode> {
    let radius = query.radius.unwrap_or(5).clamp(0, 50);

    // Cube-distance filter: max(|dq|, |dr|, |-dq-dr|) <= radius.
    let rows = sqlx::query_as::<_, (i32, i32, String, i32, Option<Uuid>, Option<String>, i16)>(
        "SELECT q, r, terrain, elevation, nation_id, resource, infrastructure_level \
         FROM hex_map \
         WHERE GREATEST(\
                 ABS(q - $1), \
                 ABS(r - $2), \
                 ABS((-1) * (q - $1) - (r - $2))\
             ) <= $3",
    )
    .bind(query.q)
    .bind(query.r)
    .bind(radius)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error fetching region: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let hexes = rows
        .into_iter()
        .map(|(q, r, terrain, elevation, nation_id, resource, infrastructure_level)| HexResponse {
            q,
            r,
            terrain,
            elevation,
            nation_id,
            resource,
            infrastructure_level,
        })
        .collect();

    Ok(Json(RegionResponse {
        center_q: query.q,
        center_r: query.r,
        radius,
        hexes,
    }))
}

#[derive(Serialize)]
struct TileResponse {
    lod: u32,
    x: u32,
    y: u32,
    // Binary tile data would go here in production
    data: Vec<u8>,
}

async fn get_tile(
    State(_state): State<AppState>,
    Path((lod, x, y)): Path<(u32, u32, u32)>,
) -> Result<Json<TileResponse>, StatusCode> {
    // LOD tile pipeline not implemented yet — planned for Etap 1.7.B.
    Ok(Json(TileResponse {
        lod,
        x,
        y,
        data: vec![],
    }))
}
