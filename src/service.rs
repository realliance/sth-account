use axum::{routing::get, Router};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::{config::Config, error::Result, health};

pub async fn run_service() -> Result<()> {
    let config = Config::from_env()?;
    
    let app = Router::new()
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check))
        .layer(TraceLayer::new_for_http());

    let bind_addr = format!("{}:{}", config.bind_address, config.bind_port);
    info!("Starting service on {}", bind_addr);

    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}