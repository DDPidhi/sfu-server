mod sfu;
mod config;
mod api;
mod error;
mod recording;
mod ipfs;
mod substrate;

use warp::Filter;
use config::Config;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() {
    // Initialize tracing subscriber with environment filter
    // Set RUST_LOG environment variable to control log levels
    // Example: RUST_LOG=info,sfu_server=debug
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into())
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting SFU server");

    let config = Config::from_env();
    tracing::info!(
        host = %config.server.host,
        port = config.server.port,
        "Server configuration loaded"
    );

    // Initialize Asset Hub EVM blockchain integration if configured
    let event_queue = match substrate::init_from_env().await {
        Some((_client, queue)) => {
            tracing::info!("Asset Hub EVM blockchain integration enabled");
            Some(queue)
        }
        None => {
            tracing::info!("Asset Hub EVM blockchain integration disabled");
            None
        }
    };

    let routes = api::sfu_routes::sfu_websocket_route_with_queue(event_queue)
        .or(api::sfu_routes::sfu_health_check())
        .or(api::sfu_routes::sfu_config_endpoint());

    tracing::info!("Starting server on {}:{}", config.server.host, config.server.port);

    warp::serve(routes)
        .run(config.bind_address())
        .await;
}