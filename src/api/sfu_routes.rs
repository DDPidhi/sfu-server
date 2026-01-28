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

            let config = serde_json::json!({
                "SFU_WEBSOCKET_URL": env::var("SFU_WEBSOCKET_URL").ok(),
                "STUN_SERVER_URL": env::var("STUN_SERVER_URL").ok(),
                "PROCTOR_UI_URL": env::var("PROCTOR_UI_URL").ok(),
                "STUDENT_UI_URL": env::var("STUDENT_UI_URL").ok()
            });

            warp::reply::json(&config)
        })
}

fn with_sfu_server(
    sfu_server: Arc<SfuServer>,
) -> impl Filter<Extract = (Arc<SfuServer>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || sfu_server.clone())
}