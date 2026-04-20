use game_core::StrategicGrid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://game:game_dev@localhost:5432/grand_strategy".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    tracing::info!("Connected to PostgreSQL");

    let width = 1200i32;
    let height = 600i32;

    let map_path = std::env::var("MAP_PNG")
        .unwrap_or_else(|_| "apps/web/public/maps/flat-earth.png".to_string());

    let grid = match image::open(&map_path) {
        Ok(dyn_img) => {
            let rgb = dyn_img.to_rgb8();
            let iw = rgb.width();
            let ih = rgb.height();
            tracing::info!("Sampling terrain from {} ({}x{})", map_path, iw, ih);
            StrategicGrid::from_map_rgb(width, height, rgb.as_raw(), iw, ih)
        }
        Err(e) => {
            tracing::warn!(
                "Failed to load {}: {}. Falling back to procedural terrain.",
                map_path,
                e
            );
            StrategicGrid::new(width, height)
        }
    };

    // Batch insert hex data into the database
    const BATCH_SIZE: usize = 10_000;
    let total = (width * height) as usize;
    let mut inserted = 0usize;

    for chunk in grid.cells().chunks(BATCH_SIZE) {
        let mut query_builder = sqlx::query_builder::QueryBuilder::new(
            "INSERT INTO hex_map (q, r, terrain, elevation) VALUES ",
        );

        for (i, cell) in chunk.iter().enumerate() {
            if i > 0 {
                query_builder.push(", ");
            }
            query_builder.push("(");
            query_builder.push_bind(cell.hex.q);
            query_builder.push(", ");
            query_builder.push_bind(cell.hex.r);
            query_builder.push(", ");
            query_builder.push_bind(cell.terrain.as_str());
            query_builder.push(", ");
            query_builder.push_bind(cell.elevation);
            query_builder.push(")");
        }

        query_builder.build().execute(&pool).await?;
        inserted += chunk.len();
        tracing::info!("Inserted {}/{} hexes", inserted, total);
    }

    tracing::info!("Seed complete: {} hexes inserted", inserted);
    Ok(())
}
