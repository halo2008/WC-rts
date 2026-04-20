use axum::{
    Router,
    routing::get,
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde::Serialize;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod battlefield;
mod combat;
mod economy;
mod groups;
mod map;
mod messages;
mod nations;
mod tick;
mod units;
mod ws;

use messages::WsMessage;
use tick::TerrainCache;

/// Shared application state passed to every handler and the tick loops.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub terrain: TerrainCache,
    /// Unified broadcast channel: transit deltas, nation-state updates, spawn
    /// events. Every WS subscriber clones the receiver.
    pub tx: broadcast::Sender<WsMessage>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn db_health(State(state): State<AppState>) -> Result<Json<HealthResponse>, StatusCode> {
    match sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db).await {
        Ok(_) => Ok(Json(HealthResponse {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
        })),
        Err(e) => {
            tracing::error!("DB health check failed: {}", e);
            Err(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("game_server=info".parse()?))
        .init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://game:game_dev@localhost:5432/grand_strategy".to_string());

    let db = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await?;

    tracing::info!("Connected to PostgreSQL");

    sqlx::migrate!("./migrations").run(&db).await?;
    tracing::info!("Database migrations applied");

    // Load terrain once. If hex_map is empty (seed tool hasn't run), the
    // cache stays empty and the tick loop falls back to Plains.
    let terrain = tick::load_terrain(&db).await?;

    // Broadcast channel shared by strategic + economy ticks and every WS
    // subscriber. 1024-deep buffer tolerates brief client backpressure.
    let (tx, _rx) = broadcast::channel::<WsMessage>(1024);

    let state = AppState {
        db: db.clone(),
        terrain,
        tx,
    };

    // Background tasks. tokio drops them on shutdown.
    tokio::spawn(tick::strategic_tick_loop(state.clone()));
    tokio::spawn(economy::economy_tick_loop(state.clone()));

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/health/db", get(db_health))
        .nest("/api/map", map::routes())
        .nest("/api/battlefield", battlefield::routes())
        .nest("/api/nations", nations::routes())
        .nest("/api/units", units::routes())
        .nest("/api/groups", groups::routes())
        .nest("/ws", ws::routes())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()?;

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("Game server listening on port {}", port);

    axum::serve(listener, app).await?;

    Ok(())
}
