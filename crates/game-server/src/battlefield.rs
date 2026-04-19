use axum::{
    Router,
    extract::{Path, State, Query},
    routing::{get, post},
    response::Json,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/{q}/{r}", get(get_battlefield))
        .route("/{q}/{r}/build", post(build_building))
        .route("/{q}/{r}/buildings", get(get_buildings))
        .route("/{q}/{r}/deposits", get(get_deposits))
        .route("/{q}/{r}/viewport", get(get_battlefield_viewport))
}

// ─── Response types ─────────────────────────────────────────────────

#[derive(Serialize)]
struct BattlefieldResponse {
    id: Uuid,
    strategic_q: i32,
    strategic_r: i32,
    size: i16,
    seed: i64,
    status: String,
    macro_terrain: String,
    macro_elevation: i32,
    buildings_count: i64,
}

#[derive(Serialize)]
struct BattlefieldCellResponse {
    q: i32,
    r: i32,
    terrain: String,
    elevation: i32,
    forest_density: f32,
    has_road: bool,
    has_river: bool,
    building_id: Option<i64>,
    deposit: Option<String>,
}

#[derive(Serialize)]
struct BuildingResponse {
    id: i64,
    building_type: String,
    hex_q: i32,
    hex_r: i32,
    level: i16,
    hp: i32,
    max_hp: i32,
    nation_id: Option<Uuid>,
    is_active: bool,
}

#[derive(Serialize)]
struct DepositResponse {
    id: i64,
    deposit_type: String,
    hex_q: i32,
    hex_r: i32,
    richness: f32,
    remaining: Option<f32>,
}

#[derive(Serialize)]
struct ConstructionOrderResponse {
    id: i64,
    building_type: String,
    hex_q: i32,
    hex_r: i32,
    status: String,
    progress: f32,
    total_time: f32,
    elapsed: f32,
}

#[derive(Deserialize)]
struct ViewportQuery {
    cam_x: f32,
    cam_y: f32,
    view_width: f32,
    view_height: f32,
    hex_size: Option<f32>,
}

// ─── Handlers ───────────────────────────────────────────────────────

/// GET /api/battlefield/:q/:r — get or lazy-create a battlefield instance.
async fn get_battlefield(
    State(state): State<AppState>,
    Path((q, r)): Path<(i32, i32)>,
) -> Result<Json<BattlefieldResponse>, StatusCode> {
    // Try to get existing battlefield
    let existing = sqlx::query_as::<_, (Uuid, i32, i32, i16, i64, String, String, i32)>(
        "SELECT id, strategic_q, strategic_r, size, seed, status, macro_terrain, macro_elevation \
         FROM battlefield_instances WHERE strategic_q = $1 AND strategic_r = $2"
    )
    .bind(q)
    .bind(r)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error fetching battlefield: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some((id, sq, sr, size, seed, status, macro_terrain, macro_elevation)) = existing {
        let buildings_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM buildings WHERE battlefield_id = $1"
        )
        .bind(id)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        return Ok(Json(BattlefieldResponse {
            id,
            strategic_q: sq,
            strategic_r: sr,
            size,
            seed,
            status,
            macro_terrain,
            macro_elevation,
            buildings_count,
        }));
    }

    // Lazy-create: generate a new battlefield instance
    // Use strategic hex coords as seed for deterministic generation
    let seed = ((q as i64) * 10000 + r as i64).abs() as u64;
    let size: i16 = 64;

    // Get macro terrain from hex_map if available, otherwise default
    let (macro_terrain, macro_elevation) = sqlx::query_as::<_, (String, i32)>(
        "SELECT terrain, COALESCE(elevation, 0) FROM hex_map WHERE q = $1 AND r = $2"
    )
    .bind(q)
    .bind(r)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error fetching hex_map: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .unwrap_or_else(|| ("Plains".to_string(), 0));

    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO battlefield_instances (id, strategic_q, strategic_r, size, seed, status, macro_terrain, macro_elevation) \
         VALUES ($1, $2, $3, $4, $5, 'CALM', $6, $7)"
    )
    .bind(id)
    .bind(q)
    .bind(r)
    .bind(size)
    .bind(seed as i64)
    .bind(&macro_terrain)
    .bind(macro_elevation)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error creating battlefield: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Generate and store resource deposits
    let deposits = generate_deposits_for_terrain(&macro_terrain, seed);
    for (dq, dr, dtype, richness) in deposits {
        sqlx::query(
            "INSERT INTO resource_deposits (battlefield_id, deposit_type, hex_q, hex_r, richness) \
             VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(id)
        .bind(&dtype)
        .bind(dq)
        .bind(dr)
        .bind(richness)
        .execute(&state.db)
        .await
        .ok(); // Don't fail on deposit insert errors
    }

    tracing::info!("Created battlefield instance for hex ({}, {})", q, r);

    Ok(Json(BattlefieldResponse {
        id,
        strategic_q: q,
        strategic_r: r,
        size,
        seed: seed as i64,
        status: "CALM".to_string(),
        macro_terrain,
        macro_elevation,
        buildings_count: 0,
    }))
}

/// POST /api/battlefield/:q/:r/build — place a building on the battlefield.
#[derive(Deserialize)]
struct BuildRequest {
    building_type: String,
    hex_q: i32,
    hex_r: i32,
    nation_id: Option<Uuid>,
}

async fn build_building(
    State(state): State<AppState>,
    Path((q, r)): Path<(i32, i32)>,
    Json(req): Json<BuildRequest>,
) -> Result<Json<BuildingResponse>, StatusCode> {
    // Get battlefield ID
    let bf_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM battlefield_instances WHERE strategic_q = $1 AND strategic_r = $2"
    )
    .bind(q)
    .bind(r)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Check if hex already has a building
    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM buildings WHERE battlefield_id = $1 AND hex_q = $2 AND hex_r = $3"
    )
    .bind(bf_id)
    .bind(req.hex_q)
    .bind(req.hex_r)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if existing > 0 {
        return Err(StatusCode::CONFLICT);
    }

    // Get building stats from game-core
    let building_type = parse_building_type(&req.building_type)?;
    let max_hp = building_type.base_hp();
    let total_time = building_type.build_time();

    // Insert building
    let building_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO buildings (battlefield_id, building_type, hex_q, hex_r, level, hp, max_hp, nation_id) \
         VALUES ($1, $2, $3, $4, 1, $5, $6, $7) \
         RETURNING id"
    )
    .bind(bf_id)
    .bind(&req.building_type)
    .bind(req.hex_q)
    .bind(req.hex_r)
    .bind(max_hp)
    .bind(max_hp)
    .bind(req.nation_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error inserting building: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    // Also add to construction queue
    sqlx::query(
        "INSERT INTO construction_orders (battlefield_id, building_type, hex_q, hex_r, nation_id, status, progress, total_time, elapsed) \
         VALUES ($1, $2, $3, $4, $5, 'IN_PROGRESS', 0.0, $6, 0.0)"
    )
    .bind(bf_id)
    .bind(&req.building_type)
    .bind(req.hex_q)
    .bind(req.hex_r)
    .bind(req.nation_id)
    .bind(total_time)
    .execute(&state.db)
    .await
    .ok();

    Ok(Json(BuildingResponse {
        id: building_id,
        building_type: req.building_type,
        hex_q: req.hex_q,
        hex_r: req.hex_r,
        level: 1,
        hp: max_hp,
        max_hp,
        nation_id: req.nation_id,
        is_active: true,
    }))
}

/// GET /api/battlefield/:q/:r/buildings — list all buildings on a battlefield.
async fn get_buildings(
    State(state): State<AppState>,
    Path((q, r)): Path<(i32, i32)>,
) -> Result<Json<Vec<BuildingResponse>>, StatusCode> {
    let rows = sqlx::query_as::<_, (i64, String, i32, i32, i16, i32, i32, Option<Uuid>, bool)>(
        "SELECT b.id, b.building_type, b.hex_q, b.hex_r, b.level, b.hp, b.max_hp, b.nation_id, b.is_active \
         FROM buildings b \
         JOIN battlefield_instances bf ON b.battlefield_id = bf.id \
         WHERE bf.strategic_q = $1 AND bf.strategic_r = $2"
    )
    .bind(q)
    .bind(r)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let buildings = rows.into_iter().map(|(id, bt, hq, hr, lvl, hp, mhp, nid, active)| {
        BuildingResponse {
            id,
            building_type: bt,
            hex_q: hq,
            hex_r: hr,
            level: lvl,
            hp,
            max_hp: mhp,
            nation_id: nid,
            is_active: active,
        }
    }).collect();

    Ok(Json(buildings))
}

/// GET /api/battlefield/:q/:r/deposits — list resource deposits.
async fn get_deposits(
    State(state): State<AppState>,
    Path((q, r)): Path<(i32, i32)>,
) -> Result<Json<Vec<DepositResponse>>, StatusCode> {
    let rows = sqlx::query_as::<_, (i64, String, i32, i32, f32, Option<f32>)>(
        "SELECT rd.id, rd.deposit_type, rd.hex_q, rd.hex_r, rd.richness, rd.remaining \
         FROM resource_deposits rd \
         JOIN battlefield_instances bf ON rd.battlefield_id = bf.id \
         WHERE bf.strategic_q = $1 AND bf.strategic_r = $2"
    )
    .bind(q)
    .bind(r)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let deposits = rows.into_iter().map(|(id, dt, hq, hr, rich, rem)| {
        DepositResponse {
            id,
            deposit_type: dt,
            hex_q: hq,
            hex_r: hr,
            richness: rich,
            remaining: rem,
        }
    }).collect();

    Ok(Json(deposits))
}

/// GET /api/battlefield/:q/:r/viewport — get battlefield cells for a viewport.
/// This generates cells on-the-fly from the deterministic generator.
async fn get_battlefield_viewport(
    State(state): State<AppState>,
    Path((q, r)): Path<(i32, i32)>,
    Query(query): Query<ViewportQuery>,
) -> Result<Json<Vec<BattlefieldCellResponse>>, StatusCode> {
    // Get battlefield metadata
    let bf = sqlx::query_as::<_, (Uuid, i16, i64, String, i32)>(
        "SELECT id, size, seed, macro_terrain, macro_elevation \
         FROM battlefield_instances WHERE strategic_q = $1 AND strategic_r = $2"
    )
    .bind(q)
    .bind(r)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .ok_or(StatusCode::NOT_FOUND)?;

    let (_bf_id, size, seed, macro_terrain, macro_elevation) = bf;

    // Generate battlefield using game-core
    let terrain = parse_terrain(&macro_terrain);
    let config = game_core::BattlefieldConfig {
        width: size as i32,
        height: size as i32,
        seed: seed as u64,
        strategic_q: q,
        strategic_r: r,
        macro_terrain: terrain,
        macro_elevation,
    };

    let instance = game_core::BattlefieldGenerator::generate(config);
    let hex_size = query.hex_size.unwrap_or(8.0);
    let cells = instance.viewport(query.cam_x, query.cam_y, query.view_width, query.view_height, hex_size);

    // Get buildings for this battlefield to mark cells
    let buildings: std::collections::HashMap<(i32, i32), i64> = sqlx::query_as::<_, (i64, i32, i32)>(
        "SELECT b.id, b.hex_q, b.hex_r \
         FROM buildings b \
         JOIN battlefield_instances bf ON b.battlefield_id = bf.id \
         WHERE bf.strategic_q = $1 AND bf.strategic_r = $2"
    )
    .bind(q)
    .bind(r)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("DB error: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?
    .into_iter()
    .map(|(id, hq, hr)| ((hq, hr), id))
    .collect();

    let response: Vec<BattlefieldCellResponse> = cells.into_iter().map(|cell| {
        BattlefieldCellResponse {
            q: cell.hex.q,
            r: cell.hex.r,
            terrain: format!("{:?}", cell.terrain),
            elevation: cell.elevation,
            forest_density: cell.forest_density,
            has_road: cell.has_road,
            has_river: cell.has_river,
            building_id: buildings.get(&(cell.hex.q, cell.hex.r)).copied(),
            deposit: cell.deposit.map(|d| format!("{:?}", d)),
        }
    }).collect();

    Ok(Json(response))
}

// ─── Helpers ────────────────────────────────────────────────────────

fn parse_building_type(s: &str) -> Result<game_core::BuildingType, StatusCode> {
    match s {
        "Headquarters" => Ok(game_core::BuildingType::Headquarters),
        "PowerPlant" => Ok(game_core::BuildingType::PowerPlant),
        "SupplyDepot" => Ok(game_core::BuildingType::SupplyDepot),
        "Barracks" => Ok(game_core::BuildingType::Barracks),
        "Factory" => Ok(game_core::BuildingType::Factory),
        "Airfield" => Ok(game_core::BuildingType::Airfield),
        "Port" => Ok(game_core::BuildingType::Port),
        "Bunker" => Ok(game_core::BuildingType::Bunker),
        "TrenchLine" => Ok(game_core::BuildingType::TrenchLine),
        "Minefield" => Ok(game_core::BuildingType::Minefield),
        "AABattery" => Ok(game_core::BuildingType::AABattery),
        "AntiTankPosition" => Ok(game_core::BuildingType::AntiTankPosition),
        "Wall" => Ok(game_core::BuildingType::Wall),
        "Mine" => Ok(game_core::BuildingType::Mine),
        "OilWell" => Ok(game_core::BuildingType::OilWell),
        "Farm" => Ok(game_core::BuildingType::Farm),
        "LumberMill" => Ok(game_core::BuildingType::LumberMill),
        "RadarStation" => Ok(game_core::BuildingType::RadarStation),
        "CommsTower" => Ok(game_core::BuildingType::CommsTower),
        "ResearchLab" => Ok(game_core::BuildingType::ResearchLab),
        "Hospital" => Ok(game_core::BuildingType::Hospital),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

fn parse_terrain(s: &str) -> game_core::Terrain {
    match s {
        "DeepOcean" => game_core::Terrain::DeepOcean,
        "Ocean" => game_core::Terrain::Ocean,
        "Coast" => game_core::Terrain::Coast,
        "Plains" => game_core::Terrain::Plains,
        "Forest" => game_core::Terrain::Forest,
        "Hills" => game_core::Terrain::Hills,
        "Mountain" => game_core::Terrain::Mountain,
        "Desert" => game_core::Terrain::Desert,
        "Tundra" => game_core::Terrain::Tundra,
        "Urban" => game_core::Terrain::Urban,
        "Ice" => game_core::Terrain::Ice,
        _ => game_core::Terrain::Plains,
    }
}

/// Generate resource deposits for a given macro terrain, matching game-core logic.
fn generate_deposits_for_terrain(macro_terrain: &str, seed: u64) -> Vec<(i32, i32, String, f32)> {
    let mut deposits = Vec::new();
    let mut rng_state = seed + 999;

    let count = match macro_terrain {
        "Mountain" | "Hills" => 4,
        "Desert" => 3,
        "Forest" => 3,
        _ => 2,
    };

    let deposit_type = match macro_terrain {
        "Mountain" | "Hills" => "Metals",
        "Desert" => "Oil",
        "Forest" => "Timber",
        "Plains" => "Farmland",
        _ => "Metals",
    };

    for _ in 0..count {
        // Simple xorshift for deterministic placement
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let dq = ((rng_state % 54) + 5) as i32;

        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let dr = ((rng_state % 54) + 5) as i32;

        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let richness = 0.5 + (rng_state % 150) as f32 / 100.0;

        deposits.push((dq, dr, deposit_type.to_string(), richness));
    }

    deposits
}
