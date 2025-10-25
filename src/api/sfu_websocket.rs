use std::sync::Arc;
use tokio::sync::mpsc;
use warp::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};

use crate::sfu::{SfuServer, SfuSignalingHandler, SfuMessage};

pub async fn handle_sfu_websocket(
    websocket: WebSocket,
    sfu_server: Arc<SfuServer>,
) {
    tracing::info!("New SFU WebSocket connection established");

    let (mut ws_sender, mut ws_receiver) = websocket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Create signaling handler
    let mut signaling_handler = SfuSignalingHandler::new(sfu_server, tx);

    // Spawn task to send messages to client
    let sender_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if let Err(e) = ws_sender.send(message).await {
                tracing::error!(error = %e, "Failed to send WebSocket message");
                break;
            }
        }
    });

    while let Some(result) = ws_receiver.next().await {
        match result {
            Ok(message) => {
                if let Err(e) = handle_websocket_message(&mut signaling_handler, message).await {
                    tracing::error!(error = %e, "Error handling WebSocket message");
                    break;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "WebSocket error");
                break;
            }
        }
    }


    signaling_handler.cleanup().await;
    sender_task.abort();
    tracing::info!("SFU WebSocket connection closed");
}

async fn handle_websocket_message(
    signaling_handler: &mut SfuSignalingHandler,
    message: Message,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if let Ok(text) = message.to_str() {
        tracing::debug!("Received SFU message: {}", text);

        match serde_json::from_str::<SfuMessage>(text) {
            Ok(sfu_message) => {
                signaling_handler.handle_message(sfu_message).await;
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    raw_message = %text,
                    "Failed to parse SFU message"
                );
            }
        }
    }

    Ok(())
}