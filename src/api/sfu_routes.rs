use std::sync::Arc;
use warp::Filter;

use crate::sfu::SfuServer;
use crate::substrate::EventQueue;
use super::sfu_websocket;


/// Creates the SFU WebSocket route with optional blockchain integration
pub fn sfu_websocket_route_with_queue(
    event_queue: Option<EventQueue>,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let mut sfu_server = SfuServer::new();

    // Set up blockchain event queue if available
    if let Some(queue) = event_queue {
        sfu_server.set_event_queue(queue);
        tracing::info!("SFU server configured with blockchain integration");
    }

    let sfu_server = Arc::new(sfu_server);
    sfu_server.clone().start_track_processing();

    warp::path("sfu")
        .and(warp::ws())
        .and(with_sfu_server(sfu_server))
        .map(|ws: warp::ws::Ws, sfu_server: Arc<SfuServer>| {
            ws.on_upgrade(move |websocket| {
                sfu_websocket::handle_sfu_websocket(websocket, sfu_server)
            })
        })
}

/// Creates the SFU WebSocket route without blockchain integration
pub fn sfu_websocket_route() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    sfu_websocket_route_with_queue(None)
}

pub fn sfu_health_check() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("sfu")
        .and(warp::path("health"))
        .and(warp::get())
        .map(|| {
            warp::reply::json(&serde_json::json!({
                "status": "healthy",
                "service": "SFU Server",
                "version": "1.0.0"
            }))
        })
}

pub fn sfu_config_endpoint() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path("sfu")
        .and(warp::path("config"))
        .and(warp::get())
        .map(|| {
            use std::env;

            // Check blockchain configuration (without exposing private key)
            let blockchain_enabled = env::var("ASSET_HUB_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false);

            let blockchain_config = if blockchain_enabled {
                serde_json::json!({
                    "enabled": true,
                    "rpc_url": env::var("ASSET_HUB_RPC_URL").ok(),
                    "contract_address": env::var("ASSET_HUB_CONTRACT_ADDRESS").ok(),
                    "gas_limit": env::var("ASSET_HUB_GAS_LIMIT").ok(),
                    "submission_timeout_secs": env::var("ASSET_HUB_SUBMISSION_TIMEOUT_SECS").ok(),
                    "retry_count": env::var("ASSET_HUB_RETRY_COUNT").ok(),
                })
            } else {
                serde_json::json!({
                    "enabled": false
                })
            };

            // Check recording configuration
            let recording_enabled = env::var("RECORDING_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false);

            let recording_config = if recording_enabled {
                serde_json::json!({
                    "enabled": true,
                    "output_dir": env::var("RECORDING_OUTPUT_DIR").ok(),
                    "format": env::var("RECORDING_FORMAT").unwrap_or_else(|_| "webm".to_string()),
                })
            } else {
                serde_json::json!({
                    "enabled": false
                })
            };

            // Check IPFS configuration
            let ipfs_enabled = env::var("IPFS_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false);

            let ipfs_config = if ipfs_enabled {
                serde_json::json!({
                    "enabled": true,
                    "api_url": env::var("IPFS_API_URL").ok(),
                    "gateway_url": env::var("IPFS_GATEWAY_URL").ok(),
                })
            } else {
                serde_json::json!({
                    "enabled": false
                })
            };

            let config = serde_json::json!({
                "SFU_WEBSOCKET_URL": env::var("SFU_WEBSOCKET_URL").ok(),
                "STUN_SERVER_URL": env::var("STUN_SERVER_URL").ok(),
                "PROCTOR_UI_URL": env::var("PROCTOR_UI_URL").ok(),
                "STUDENT_UI_URL": env::var("STUDENT_UI_URL").ok(),
                "blockchain": blockchain_config,
                "recording": recording_config,
                "ipfs": ipfs_config,
            });

            warp::reply::json(&config)
        })
}

fn with_sfu_server(
    sfu_server: Arc<SfuServer>,
) -> impl Filter<Extract = (Arc<SfuServer>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || sfu_server.clone())
}