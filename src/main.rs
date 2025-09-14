mod sfu;
mod config;
mod api;

use warp::Filter;
use config::Config;

#[tokio::main]
async fn main() {
    let config = Config::from_env();

    let routes = api::sfu_routes::sfu_websocket_route()
        .or(api::sfu_routes::sfu_health_check())
        .or(api::sfu_routes::sfu_config_endpoint());

    warp::serve(routes)
        .run(config.bind_address())
        .await;
}