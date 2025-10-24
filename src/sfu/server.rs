use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
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
    pending_renegotiations: Arc<RwLock<HashMap<String, bool>>>,
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
            pending_renegotiations: Arc::new(RwLock::new(HashMap::new())),
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


    pub async fn create_room(&self, proctor_id: String, proctor_name: Option<String>) -> Result<String, String> {
        self.room_manager.create_room(proctor_id, proctor_name).await
    }


    pub async fn add_peer_with_role(
        &self,
        peer_id: String,
        room_id: String,
        role: String,
        name: Option<String>,
        sender: mpsc::UnboundedSender<Message>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

        if role == "student" {
            let mut retries = 0;
            while !self.is_proctor_ready(&room_id).await && retries < 15 {
                println!(" Waiting for proctor tracks... (retry {}/15)", retries);
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                retries += 1;
            }

            if !self.is_proctor_ready(&room_id).await {
                println!("Proctor tracks not ready after 3s, continuing anyway");
            } else {
                println!("Proctor tracks ready, adding student now");
            }

            self.room_manager.join_room(room_id.clone(), peer_id.clone(), name).await?;
        }


        self.add_peer(peer_id, room_id, sender).await
    }


    pub async fn add_peer(
        &self,
        peer_id: String,
        room_id: String,
        sender: mpsc::UnboundedSender<Message>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Adding peer {} to SFU", peer_id);

        // Create SFU connection
        let connection = Arc::new(
            SfuConnection::new(
                peer_id.clone(),
                sender,
                &self.api,
                self.track_manager.clone(),
                Some(self.track_notification_sender.clone()),
            )
                .await?,
        );

        let existing_tracks = self.get_tracks_for_peer(&peer_id, &room_id).await;
        if !existing_tracks.is_empty() {
            println!("Adding {} existing tracks to peer {}", existing_tracks.len(), peer_id);
            connection
                .add_existing_tracks(self.track_manager.clone(), existing_tracks)
                .await?;
        } else {
            println!("No existing tracks to add to peer {}", peer_id);
        }

        {
            let mut connections = self.connections.write().await;
            connections.insert(peer_id.clone(), connection.clone());
        }

        self.create_and_send_offer(&peer_id).await?;

        println!("Peer {} added to SFU successfully", peer_id);
        Ok(())
    }

    pub async fn remove_peer(&self, peer_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Removing peer {} from SFU", peer_id);

        // Remove peer from room manager (this handles room closure if proctor leaves)
        let room_info = self.room_manager.remove_peer(peer_id).await;

        // Remove connection
        let connection = {
            let mut connections = self.connections.write().await;
            connections.remove(peer_id)
        };

        if let Some(connection) = connection {
            connection.close().await;
        }

        // Remove tracks from this peer
        self.track_manager.remove_peer_tracks(peer_id).await;

        // If proctor left, close all connections in that room
        if let Some((room_id, role)) = room_info {
            if matches!(role, PeerRole::Proctor) {
                // Get all student connections to close
                let students_to_close: Vec<String> = self.room_manager.get_room_peers(&room_id).await
                    .into_iter()
                    .filter(|p| p.id != peer_id)
                    .map(|p| p.id)
                    .collect();

                // Close all student connections
                for student_id in students_to_close {
                    self.close_peer_connection(&student_id).await;
                }
            } else {
                // Update all other connections to remove tracks from this peer
                self.update_all_connections_for_peer_removal(peer_id).await?;
            }
        }

        println!("Peer {} removed from SFU", peer_id);
        Ok(())
    }


    async fn close_peer_connection(&self, peer_id: &str) {
        println!("Closing connection for peer {}", peer_id);

        // Remove connection
        let connection = {
            let mut connections = self.connections.write().await;
            connections.remove(peer_id)
        };

        if let Some(connection) = connection {
            connection.close().await;
        }

        // Remove tracks from this peer
        self.track_manager.remove_peer_tracks(peer_id).await;
    }


    async fn create_and_send_offer(&self, peer_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connection = {
            let connections = self.connections.read().await;
            connections.get(peer_id).cloned()
        };

        if let Some(connection) = connection {
            let offer = connection.peer_connection.create_offer(None).await?;
            connection.peer_connection.set_local_description(offer.clone()).await?;

            let offer_message = serde_json::to_string(&serde_json::json!({
                "type": "offer",
                "sdp": offer.sdp,
                "peer_id": "sfu"
            }))?;

            connection.send_message(Message::text(offer_message)).await?;
            println!("Sent SFU offer to peer: {}", peer_id);
        }

        Ok(())
    }


    pub async fn handle_answer(
        &self,
        peer_id: &str,
        sdp: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connection = {
            let connections = self.connections.read().await;
            connections.get(peer_id).cloned()
        };

        if let Some(connection) = connection {
            use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

            let answer = RTCSessionDescription::answer(sdp.to_string()).unwrap();
            connection.peer_connection.set_remote_description(answer).await?;
            println!("Processed answer from peer: {}", peer_id);
            println!("Answer processed, waiting for tracks from peer: {}", peer_id);
        }

        Ok(())
    }


    pub async fn handle_ice_candidate(
        &self,
        peer_id: &str,
        candidate: &str,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connection = {
            let connections = self.connections.read().await;
            connections.get(peer_id).cloned()
        };

        if let Some(connection) = connection {
            if connection.peer_connection.remote_description().await.is_none() {
                println!("Warning: Received ICE candidate for {} before remote description was set", peer_id);
            }

            println!("SERVER receiving ICE candidate from peer {}", peer_id);

            let ice_candidate = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
                candidate: candidate.to_string(),
                sdp_mid,
                sdp_mline_index,
                username_fragment: None,
            };

            connection.peer_connection.add_ice_candidate(ice_candidate).await?;
            println!("SERVER added ICE candidate from peer {}", peer_id);
        }

        Ok(())
    }


    async fn get_tracks_for_peer(&self, peer_id: &str, room_id: &str) -> Vec<String> {
        let mut tracks_to_forward = Vec::new();

        let room_peers = self.room_manager.get_room_peers(room_id).await;

        let all_tracks = self.track_manager.get_all_track_ids().await;

        for track_id in all_tracks {
            for peer in &room_peers {
                if track_id.starts_with(&peer.id) && peer.id != *peer_id {
                    // Check if this track should be forwarded based on roles
                    if self.room_manager.should_forward_track(&peer.id, peer_id).await {
                        tracks_to_forward.push(track_id.clone());
                    }
                    break;
                }
            }
        }

        tracks_to_forward
    }


    async fn is_proctor_ready(&self, room_id: &str) -> bool {
        let proctor_id = match self.room_manager.get_room_proctor(room_id).await {
            Some(id) => id,
            None => {
                println!("No proctor found for room {}", room_id);
                return false;
            }
        };

        let peers = self.peers_with_tracks.read().await;
        let track_count = peers.get(&proctor_id).unwrap_or(&0);

        let ready = *track_count >= 1;
        println!("Proctor {} readiness check: {} tracks, ready={}", proctor_id, track_count, ready);
        ready
    }

    pub async fn handle_track_received(&self, peer_id: &str, track_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Handling new track {} from peer {}", track_id, peer_id);

        {
            let mut peers = self.peers_with_tracks.write().await;
            *peers.entry(peer_id.to_string()).or_insert(0) += 1;
            println!("Peer {} now has {} tracks", peer_id, peers.get(peer_id).unwrap());
        }

        let connections = self.connections.read().await;
        for (target_peer_id, connection) in connections.iter() {
            if target_peer_id != peer_id {
                if !self.room_manager.should_forward_track(peer_id, target_peer_id).await {
                    continue;
                }

                if let Some(local_track) = self
                    .track_manager
                    .create_local_track_for_peer(track_id, target_peer_id)
                    .await
                {
                    connection.peer_connection.add_track(local_track).await?;
                    println!("Added track {} to peer {}", track_id, target_peer_id);

                    let should_schedule = {
                        let mut pending = self.pending_renegotiations.write().await;
                        let is_pending = pending.contains_key(target_peer_id);
                        pending.insert(target_peer_id.to_string(), true);
                        !is_pending
                    };

                    if should_schedule {
                        println!("Scheduling renegotiation for peer {} in 150ms", target_peer_id);
                        let connections_clone = self.connections.clone();
                        let target_id = target_peer_id.clone();
                        let pending_clone = self.pending_renegotiations.clone();
                        tokio::spawn(async move {
                            sleep(Duration::from_millis(150)).await;
                            let _ = Self::perform_renegotiation_static(connections_clone, pending_clone, &target_id).await;
                        });
                    } else {
                        println!("Renegotiation already scheduled for peer {}, batching tracks", target_peer_id);
                    }
                }
            }
        }

        Ok(())
    }

    async fn perform_renegotiation_static(
        connections: Arc<RwLock<HashMap<String, Arc<SfuConnection>>>>,
        pending: Arc<RwLock<HashMap<String, bool>>>,
        target_peer_id: &str
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let mut pending_map = pending.write().await;
            pending_map.remove(target_peer_id);
        }

        let connection = {
            let connections_map = connections.read().await;
            connections_map.get(target_peer_id).cloned()
        };

        if let Some(connection) = connection {
            let signaling_state = connection.peer_connection.signaling_state();
            println!("Signaling state for peer {}: {:?}", target_peer_id, signaling_state);

            if signaling_state == webrtc::peer_connection::signaling_state::RTCSignalingState::Stable {
                println!("Creating renegotiation offer for peer {} (batched)", target_peer_id);

                let offer = connection.peer_connection.create_offer(None).await?;
                connection.peer_connection.set_local_description(offer.clone()).await?;
                println!("Set local description for peer {}", target_peer_id);

                let renegotiate_message = serde_json::to_string(&serde_json::json!({
                    "type": "renegotiate",
                    "sdp": offer.sdp
                }))?;

                connection.send_message(Message::text(renegotiate_message)).await?;
                println!("Sent renegotiation offer to {}", target_peer_id);
            } else {
                println!("Signaling state not stable for peer {}: {:?}", target_peer_id, signaling_state);
                println!("Tracks were added but renegotiation skipped - will happen on next track or when stable");
            }
        }

        Ok(())
    }

    async fn update_all_connections_for_peer_removal(
        &self,
        removed_peer_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Should remove tracks from {} in all other connections", removed_peer_id);
        Ok(())
    }

    pub async fn forward_join_request(
        &self,
        room_id: String,
        student_peer_id: String,
        student_name: Option<String>,
        role: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let proctor_peer_id = self.room_manager.get_room_proctor(&room_id).await;

        if let Some(proctor_id) = proctor_peer_id {
            let connections = self.connections.read().await;
            if let Some(proctor_connection) = connections.get(&proctor_id) {
                let join_request_message = SfuMessage::JoinRequest {
                    room_id,
                    peer_id: student_peer_id,
                    name: student_name,
                    role,
                };

                let message_str = serde_json::to_string(&join_request_message)?;
                proctor_connection.send_message(Message::text(message_str)).await?;

                return Ok(());
            }
        }

        Err("Proctor not found for this room".into())
    }

    pub async fn track_pending_student(
        &self,
        student_peer_id: String,
        sender: mpsc::UnboundedSender<Message>,
    ) {
        let mut pending = self.pending_students.write().await;
        pending.insert(student_peer_id, sender);
    }


    pub async fn send_join_response(
        &self,
        room_id: String,
        student_peer_id: String,
        approved: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        {
            let connections = self.connections.read().await;
            if let Some(student_connection) = connections.get(&student_peer_id) {
                let response_message = if approved {
                    serde_json::json!({
                        "type": "join_approved",
                        "room_id": room_id,
                        "message": "Join request approved! Connecting to room..."
                    })
                } else {
                    serde_json::json!({
                        "type": "join_denied",
                        "room_id": room_id,
                        "message": "Join request denied by proctor"
                    })
                };

                let message_str = serde_json::to_string(&response_message)?;
                student_connection.send_message(Message::text(message_str)).await?;

                return Ok(());
            }
        }


        let pending = self.pending_students.read().await;
        if let Some(student_sender) = pending.get(&student_peer_id) {
            let response_message = if approved {
                serde_json::json!({
                    "type": "join_approved",
                    "room_id": room_id,
                    "message": "Join request approved! Connecting to room..."
                })
            } else {
                serde_json::json!({
                    "type": "join_denied",
                    "room_id": room_id,
                    "message": "Join request denied by proctor"
                })
            };

            let message_str = serde_json::to_string(&response_message)?;
            student_sender.send(Message::text(message_str))?;

            return Ok(());
        }

        Err("Student connection not found".into())
    }


    pub async fn remove_pending_student(&self, student_peer_id: &str) {
        let mut pending = self.pending_students.write().await;
        pending.remove(student_peer_id);
    }
}