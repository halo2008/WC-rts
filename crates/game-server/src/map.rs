use axum::{
    Router,
    extract::{Path, Query, State},
    routing::get,
    response::Json,
};
use serde::{Deserialize, Serialize};
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
    nation_id: Option<uuid::Uuid>,
}

async fn get_hex(
    State(_state): State<AppState>,
    Path((q, r)): Path<(i32, i32)>,
) -> Result<Json<HexResponse>, StatusCode> {
    // TODO: Query from database once hex_map table is seeded
    // For now return a placeholder
    Ok(Json(HexResponse {
        q,
        r,
        terrain: "Plains".to_string(),
        elevation: 0,
        nation_id: None,
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
    hexes: Vec<HexResponse>,
}

async fn get_region(
    State(_state): State<AppState>,
    Query(query): Query<RegionQuery>,
) -> Result<Json<RegionResponse>, StatusCode> {
    let _radius = query.radius.unwrap_or(5).min(50);
    // TODO: Query from database with wrap-aware range
    Ok(Json(RegionResponse {
        hexes: vec![HexResponse {
            q: query.q,
            r: query.r,
            terrain: "Plains".to_string(),
            elevation: 0,
            nation_id: None,
        }],
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
    // TODO: Load pre-rendered LOD tile from storage
    Ok(Json(TileResponse {
        lod,
        x,
        y,
        data: vec![],
    }))
}

use axum::http::StatusCode;
