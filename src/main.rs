mod sfu;
mod config;

use tokio::sync::mpsc;
use warp::ws::Message;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let api = sfu::connection::create_api();

    let (tx, _rx) = mpsc::unbounded_channel::<Message>();

    let connection = sfu::connection::BasicSfuConnection::new(
        "peer-123".to_string(),
        tx,
        &api,
    ).await?;

    let offer_sdp = connection.create_offer().await?;
    println!("Offer SDP: {}", offer_sdp);

    connection.close().await;

    Ok(())
}
