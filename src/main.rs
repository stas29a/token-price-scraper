use crate::app::App;
use crate::persistence::PriceRepositoryImpl;
use crate::scraper::GeckoScraper;
use diesel::r2d2::ConnectionManager;
use diesel::PgConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use log::info;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::{EnvFilter, fmt};
use tracing_unwrap::ResultExt;

mod app;
mod config;
mod data;
mod persistence;
mod schema;
mod scraper;
mod web;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();
    info!("Starting the application...");

    let config_path = std::env::var("CONFIG_PATH").unwrap_or_else(|_| "config.toml".to_string());
    let config = config::Config::from_file(std::path::Path::new(&config_path))?;
    let scraper = GeckoScraper::new(
        "https://api.coingecko.com".to_string(),
        config.gecko_api_url.clone(),
    );

    let manager = ConnectionManager::<PgConnection>::new(config.database_url.clone());
    let connection_pool = r2d2::Pool::builder()
        .max_size(10)
        .build(manager)
        .expect("Failed to create pool.");

    {
        let mut connection = connection_pool.get()?;
        connection
            .run_pending_migrations(MIGRATIONS)
            .unwrap_or_log();
    }

    let cancellation_token = CancellationToken::new();
    let arc_connection_pool = Arc::new(connection_pool);
    let repository = Arc::new(
        PriceRepositoryImpl::new(cancellation_token.clone(), arc_connection_pool.clone()).await?,
    );

    let mut app = App::new(config, scraper, repository);

    app.run(cancellation_token).await?;

    Ok(())
}
