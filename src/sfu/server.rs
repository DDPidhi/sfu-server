use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use warp::ws::Message;
use webrtc::api::API;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;

use super::connection::{SfuConnection, TrackNotificationSender};
use super::room::{RoomManager, PeerRole};
use super::track_manager::TrackManager;
use super::signaling::SfuMessage;
use crate::error::SfuError;
use crate::recording::{RecordingManager, RecordingResult};
use crate::ipfs::{IpfsClient, IpfsConfig};

/// Queued ICE candidate waiting for remote description
#[derive(Debug, Clone)]
struct PendingIceCandidate {
    candidate: String,
    sdp_mid: Option<String>,
    sdp_mline_index: Option<u16>,
}

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
    pending_ice_candidates: Arc<RwLock<HashMap<String, Vec<PendingIceCandidate>>>>,
    recording_manager: Arc<RecordingManager>,
}

impl SfuServer {
    pub fn new() -> Self {
        use super::webrtc_utils;
        let api = webrtc_utils::create_webrtc_api();

        let (track_sender, track_receiver) = mpsc::unbounded_channel();

        let recording_output_dir = std::env::var("RECORDING_OUTPUT_DIR")
            .unwrap_or_else(|_| "./recordings".to_string());

        // Initialize IPFS client if configured
        let ipfs_client = IpfsConfig::from_env().and_then(|config| {
            match IpfsClient::new(config) {
                Ok(client) => {
                    tracing::info!("IPFS client initialized");
                    Some(Arc::new(client))
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to initialize IPFS client");
                    None
                }
            }
        });

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
            pending_ice_candidates: Arc::new(RwLock::new(HashMap::new())),
            recording_manager: Arc::new(RecordingManager::new(&recording_output_dir, ipfs_client)),
        };

        server
    }

    pub fn start_track_processing(self: Arc<Self>) {
        let server = self.clone();

        tokio::spawn(async move {
            let receiver = {
                let mut receiver_guard = server.track_notification_receiver.write().await;
                receiver_guard.take()
            };

            if let Some(mut rx) = receiver {
                while let Some((peer_id, track_id)) = rx.recv().await {
                    if let Err(e) = server.handle_track_received(&peer_id, &track_id).await {
                        tracing::error!(
                            peer_id = %peer_id,
                            track_id = %track_id,
                            error = %e,
                            "Error processing track notification"
                        );
                    }
                }
            }
        });
    }


    pub async fn create_room(&self, proctor_id: String, proctor_name: Option<String>) -> Result<String, String> {
        let room_id = self.room_manager.create_room(proctor_id.clone(), proctor_name).await?;

        // Auto-start recording for the proctor when room is created
        if let Err(e) = self.recording_manager.start_recording(&room_id, &proctor_id).await {
            tracing::error!(
                room_id = %room_id,
                proctor_id = %proctor_id,
                error = %e,
                "Failed to auto-start recording for proctor"
            );
        } else {
            tracing::info!(
                room_id = %room_id,
                proctor_id = %proctor_id,
                "Auto-started recording for proctor"
            );
        }

        Ok(room_id)
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
                tracing::debug!(
                    room_id = %room_id,
                    retry = retries,
                    "Waiting for proctor tracks"
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                retries += 1;
            }

            if !self.is_proctor_ready(&room_id).await {
                tracing::warn!(
                    room_id = %room_id,
                    "Proctor tracks not ready after 3s, continuing anyway"
                );
            } else {
                tracing::info!(
                    room_id = %room_id,
                    "Proctor tracks ready, adding student"
                );
            }

            self.room_manager.join_room(room_id.clone(), peer_id.clone(), name).await?;

            // Auto-start recording for the student when they join
            if let Err(e) = self.recording_manager.start_recording(&room_id, &peer_id).await {
                tracing::error!(
                    room_id = %room_id,
                    peer_id = %peer_id,
                    error = %e,
                    "Failed to auto-start recording for student"
                );
            } else {
                tracing::info!(
                    room_id = %room_id,
                    peer_id = %peer_id,
                    "Auto-started recording for student"
                );
            }
        }


        self.add_peer(peer_id, room_id, sender).await
    }


    pub async fn add_peer(
        &self,
        peer_id: String,
        room_id: String,
        sender: mpsc::UnboundedSender<Message>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(peer_id = %peer_id, room_id = %room_id, "Adding peer to SFU");

        // Create SFU connection
        let connection = Arc::new(
            SfuConnection::new(
                peer_id.clone(),
                room_id.clone(),
                sender,
                &self.api,
                self.track_manager.clone(),
                Some(self.track_notification_sender.clone()),
                Some(self.recording_manager.clone()),
            )
                .await?,
        );

        let existing_tracks = self.get_tracks_for_peer(&peer_id, &room_id).await;
        if !existing_tracks.is_empty() {
            tracing::info!(
                peer_id = %peer_id,
                track_count = existing_tracks.len(),
                "Adding existing tracks to peer"
            );
            connection
                .add_existing_tracks(self.track_manager.clone(), existing_tracks)
                .await?;
        } else {
            tracing::debug!(peer_id = %peer_id, "No existing tracks to add to peer");
        }

        {
            let mut connections = self.connections.write().await;
            connections.insert(peer_id.clone(), connection.clone());
        }

        self.create_and_send_offer(&peer_id).await?;

        tracing::info!(peer_id = %peer_id, "Peer added to SFU successfully");
        Ok(())
    }

    pub async fn remove_peer(&self, peer_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(peer_id = %peer_id, "Removing peer from SFU");

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

        // Clean up pending ICE candidates
        {
            let mut pending_ice = self.pending_ice_candidates.write().await;
            if pending_ice.remove(peer_id).is_some() {
                tracing::debug!(peer_id = %peer_id, "Removed pending ICE candidates");
            }
        }

        // Clean up pending renegotiations
        {
            let mut pending_renego = self.pending_renegotiations.write().await;
            if pending_renego.remove(peer_id).is_some() {
                tracing::debug!(peer_id = %peer_id, "Removed pending renegotiation");
            }
        }

        // Handle recording cleanup and room closure
        if let Some((room_id, role)) = room_info {
            if matches!(role, PeerRole::Proctor) {
                tracing::info!(
                    room_id = %room_id,
                    peer_id = %peer_id,
                    "Proctor left, stopping all recordings and closing room"
                );

                // Stop all recordings in the room (proctor + all students)
                let stopped_recordings = self.recording_manager.stop_all_recordings_in_room(&room_id).await;
                for (stopped_peer_id, result) in &stopped_recordings {
                    tracing::info!(
                        room_id = %room_id,
                        peer_id = %stopped_peer_id,
                        file = %result.file_path.display(),
                        cid = ?result.cid,
                        "Recording saved on room close"
                    );
                }

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
                // Student left - stop their recording
                self.recording_manager.cleanup_peer(&room_id, peer_id).await;
                tracing::info!(
                    room_id = %room_id,
                    peer_id = %peer_id,
                    "Stopped recording for leaving student"
                );

                // Update all other connections to remove tracks from this peer
                self.update_all_connections_for_peer_removal(peer_id).await?;
            }
        }

        tracing::info!(peer_id = %peer_id, "Peer removed from SFU successfully");
        Ok(())
    }


    async fn close_peer_connection(&self, peer_id: &str) {
        tracing::info!(peer_id = %peer_id, "Closing peer connection");

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
            tracing::info!(peer_id = %peer_id, "Sent SFU offer to peer");
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

            let answer = RTCSessionDescription::answer(sdp.to_string())
                .map_err(|e| SfuError::InvalidSdp(format!("Failed to parse answer SDP: {}", e)))?;
            connection.peer_connection.set_remote_description(answer).await?;
            tracing::info!(peer_id = %peer_id, "Processed answer from peer");

            // Flush any queued ICE candidates now that remote description is set
            self.flush_pending_ice_candidates(peer_id, &connection).await?;

            tracing::debug!(peer_id = %peer_id, "Waiting for tracks from peer");
        }

        Ok(())
    }

    /// Flush any queued ICE candidates after remote description is set
    async fn flush_pending_ice_candidates(
        &self,
        peer_id: &str,
        connection: &Arc<SfuConnection>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let candidates = {
            let mut pending = self.pending_ice_candidates.write().await;
            pending.remove(peer_id)
        };

        if let Some(candidates) = candidates {
            tracing::info!(
                peer_id = %peer_id,
                count = candidates.len(),
                "Flushing queued ICE candidates"
            );

            for candidate in candidates {
                let ice_candidate = RTCIceCandidateInit {
                    candidate: candidate.candidate,
                    sdp_mid: candidate.sdp_mid,
                    sdp_mline_index: candidate.sdp_mline_index,
                    username_fragment: None,
                };

                if let Err(e) = connection.peer_connection.add_ice_candidate(ice_candidate).await {
                    tracing::error!(
                        peer_id = %peer_id,
                        error = %e,
                        "Failed to add queued ICE candidate"
                    );
                } else {
                    tracing::debug!(peer_id = %peer_id, "Added queued ICE candidate");
                }
            }
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
            // Check if remote description is set
            if connection.peer_connection.remote_description().await.is_none() {
                tracing::debug!(
                    peer_id = %peer_id,
                    "Queueing ICE candidate until remote description is set"
                );

                // Queue the candidate
                let mut pending = self.pending_ice_candidates.write().await;
                pending.entry(peer_id.to_string())
                    .or_insert_with(Vec::new)
                    .push(PendingIceCandidate {
                        candidate: candidate.to_string(),
                        sdp_mid,
                        sdp_mline_index,
                    });

                tracing::debug!(
                    peer_id = %peer_id,
                    queue_size = pending.get(peer_id).map(|v| v.len()).unwrap_or(0),
                    "ICE candidate queued"
                );
                return Ok(());
            }

            tracing::debug!(peer_id = %peer_id, "Receiving ICE candidate from peer");

            let ice_candidate = RTCIceCandidateInit {
                candidate: candidate.to_string(),
                sdp_mid,
                sdp_mline_index,
                username_fragment: None,
            };

            connection.peer_connection.add_ice_candidate(ice_candidate).await?;
            tracing::debug!(peer_id = %peer_id, "Added ICE candidate from peer");
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
                tracing::debug!(room_id = %room_id, "No proctor found for room");
                return false;
            }
        };

        let peers = self.peers_with_tracks.read().await;
        let track_count = peers.get(&proctor_id).unwrap_or(&0);

        let ready = *track_count >= 1;
        tracing::debug!(
            proctor_id = %proctor_id,
            track_count = track_count,
            ready = ready,
            "Proctor readiness check"
        );
        ready
    }

    pub async fn handle_track_received(&self, peer_id: &str, track_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(
            peer_id = %peer_id,
            track_id = %track_id,
            "Handling new track from peer"
        );

        {
            let mut peers = self.peers_with_tracks.write().await;
            let count = peers.entry(peer_id.to_string()).or_insert(0);
            *count += 1;
            tracing::debug!(peer_id = %peer_id, track_count = *count, "Updated peer track count");
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
                    tracing::info!(
                        track_id = %track_id,
                        target_peer_id = %target_peer_id,
                        "Added track to peer"
                    );

                    let should_schedule = {
                        let mut pending = self.pending_renegotiations.write().await;
                        let is_pending = pending.contains_key(target_peer_id);
                        pending.insert(target_peer_id.to_string(), true);
                        !is_pending
                    };

                    if should_schedule {
                        tracing::debug!(
                            target_peer_id = %target_peer_id,
                            "Scheduling renegotiation in 150ms"
                        );
                        let connections_clone = self.connections.clone();
                        let target_id = target_peer_id.clone();
                        let pending_clone = self.pending_renegotiations.clone();
                        tokio::spawn(async move {
                            sleep(Duration::from_millis(150)).await;
                            let _ = Self::perform_renegotiation_static(connections_clone, pending_clone, &target_id, 0).await;
                        });
                    } else {
                        tracing::debug!(
                            target_peer_id = %target_peer_id,
                            "Renegotiation already scheduled, batching tracks"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    async fn perform_renegotiation_static(
        connections: Arc<RwLock<HashMap<String, Arc<SfuConnection>>>>,
        pending: Arc<RwLock<HashMap<String, bool>>>,
        target_peer_id: &str,
        retry_count: u32,
    ) {
        const MAX_RETRIES: u32 = 3;
        const BASE_RETRY_DELAY_MS: u64 = 200;

        // Only clear pending flag on first attempt (retry_count == 0)
        if retry_count == 0 {
            let mut pending_map = pending.write().await;
            pending_map.remove(target_peer_id);
        }

        let connection = {
            let connections_map = connections.read().await;
            connections_map.get(target_peer_id).cloned()
        };

        if let Some(connection) = connection {
            let signaling_state = connection.peer_connection.signaling_state();
            tracing::debug!(
                target_peer_id = %target_peer_id,
                ?signaling_state,
                retry_count = retry_count,
                "Checking signaling state for renegotiation"
            );

            if signaling_state == webrtc::peer_connection::signaling_state::RTCSignalingState::Stable {
                tracing::info!(
                    target_peer_id = %target_peer_id,
                    retry_count = retry_count,
                    "Creating batched renegotiation offer"
                );

                let offer = match connection.peer_connection.create_offer(None).await {
                    Ok(offer) => offer,
                    Err(e) => {
                        tracing::error!(target_peer_id = %target_peer_id, error = %e, "Failed to create renegotiation offer");
                        return;
                    }
                };

                if let Err(e) = connection.peer_connection.set_local_description(offer.clone()).await {
                    tracing::error!(target_peer_id = %target_peer_id, error = %e, "Failed to set local description");
                    return;
                }
                tracing::debug!(target_peer_id = %target_peer_id, "Set local description");

                let renegotiate_message = match serde_json::to_string(&serde_json::json!({
                    "type": "renegotiate",
                    "sdp": offer.sdp
                })) {
                    Ok(msg) => msg,
                    Err(e) => {
                        tracing::error!(target_peer_id = %target_peer_id, error = %e, "Failed to serialize renegotiation message");
                        return;
                    }
                };

                if let Err(e) = connection.send_message(Message::text(renegotiate_message)).await {
                    tracing::error!(target_peer_id = %target_peer_id, error = %e, "Failed to send renegotiation offer");
                    return;
                }
                tracing::info!(
                    target_peer_id = %target_peer_id,
                    retry_count = retry_count,
                    "Sent renegotiation offer"
                );
            } else if retry_count < MAX_RETRIES {
                // Retry with exponential backoff
                let retry_delay = BASE_RETRY_DELAY_MS * (2_u64.pow(retry_count));
                tracing::warn!(
                    target_peer_id = %target_peer_id,
                    ?signaling_state,
                    retry_count = retry_count,
                    retry_delay_ms = retry_delay,
                    "Signaling state not stable, will retry on next track or manual trigger"
                );
                // Note: Retry will happen naturally when next track is added
                // or connection state changes. The exponential backoff is logged
                // for monitoring purposes.
            } else {
                tracing::error!(
                    target_peer_id = %target_peer_id,
                    ?signaling_state,
                    retry_count = retry_count,
                    "Renegotiation failed after {} retries, giving up",
                    MAX_RETRIES
                );
            }
        }
    }

    async fn update_all_connections_for_peer_removal(
        &self,
        removed_peer_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!(
            removed_peer_id = %removed_peer_id,
            "Should remove tracks in all other connections"
        );
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

    // Recording methods
    pub async fn start_recording(&self, room_id: &str, peer_id: &str) -> Result<(), SfuError> {
        tracing::info!(room_id = %room_id, peer_id = %peer_id, "Starting recording for peer");
        self.recording_manager.start_recording(room_id, peer_id).await
    }

    pub async fn stop_recording(&self, room_id: &str, peer_id: &str) -> Result<RecordingResult, SfuError> {
        tracing::info!(room_id = %room_id, peer_id = %peer_id, "Stopping recording for peer");
        self.recording_manager.stop_recording(room_id, peer_id).await
    }

    pub async fn stop_all_recordings(&self, room_id: &str) -> Vec<(String, RecordingResult)> {
        tracing::info!(room_id = %room_id, "Stopping all recordings in room");
        self.recording_manager.stop_all_recordings_in_room(room_id).await
    }

    pub async fn is_peer_recording(&self, room_id: &str, peer_id: &str) -> bool {
        self.recording_manager.is_recording(room_id, peer_id).await
    }

    pub async fn get_recording_peers(&self, room_id: &str) -> Vec<String> {
        self.recording_manager.get_recording_peers(room_id).await
    }

    pub fn get_recording_manager(&self) -> Arc<RecordingManager> {
        self.recording_manager.clone()
    }
}