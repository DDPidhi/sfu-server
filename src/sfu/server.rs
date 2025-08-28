use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use warp::ws::Message;
use webrtc::api::API;

use super::connection::{SfuConnection, TrackNotificationSender};
use super::room::{RoomManager, PeerRole};
use super::track_manager::TrackManager;
use super::signaling::SfuMessage;


pub struct SfuServer {
    api: Arc<API>,
    connections: Arc<RwLock<HashMap<String, Arc<SfuConnection>>>>,
    pending_students: Arc<RwLock<HashMap<String, mpsc::UnboundedSender<Message>>>>,
    track_manager: Arc<TrackManager>,
    room_manager: Arc<RoomManager>,
    track_notification_sender: TrackNotificationSender,
    track_notification_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<(String, String)>>>>,
    peers_with_tracks: Arc<RwLock<HashMap<String, usize>>>,
}

impl SfuServer {
    pub fn new() -> Self {
        use super::webrtc_utils;
        let api = webrtc_utils::create_webrtc_api();

        let (track_sender, track_receiver) = mpsc::unbounded_channel();

        let server = Self {
            api,
            connections: Arc::new(RwLock::new(HashMap::new())),
            pending_students: Arc::new(RwLock::new(HashMap::new())),
            track_manager: Arc::new(TrackManager::new()),
            room_manager: RoomManager::new(),
            track_notification_sender: track_sender,
            track_notification_receiver: Arc::new(RwLock::new(Some(track_receiver))),
            peers_with_tracks: Arc::new(RwLock::new(HashMap::new())),
        };

        server
    }

    pub fn start_track_processing(self: Arc<Self>) {
        let server = self.clone();

        tokio::spawn(async move {
            let mut receiver = {
                let mut receiver_guard = server.track_notification_receiver.write().await;
                receiver_guard.take()
            };

            if let Some(mut rx) = receiver {
                while let Some((peer_id, track_id)) = rx.recv().await {
                    if let Err(e) = server.handle_track_received(&peer_id, &track_id).await {
                        println!("Error processing track notification: {}", e);
                    }
                }
            }
        });
    }

    pub async fn handle_track_received(&self, peer_id: &str, track_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Handling new track {} from peer {}", track_id, peer_id);

        Ok(())
    }
}