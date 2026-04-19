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

    let width = 1200;
    let height = 600;

    tracing::info!("Generating strategic grid {}x{}...", width, height);
    let grid = StrategicGrid::new(width, height);

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
            query_builder.push_bind(format!("{:?}", cell.terrain));
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
