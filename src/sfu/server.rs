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
use crate::substrate::{EventQueue, ChainEvent, Role as ChainRole, LeaveReason as ChainLeaveReason, VerificationStatus as ChainVerificationStatus, SuspiciousActivityType as ChainSuspiciousActivityType, RoomCloseReason as ChainRoomCloseReason, Address, parse_address};

/// Queued ICE candidate waiting for remote description
#[derive(Debug, Clone)]
struct PendingIceCandidate {
    candidate: String,
    sdp_mid: Option<String>,
    sdp_mline_index: Option<u16>,
}

/// Pending student info including wallet address
struct PendingStudent {
    sender: mpsc::UnboundedSender<Message>,
    wallet_address: Option<String>,
}

/// Stores exam result info for a peer
#[derive(Debug, Clone)]
pub struct ExamGrade {
    pub grade: u64,      // Grade in basis points (8500 = 85.00%)
    pub exam_name: String,
}

pub struct SfuServer {
    api: Arc<API>,
    connections: Arc<RwLock<HashMap<String, Arc<SfuConnection>>>>,
    pending_students: Arc<RwLock<HashMap<String, PendingStudent>>>,
    /// Maps peer_id to wallet address for on-chain event emission
    peer_wallets: Arc<RwLock<HashMap<String, Address>>>,
    /// Maps peer_id to their exam grade (set when student submits exam)
    peer_exam_grades: Arc<RwLock<HashMap<String, ExamGrade>>>,
    track_manager: Arc<TrackManager>,
    room_manager: Arc<RoomManager>,
    track_notification_sender: TrackNotificationSender,
    track_notification_receiver: Arc<RwLock<Option<mpsc::UnboundedReceiver<(String, String)>>>>,
    peers_with_tracks: Arc<RwLock<HashMap<String, usize>>>,
    pending_renegotiations: Arc<RwLock<HashMap<String, bool>>>,
    pending_ice_candidates: Arc<RwLock<HashMap<String, Vec<PendingIceCandidate>>>>,
    recording_manager: Arc<RecordingManager>,
    /// Optional blockchain event queue for recording events on-chain
    event_queue: Option<EventQueue>,
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
            peer_wallets: Arc::new(RwLock::new(HashMap::new())),
            peer_exam_grades: Arc::new(RwLock::new(HashMap::new())),
            track_manager: Arc::new(TrackManager::new()),
            room_manager: RoomManager::new(),
            track_notification_sender: track_sender,
            track_notification_receiver: Arc::new(RwLock::new(Some(track_receiver))),
            peers_with_tracks: Arc::new(RwLock::new(HashMap::new())),
            pending_renegotiations: Arc::new(RwLock::new(HashMap::new())),
            pending_ice_candidates: Arc::new(RwLock::new(HashMap::new())),
            recording_manager: Arc::new(RecordingManager::new(&recording_output_dir, ipfs_client)),
            event_queue: None,
        };

        server
    }

    /// Sets the blockchain event queue for recording events on-chain
    pub fn set_event_queue(&mut self, queue: EventQueue) {
        self.event_queue = Some(queue);
        tracing::info!("Blockchain event queue configured");
    }

    /// Helper to emit a chain event if the queue is configured
    fn emit_chain_event(&self, event: ChainEvent) {
        if let Some(ref queue) = self.event_queue {
            queue.emit(event);
        }
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


    pub async fn create_room(&self, proctor_id: String, proctor_name: Option<String>, wallet_address: Option<String>) -> Result<String, String> {
        let room_id = self.room_manager.create_room(proctor_id.clone(), proctor_name.clone()).await?;

        // Store wallet address if provided
        let proctor_wallet = wallet_address.as_ref().and_then(|w| parse_address(w));
        if let Some(wallet) = proctor_wallet {
            let mut wallets = self.peer_wallets.write().await;
            wallets.insert(proctor_id.clone(), wallet);
            tracing::info!(proctor_id = %proctor_id, wallet = %wallet, "Stored proctor wallet address");

            // Emit chain event for room creation with wallet address
            self.emit_chain_event(ChainEvent::RoomCreated {
                room_id: room_id.clone(),
                proctor: wallet,
                proctor_name: proctor_name.clone(),
            });

        } else {
            tracing::debug!(proctor_id = %proctor_id, "No wallet address provided for proctor");
        }

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

            // Emit chain event for recording started (only if wallet is available)
            if let Some(wallet) = proctor_wallet {
                self.emit_chain_event(ChainEvent::RecordingStarted {
                    room_id: room_id.clone(),
                    participant: wallet,
                });
            }
        }

        Ok(room_id)
    }


    pub async fn add_peer_with_role(
        &self,
        peer_id: String,
        room_id: String,
        role: String,
        name: Option<String>,
        wallet_address: Option<String>,
        sender: mpsc::UnboundedSender<Message>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

        let chain_role = if role == "proctor" {
            ChainRole::Proctor
        } else {
            ChainRole::Student
        };

        // For students, try to get wallet from pending_students if not provided
        let effective_wallet = if wallet_address.is_some() {
            wallet_address
        } else if role == "student" {
            let pending = self.pending_students.read().await;
            let wallet = pending.get(&peer_id).and_then(|p| p.wallet_address.clone());
            if wallet.is_some() {
                tracing::info!(peer_id = %peer_id, "Retrieved wallet from pending student");
            }
            wallet
        } else {
            None
        };

        // Clean up pending student entry now that they're joining
        if role == "student" {
            self.remove_pending_student(&peer_id).await;
        }

        // Store wallet address if provided
        let participant_wallet = effective_wallet.as_ref().and_then(|w| parse_address(w));
        if let Some(wallet) = participant_wallet {
            let mut wallets = self.peer_wallets.write().await;
            wallets.insert(peer_id.clone(), wallet);
            tracing::info!(peer_id = %peer_id, wallet = %wallet, "Stored participant wallet address");
        }

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

            self.room_manager.join_room(room_id.clone(), peer_id.clone(), name.clone()).await?;

            // Emit chain event for participant joined (only if wallet is available)
            if let Some(wallet) = participant_wallet {
                self.emit_chain_event(ChainEvent::ParticipantJoined {
                    room_id: room_id.clone(),
                    participant: wallet,
                    name: name.clone(),
                    role: chain_role,
                });
            }

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

                // Emit chain event for recording started (only if wallet is available)
                if let Some(wallet) = participant_wallet {
                    self.emit_chain_event(ChainEvent::RecordingStarted {
                        room_id: room_id.clone(),
                        participant: wallet,
                    });
                }
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
        // Check if peer already has an active connection to prevent duplicate joins
        {
            let connections = self.connections.read().await;
            if connections.contains_key(&peer_id) {
                tracing::warn!(peer_id = %peer_id, "Peer already connected, ignoring duplicate join");
                return Ok(());
            }
        }

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
            // Get current connections for PLI sending
            let connections = self.connections.read().await;
            let connections_map: std::collections::HashMap<String, Arc<SfuConnection>> =
                connections.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            drop(connections);

            connection
                .add_existing_tracks(self.track_manager.clone(), existing_tracks, &connections_map)
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
        if let Some((room_id, role, peer_name)) = room_info {
            // Get wallet address for this peer
            let peer_wallet = {
                let wallets = self.peer_wallets.read().await;
                wallets.get(peer_id).copied()
            };

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

                    // Emit chain event for recording stopped (only if wallet available)
                    let stopped_wallet = {
                        let wallets = self.peer_wallets.read().await;
                        wallets.get(stopped_peer_id).copied()
                    };
                    if let Some(wallet) = stopped_wallet {
                        self.emit_chain_event(ChainEvent::RecordingStopped {
                            room_id: room_id.clone(),
                            participant: wallet,
                            duration_secs: 0, // Duration not tracked currently
                            ipfs_cid: result.cid.clone(),
                        });
                    }
                }

                // Emit chain event for proctor leaving (only if wallet available)
                if let Some(wallet) = peer_wallet {
                    self.emit_chain_event(ChainEvent::ParticipantLeft {
                        room_id: room_id.clone(),
                        participant: wallet,
                        reason: ChainLeaveReason::Normal,
                    });
                }

                // Get all student connections to close
                let students_to_close: Vec<String> = self.room_manager.get_room_peers(&room_id).await
                    .into_iter()
                    .filter(|p| p.id != peer_id)
                    .map(|p| p.id)
                    .collect();

                // Emit chain events for students being forced to leave
                for student_id in &students_to_close {
                    let student_wallet = {
                        let wallets = self.peer_wallets.read().await;
                        wallets.get(student_id).copied()
                    };
                    if let Some(wallet) = student_wallet {
                        self.emit_chain_event(ChainEvent::ParticipantLeft {
                            room_id: room_id.clone(),
                            participant: wallet,
                            reason: ChainLeaveReason::RoomClosed,
                        });
                    }
                }

                // Emit chain event for room closed
                self.emit_chain_event(ChainEvent::RoomClosed {
                    room_id: room_id.clone(),
                    reason: ChainRoomCloseReason::ProctorLeft,
                });

                // Close all student connections and clean up their wallet mappings
                for student_id in students_to_close {
                    self.close_peer_connection(&student_id).await;
                    let mut wallets = self.peer_wallets.write().await;
                    wallets.remove(&student_id);
                }
            } else {
                // Student left - get their exam grade (if submitted)
                let exam_grade = self.get_exam_grade(peer_id).await;

                // Stop their recording
                if let Ok(result) = self.recording_manager.stop_recording(&room_id, peer_id).await {
                    // Emit chain events (only if wallet available)
                    if let Some(wallet) = peer_wallet {
                        // Get grade and exam name from submitted result, or use defaults
                        let (grade, exam_name) = match &exam_grade {
                            Some(eg) => (eg.grade, eg.exam_name.clone()),
                            None => (0, format!("Exam Session {}", room_id)),
                        };

                        tracing::info!(
                            peer_id = %peer_id,
                            grade = grade,
                            exam_name = %exam_name,
                            "Creating exam result with grade"
                        );

                        // IMPORTANT: CreateExamResult must be emitted BEFORE RecordingStopped
                        // so the contract can link the recording CID to the exam result
                        self.emit_chain_event(ChainEvent::CreateExamResult {
                            room_id: room_id.clone(),
                            participant: wallet,
                            grade,
                            exam_name,
                        });

                        // Now emit RecordingStopped - the contract will add the CID to the exam result
                        self.emit_chain_event(ChainEvent::RecordingStopped {
                            room_id: room_id.clone(),
                            participant: wallet,
                            duration_secs: 0,
                            ipfs_cid: result.cid.clone(),
                        });
                    }
                }

                // Clean up exam grade
                self.remove_exam_grade(peer_id).await;

                tracing::info!(
                    room_id = %room_id,
                    peer_id = %peer_id,
                    "Stopped recording for leaving student"
                );

                // Emit chain event for participant left (only if wallet available)
                if let Some(wallet) = peer_wallet {
                    self.emit_chain_event(ChainEvent::ParticipantLeft {
                        room_id: room_id.clone(),
                        participant: wallet,
                        reason: ChainLeaveReason::Normal,
                    });
                }

                // Notify proctor about participant leaving
                self.update_all_connections_for_peer_removal(peer_id, &room_id, peer_name).await?;
            }

            // Clean up wallet mapping for this peer
            let mut wallets = self.peer_wallets.write().await;
            wallets.remove(peer_id);
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
        // Get source connection for sending PLI
        let source_connection = connections.get(peer_id).cloned();

        for (target_peer_id, connection) in connections.iter() {
            if target_peer_id != peer_id {
                if !self.room_manager.should_forward_track(peer_id, target_peer_id).await {
                    continue;
                }

                if let Some((local_track, is_new, is_video, ssrc, _source_peer_id)) = self
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

                    // Send PLI for new video track subscriptions to get immediate keyframe
                    if is_new && is_video {
                        if let Some(ref src_conn) = source_connection {
                            if let Err(e) = SfuConnection::send_pli(&src_conn.peer_connection, ssrc).await {
                                tracing::warn!(
                                    track_id = %track_id,
                                    error = %e,
                                    "Failed to send PLI for new subscriber"
                                );
                            } else {
                                tracing::info!(
                                    track_id = %track_id,
                                    target_peer_id = %target_peer_id,
                                    "Sent PLI for new subscriber keyframe request"
                                );
                            }
                        }
                    }

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
        room_id: &str,
        peer_name: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::debug!(
            removed_peer_id = %removed_peer_id,
            room_id = %room_id,
            "Notifying proctor about participant leaving"
        );

        // Notify the proctor that a participant has left
        if let Some(proctor_id) = self.room_manager.get_room_proctor(room_id).await {
            let connections = self.connections.read().await;
            if let Some(proctor_connection) = connections.get(&proctor_id) {
                let message = SfuMessage::ParticipantLeft {
                    room_id: room_id.to_string(),
                    peer_id: removed_peer_id.to_string(),
                    name: peer_name,
                };

                if let Ok(message_str) = serde_json::to_string(&message) {
                    if let Err(e) = proctor_connection.send_message(Message::text(message_str)).await {
                        tracing::error!(error = %e, "Failed to send ParticipantLeft to proctor");
                    } else {
                        tracing::info!(
                            room_id = %room_id,
                            peer_id = %removed_peer_id,
                            "Notified proctor about participant leaving"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn forward_join_request(
        &self,
        room_id: String,
        student_peer_id: String,
        student_name: Option<String>,
        role: String,
        wallet_address: Option<String>,
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
                    wallet_address,
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
        wallet_address: Option<String>,
        sender: mpsc::UnboundedSender<Message>,
    ) {
        let mut pending = self.pending_students.write().await;
        pending.insert(student_peer_id, PendingStudent { sender, wallet_address });
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
        if let Some(pending_student) = pending.get(&student_peer_id) {
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
            pending_student.sender.send(Message::text(message_str))?;

            return Ok(());
        }

        Err("Student connection not found".into())
    }


    pub async fn remove_pending_student(&self, student_peer_id: &str) {
        let mut pending = self.pending_students.write().await;
        pending.remove(student_peer_id);
    }

    /// Store exam grade for a peer (called when student submits exam)
    pub async fn set_exam_grade(&self, peer_id: &str, grade: u64, exam_name: String) {
        let mut grades = self.peer_exam_grades.write().await;
        grades.insert(peer_id.to_string(), ExamGrade { grade, exam_name });
        tracing::info!(peer_id = %peer_id, grade = grade, "Stored exam grade for peer");
    }

    /// Get exam grade for a peer (returns grade in basis points, e.g., 8500 = 85.00%)
    pub async fn get_exam_grade(&self, peer_id: &str) -> Option<ExamGrade> {
        let grades = self.peer_exam_grades.read().await;
        grades.get(peer_id).cloned()
    }

    /// Remove exam grade for a peer
    pub async fn remove_exam_grade(&self, peer_id: &str) {
        let mut grades = self.peer_exam_grades.write().await;
        grades.remove(peer_id);
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

    // Chain event emission methods

    /// Emits a participant kicked event to the blockchain
    pub async fn emit_participant_kicked(
        &self,
        room_id: &str,
        kicked_peer_id: &str,
        reason: Option<String>,
    ) {
        // Get wallet addresses for proctor and kicked participant
        let wallets = self.peer_wallets.read().await;
        let proctor_id = self.room_manager.get_room_proctor(room_id).await;
        let proctor_wallet = proctor_id.as_ref().and_then(|id| wallets.get(id).copied());
        let kicked_wallet = wallets.get(kicked_peer_id).copied();

        if let (Some(proctor), Some(kicked)) = (proctor_wallet, kicked_wallet) {
            self.emit_chain_event(ChainEvent::ParticipantKicked {
                room_id: room_id.to_string(),
                proctor,
                kicked,
                reason,
            });
        } else {
            tracing::debug!(
                room_id = %room_id,
                kicked_peer_id = %kicked_peer_id,
                "Cannot emit participant kicked event: wallet addresses not available"
            );
        }
    }

    /// Emits an ID verification event to the blockchain
    pub async fn emit_id_verification(
        &self,
        room_id: &str,
        peer_id: &str,
        status: &str,
        verified_by: &str,
    ) {
        let verification_status = match status.to_lowercase().as_str() {
            "valid" => ChainVerificationStatus::Valid,
            "invalid" => ChainVerificationStatus::Invalid,
            "pending" => ChainVerificationStatus::Pending,
            "skipped" => ChainVerificationStatus::Skipped,
            _ => ChainVerificationStatus::Pending,
        };

        let wallets = self.peer_wallets.read().await;
        if let Some(wallet) = wallets.get(peer_id).copied() {
            self.emit_chain_event(ChainEvent::IdVerification {
                room_id: room_id.to_string(),
                participant: wallet,
                status: verification_status,
                verified_by: verified_by.to_string(),
            });
        } else {
            tracing::debug!(
                room_id = %room_id,
                peer_id = %peer_id,
                "Cannot emit ID verification event: wallet address not available"
            );
        }
    }

    /// Emits a suspicious activity event to the blockchain
    pub async fn emit_suspicious_activity(
        &self,
        room_id: &str,
        peer_id: &str,
        activity_type: &str,
        details: Option<String>,
    ) {
        let suspicious_type = match activity_type.to_lowercase().as_str() {
            "multiple_devices" => ChainSuspiciousActivityType::MultipleDevices,
            "tab_switch" => ChainSuspiciousActivityType::TabSwitch,
            "window_blur" => ChainSuspiciousActivityType::WindowBlur,
            "screen_share" => ChainSuspiciousActivityType::ScreenShare,
            "unauthorized_person" => ChainSuspiciousActivityType::UnauthorizedPerson,
            "audio_anomaly" => ChainSuspiciousActivityType::AudioAnomaly,
            _ => ChainSuspiciousActivityType::Other,
        };

        let wallets = self.peer_wallets.read().await;
        if let Some(wallet) = wallets.get(peer_id).copied() {
            self.emit_chain_event(ChainEvent::SuspiciousActivity {
                room_id: room_id.to_string(),
                participant: wallet,
                activity_type: suspicious_type,
                details,
            });
        } else {
            tracing::debug!(
                room_id = %room_id,
                peer_id = %peer_id,
                "Cannot emit suspicious activity event: wallet address not available"
            );
        }
    }

    // Signaling helper methods

    /// Sends a kick notification to a participant
    pub async fn send_kick_notification(
        &self,
        room_id: &str,
        peer_id: &str,
        reason: Option<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connections = self.connections.read().await;
        if let Some(connection) = connections.get(peer_id) {
            let message = SfuMessage::ParticipantKicked {
                room_id: room_id.to_string(),
                peer_id: peer_id.to_string(),
                reason,
            };
            let message_str = serde_json::to_string(&message)?;
            connection.send_message(Message::text(message_str)).await?;
        }
        Ok(())
    }

    /// Sends a verification request to a participant
    pub async fn send_verification_request(
        &self,
        room_id: &str,
        peer_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connections = self.connections.read().await;
        if let Some(connection) = connections.get(peer_id) {
            let message = SfuMessage::StartIdVerification {
                room_id: room_id.to_string(),
                peer_id: peer_id.to_string(),
            };
            let message_str = serde_json::to_string(&message)?;
            connection.send_message(Message::text(message_str)).await?;
        }
        Ok(())
    }

    /// Sends a verification result to a participant
    pub async fn send_verification_result(
        &self,
        room_id: &str,
        peer_id: &str,
        status: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connections = self.connections.read().await;
        if let Some(connection) = connections.get(peer_id) {
            let message = serde_json::json!({
                "type": "id_verification_status",
                "room_id": room_id,
                "peer_id": peer_id,
                "status": status
            });
            let message_str = serde_json::to_string(&message)?;
            connection.send_message(Message::text(message_str)).await?;
        }
        Ok(())
    }
}