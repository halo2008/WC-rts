use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, patch},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_nations))
        .route("/{id}", get(get_nation))
        .route("/{id}/state", get(get_nation_state))
        .route("/{id}/budget", patch(update_budget))
}

#[derive(Serialize)]
struct NationSummary {
    id: Uuid,
    code: String,
    name: String,
    government_type: String,
    tier: i16,
    color: String,
    /// Capital hex. Null if the nation doesn't have a `nation_state` row yet —
    /// shouldn't happen in practice, but we allow it for robustness.
    capital_q: Option<i32>,
    capital_r: Option<i32>,
}

async fn list_nations(
    State(state): State<AppState>,
) -> Result<Json<Vec<NationSummary>>, StatusCode> {
    let rows = sqlx::query_as::<_, (Uuid, String, String, String, i16, String, Option<i32>, Option<i32>)>(
        "SELECT n.id, n.code, n.name, n.government_type, n.tier, n.color, \
                ns.capital_q, ns.capital_r \
         FROM nations n \
         LEFT JOIN nation_state ns ON ns.nation_id = n.id \
         ORDER BY n.tier, n.name",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error listing nations: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, code, name, government_type, tier, color, capital_q, capital_r)| NationSummary {
                id,
                code,
                name,
                government_type,
                tier,
                color,
                capital_q,
                capital_r,
            })
            .collect(),
    ))
}

async fn get_nation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<NationSummary>, StatusCode> {
    let row = sqlx::query_as::<_, (Uuid, String, String, String, i16, String, Option<i32>, Option<i32>)>(
        "SELECT n.id, n.code, n.name, n.government_type, n.tier, n.color, \
                ns.capital_q, ns.capital_r \
         FROM nations n \
         LEFT JOIN nation_state ns ON ns.nation_id = n.id \
         WHERE n.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error fetching nation: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let (id, code, name, government_type, tier, color, capital_q, capital_r) = row;
    Ok(Json(NationSummary {
        id,
        code,
        name,
        government_type,
        tier,
        color,
        capital_q,
        capital_r,
    }))
}

#[derive(Serialize)]
struct NationStateResponse {
    nation_id: Uuid,
    stockpile: StockpileJson,
    budget: BudgetJson,
    capital_q: i32,
    capital_r: i32,
    war_support: f32,
    stability: f32,
}

#[derive(Serialize)]
struct StockpileJson {
    fuel: f32,
    metals: f32,
    tech: f32,
    food: f32,
}

#[derive(Serialize, Deserialize)]
struct BudgetJson {
    military: f32,
    economy: f32,
    research: f32,
    social: f32,
    intel: f32,
}

async fn get_nation_state(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<NationStateResponse>, StatusCode> {
    let row = sqlx::query_as::<
        _,
        (Uuid, f32, f32, f32, f32, f32, f32, f32, f32, f32, i32, i32, f32, f32),
    >(
        "SELECT nation_id, fuel, metals, tech, food, \
                budget_military, budget_economy, budget_research, budget_social, budget_intel, \
                capital_q, capital_r, war_support, stability \
         FROM nation_state WHERE nation_id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error fetching nation_state: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let (nid, fuel, metals, tech, food, bm, be, br, bs, bi, cq, cr, ws, stab) = row;
    Ok(Json(NationStateResponse {
        nation_id: nid,
        stockpile: StockpileJson { fuel, metals, tech, food },
        budget: BudgetJson {
            military: bm,
            economy: be,
            research: br,
            social: bs,
            intel: bi,
        },
        capital_q: cq,
        capital_r: cr,
        war_support: ws,
        stability: stab,
    }))
}

async fn update_budget(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<BudgetJson>,
) -> Result<Json<BudgetJson>, StatusCode> {
    // Normalise so sliders always sum to 1.0; we trust game-core's rule here.
    let alloc = game_core::BudgetAllocation {
        military: req.military,
        economy: req.economy,
        research: req.research,
        social: req.social,
        intel: req.intel,
    }
    .normalized();

    let affected = sqlx::query(
        "UPDATE nation_state SET \
            budget_military = $2, \
            budget_economy  = $3, \
            budget_research = $4, \
            budget_social   = $5, \
            budget_intel    = $6, \
            updated_at      = now() \
         WHERE nation_id = $1",
    )
    .bind(id)
    .bind(alloc.military)
    .bind(alloc.economy)
    .bind(alloc.research)
    .bind(alloc.social)
    .bind(alloc.intel)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error updating budget: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if affected.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(Json(BudgetJson {
        military: alloc.military,
        economy: alloc.economy,
        research: alloc.research,
        social: alloc.social,
        intel: alloc.intel,
    }))
}
