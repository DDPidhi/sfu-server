use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use warp::ws::Message;

use super::server::SfuServer;

/// Recording info for stopped recordings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingInfo {
    pub peer_id: String,
    pub file_path: Option<String>,
    pub cid: Option<String>,
    pub ipfs_gateway_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SfuMessage {

    CreateRoom {
        peer_id: String,
        name: Option<String>,
        /// Wallet address of the proctor (for on-chain recording)
        wallet_address: Option<String>,
    },

    RoomCreated {
        room_id: String,
    },

    JoinRequest {
        room_id: String,
        peer_id: String,
        name: Option<String>,
        role: String,
        /// Wallet address of the participant (for on-chain recording and NFT generation)
        wallet_address: Option<String>,
    },

    JoinResponse {
        room_id: String,
        peer_id: String,
        approved: bool,
        requester_peer_id: String,
    },

    Join {
        room_id: String,
        peer_id: String,
        name: Option<String>,
        role: String,
        /// Wallet address of the participant (for on-chain recording and NFT generation)
        wallet_address: Option<String>,
    },

    Leave {
        peer_id: String,
    },

    Offer {
        sdp: String,
    },

    Answer {
        peer_id: String,
        sdp: String,
    },

    IceCandidate {
        peer_id: String,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
    },

    Renegotiate {
        sdp: String,
    },

    MediaReady {
        peer_id: String,
        has_video: bool,
        has_audio: bool,
    },

    // Recording messages
    StartRecording {
        room_id: String,
        peer_id: String,
    },

    StopRecording {
        room_id: String,
        peer_id: String,
    },

    StopAllRecordings {
        room_id: String,
    },

    RecordingStarted {
        room_id: String,
        peer_id: String,
    },

    RecordingStopped {
        room_id: String,
        peer_id: String,
        file_path: Option<String>,
        cid: Option<String>,
        ipfs_gateway_url: Option<String>,
    },

    AllRecordingsStopped {
        room_id: String,
        recordings: Vec<RecordingInfo>,
    },

    RecordingError {
        room_id: String,
        peer_id: Option<String>,
        error: String,
    },

    GetRecordingStatus {
        room_id: String,
    },

    RecordingStatus {
        room_id: String,
        recording_peers: Vec<String>,
    },

    // Proctor action messages
    KickParticipant {
        room_id: String,
        peer_id: String,
        reason: Option<String>,
    },

    ParticipantKicked {
        room_id: String,
        peer_id: String,
        reason: Option<String>,
    },

    /// Sent to proctor when a participant voluntarily leaves the room
    ParticipantLeft {
        room_id: String,
        peer_id: String,
        name: Option<String>,
    },

    // ID verification messages
    StartIdVerification {
        room_id: String,
        peer_id: String,
    },

    IdVerificationResult {
        room_id: String,
        peer_id: String,
        status: String, // "valid", "invalid", "pending", "skipped"
        verified_by: String,
    },

    // Suspicious activity messages
    ReportSuspiciousActivity {
        room_id: String,
        peer_id: String,
        activity_type: String, // "multiple_devices", "tab_switch", "window_blur", "screen_share", "unauthorized_person", "audio_anomaly", "other"
        details: Option<String>,
    },

    SuspiciousActivityReported {
        room_id: String,
        peer_id: String,
        activity_type: String,
    },

    /// Sent by student when they complete an exam with their score
    SubmitExamResult {
        room_id: String,
        peer_id: String,
        score: u64,      // Score achieved (e.g., 85 for 85%)
        total: u64,      // Total possible (e.g., 100)
        exam_name: Option<String>,
    },

    /// Confirmation sent back to student after exam result is recorded
    ExamResultSubmitted {
        room_id: String,
        peer_id: String,
        grade: u64,      // Grade in basis points (8500 = 85.00%)
    },
}

pub struct SfuSignalingHandler {
    sfu_server: Arc<SfuServer>,
    peer_id: Option<String>,
    room_id: Option<String>,
    sender: mpsc::UnboundedSender<Message>,
}

impl SfuSignalingHandler {
    pub fn new(
        sfu_server: Arc<SfuServer>,
        sender: mpsc::UnboundedSender<Message>,
    ) -> Self {
        Self {
            sfu_server,
            peer_id: None,
            room_id: None,
            sender,
        }
    }

    pub async fn handle_message(&mut self, message: SfuMessage) {
        match message {
            SfuMessage::CreateRoom { peer_id, name, wallet_address } => {
                self.handle_create_room(peer_id, name, wallet_address).await;
            }
            SfuMessage::Join { room_id, peer_id, name, role, wallet_address } => {
                self.handle_join(room_id, peer_id, name, role, wallet_address).await;
            }
            SfuMessage::JoinRequest { room_id, peer_id, name, role, wallet_address } => {
                self.handle_join_request(room_id, peer_id, name, role, wallet_address).await;
            }
            SfuMessage::JoinResponse { room_id, peer_id, approved, requester_peer_id } => {
                self.handle_join_response(room_id, peer_id, approved, requester_peer_id).await;
            }
            SfuMessage::Leave { peer_id } => {
                self.handle_leave(peer_id).await;
            }
            SfuMessage::Answer { peer_id, sdp } => {
                self.handle_answer(peer_id, sdp).await;
            }
            SfuMessage::IceCandidate {
                peer_id,
                candidate,
                sdp_mid,
                sdp_mline_index,
            } => {
                self.handle_ice_candidate(peer_id, candidate, sdp_mid, sdp_mline_index).await;
            }
            SfuMessage::MediaReady { peer_id, has_video, has_audio } => {
                self.handle_media_ready(peer_id, has_video, has_audio).await;
            }
            SfuMessage::StartRecording { room_id, peer_id } => {
                self.handle_start_recording(room_id, peer_id).await;
            }
            SfuMessage::StopRecording { room_id, peer_id } => {
                self.handle_stop_recording(room_id, peer_id).await;
            }
            SfuMessage::StopAllRecordings { room_id } => {
                self.handle_stop_all_recordings(room_id).await;
            }
            SfuMessage::GetRecordingStatus { room_id } => {
                self.handle_get_recording_status(room_id).await;
            }
            SfuMessage::KickParticipant { room_id, peer_id, reason } => {
                self.handle_kick_participant(room_id, peer_id, reason).await;
            }
            SfuMessage::StartIdVerification { room_id, peer_id } => {
                self.handle_start_id_verification(room_id, peer_id).await;
            }
            SfuMessage::IdVerificationResult { room_id, peer_id, status, verified_by } => {
                self.handle_id_verification_result(room_id, peer_id, status, verified_by).await;
            }
            SfuMessage::ReportSuspiciousActivity { room_id, peer_id, activity_type, details } => {
                self.handle_report_suspicious_activity(room_id, peer_id, activity_type, details).await;
            }
            SfuMessage::SubmitExamResult { room_id, peer_id, score, total, exam_name } => {
                self.handle_submit_exam_result(room_id, peer_id, score, total, exam_name).await;
            }
            _ => {
                tracing::warn!("Unhandled SFU message type");
            }
        }
    }

    async fn handle_create_room(&mut self, peer_id: String, name: Option<String>, wallet_address: Option<String>) {
        tracing::info!(peer_id = %peer_id, name = ?name, wallet = ?wallet_address, "Proctor creating room");

        match self.sfu_server.create_room(peer_id.clone(), name, wallet_address).await {
            Ok(room_id) => {
                self.peer_id = Some(peer_id.clone());
                self.room_id = Some(room_id.clone());

                let message = SfuMessage::RoomCreated { room_id: room_id.clone() };
                if let Ok(msg_str) = serde_json::to_string(&message) {
                    tracing::debug!(room_id = %room_id, "Sending RoomCreated message");
                    let _ = self.sender.send(Message::text(msg_str));
                } else {
                    tracing::error!("Failed to serialize RoomCreated message");
                }

                if let Err(e) = self.sfu_server.add_peer(peer_id, room_id, self.sender.clone()).await {
                    tracing::error!(error = %e, "Failed to add proctor to SFU");
                    self.send_error(&format!("Failed to setup room: {}", e)).await;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to create room");
                self.send_error(&format!("Failed to create room: {}", e)).await;
            }
        }
    }

    async fn handle_join(&mut self, room_id: String, peer_id: String, name: Option<String>, role: String, wallet_address: Option<String>) {
        tracing::info!(
            role = %role,
            peer_id = %peer_id,
            room_id = %room_id,
            name = ?name,
            wallet = ?wallet_address,
            "Peer joining room"
        );

        self.peer_id = Some(peer_id.clone());
        self.room_id = Some(room_id.clone());

        self.sfu_server.remove_pending_student(&peer_id).await;

        // Add peer to SFU with role and wallet address
        if let Err(e) = self.sfu_server.add_peer_with_role(peer_id.clone(), room_id, role, name, wallet_address, self.sender.clone()).await {
            tracing::error!(peer_id = %peer_id, error = %e, "Failed to add peer to SFU");
            self.send_error(&format!("Failed to join: {}", e)).await;
        } else {
            self.send_join_success().await;
        }
    }

    async fn handle_join_request(&mut self, room_id: String, peer_id: String, name: Option<String>, role: String, wallet_address: Option<String>) {
        tracing::info!(
            peer_id = %peer_id,
            room_id = %room_id,
            name = ?name,
            wallet = ?wallet_address,
            "Student requesting to join room"
        );

        self.peer_id = Some(peer_id.clone());
        self.room_id = Some(room_id.clone());

        self.sfu_server.track_pending_student(peer_id.clone(), wallet_address.clone(), self.sender.clone()).await;

        // Forward the join request to the proctor (but don't add connection to SFU yet)
        if let Err(e) = self.sfu_server.forward_join_request(room_id, peer_id, name, role, wallet_address).await {
            tracing::error!(error = %e, "Failed to forward join request");
            self.send_error(&format!("Failed to send join request: {}", e)).await;
        } else {
            tracing::debug!("Join request forwarded to proctor");
            self.send_join_request_sent().await;
        }
    }

    async fn handle_join_response(&mut self, room_id: String, peer_id: String, approved: bool, requester_peer_id: String) {
        tracing::info!(
            proctor_id = %peer_id,
            requester_peer_id = %requester_peer_id,
            room_id = %room_id,
            approved = approved,
            "Proctor responded to join request"
        );

        if let Err(e) = self.sfu_server.send_join_response(room_id, requester_peer_id, approved).await {
            tracing::error!(error = %e, "Failed to send join response");
            self.send_error(&format!("Failed to send join response: {}", e)).await;
        }
    }

    async fn handle_leave(&mut self, peer_id: String) {
        tracing::info!(peer_id = %peer_id, "Client leaving");

        if let Err(e) = self.sfu_server.remove_peer(&peer_id).await {
            tracing::error!(peer_id = %peer_id, error = %e, "Failed to remove peer from SFU");
        }

        self.peer_id = None;
        self.room_id = None;
    }

    async fn handle_answer(&self, peer_id: String, sdp: String) {
        tracing::info!(peer_id = %peer_id, "Received answer from client");

        if let Err(e) = self.sfu_server.handle_answer(&peer_id, &sdp).await {
            tracing::error!(peer_id = %peer_id, error = %e, "Failed to handle answer");
            self.send_error(&format!("Failed to process answer: {}", e)).await;
        } else {
            tracing::debug!(peer_id = %peer_id, "Successfully processed answer");
        }
    }

    async fn handle_ice_candidate(
        &self,
        peer_id: String,
        candidate: String,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
    ) {
        if let Err(e) = self.sfu_server
            .handle_ice_candidate(&peer_id, &candidate, sdp_mid, sdp_mline_index)
            .await
        {
            tracing::error!(peer_id = %peer_id, error = %e, "Failed to handle ICE candidate");
        }
    }

    async fn handle_media_ready(&self, peer_id: String, has_video: bool, has_audio: bool) {
        tracing::info!(
            peer_id = %peer_id,
            has_video = has_video,
            has_audio = has_audio,
            "Client media ready"
        );
    }

    async fn handle_start_recording(&self, room_id: String, peer_id: String) {
        tracing::info!(room_id = %room_id, peer_id = %peer_id, "Starting recording for peer");

        match self.sfu_server.start_recording(&room_id, &peer_id).await {
            Ok(()) => {
                let message = SfuMessage::RecordingStarted {
                    room_id,
                    peer_id,
                };
                if let Ok(msg_str) = serde_json::to_string(&message) {
                    let _ = self.sender.send(Message::text(msg_str));
                }
            }
            Err(e) => {
                tracing::error!(room_id = %room_id, peer_id = %peer_id, error = %e, "Failed to start recording");
                let message = SfuMessage::RecordingError {
                    room_id,
                    peer_id: Some(peer_id),
                    error: e.to_string(),
                };
                if let Ok(msg_str) = serde_json::to_string(&message) {
                    let _ = self.sender.send(Message::text(msg_str));
                }
            }
        }
    }

    async fn handle_stop_recording(&self, room_id: String, peer_id: String) {
        tracing::info!(room_id = %room_id, peer_id = %peer_id, "Stopping recording for peer");

        match self.sfu_server.stop_recording(&room_id, &peer_id).await {
            Ok(result) => {
                let message = SfuMessage::RecordingStopped {
                    room_id,
                    peer_id,
                    file_path: Some(result.file_path.to_string_lossy().to_string()),
                    cid: result.cid,
                    ipfs_gateway_url: result.ipfs_gateway_url,
                };
                if let Ok(msg_str) = serde_json::to_string(&message) {
                    let _ = self.sender.send(Message::text(msg_str));
                }
            }
            Err(e) => {
                tracing::error!(room_id = %room_id, peer_id = %peer_id, error = %e, "Failed to stop recording");
                let message = SfuMessage::RecordingError {
                    room_id,
                    peer_id: Some(peer_id),
                    error: e.to_string(),
                };
                if let Ok(msg_str) = serde_json::to_string(&message) {
                    let _ = self.sender.send(Message::text(msg_str));
                }
            }
        }
    }

    async fn handle_stop_all_recordings(&self, room_id: String) {
        tracing::info!(room_id = %room_id, "Stopping all recordings in room");

        let stopped = self.sfu_server.stop_all_recordings(&room_id).await;
        let recordings: Vec<RecordingInfo> = stopped
            .into_iter()
            .map(|(peer_id, result)| RecordingInfo {
                peer_id,
                file_path: Some(result.file_path.to_string_lossy().to_string()),
                cid: result.cid,
                ipfs_gateway_url: result.ipfs_gateway_url,
            })
            .collect();

        let message = SfuMessage::AllRecordingsStopped {
            room_id,
            recordings,
        };
        if let Ok(msg_str) = serde_json::to_string(&message) {
            let _ = self.sender.send(Message::text(msg_str));
        }
    }

    async fn handle_get_recording_status(&self, room_id: String) {
        tracing::debug!(room_id = %room_id, "Getting recording status");

        let recording_peers = self.sfu_server.get_recording_peers(&room_id).await;
        let message = SfuMessage::RecordingStatus {
            room_id,
            recording_peers,
        };
        if let Ok(msg_str) = serde_json::to_string(&message) {
            let _ = self.sender.send(Message::text(msg_str));
        }
    }

    async fn handle_kick_participant(&self, room_id: String, peer_id: String, reason: Option<String>) {
        tracing::info!(
            room_id = %room_id,
            peer_id = %peer_id,
            reason = ?reason,
            "Proctor kicking participant"
        );

        // Notify the kicked participant
        if let Err(e) = self.sfu_server.send_kick_notification(&room_id, &peer_id, reason.clone()).await {
            tracing::error!(
                room_id = %room_id,
                peer_id = %peer_id,
                error = %e,
                "Failed to send kick notification"
            );
        }

        // Remove the participant from the room
        if let Err(e) = self.sfu_server.remove_peer(&peer_id).await {
            tracing::error!(
                peer_id = %peer_id,
                error = %e,
                "Failed to remove kicked peer"
            );
        }

        // Emit chain event for the kick (handled by SfuServer)
        self.sfu_server.emit_participant_kicked(&room_id, &peer_id, reason).await;
    }

    async fn handle_start_id_verification(&self, room_id: String, peer_id: String) {
        tracing::info!(
            room_id = %room_id,
            peer_id = %peer_id,
            "Starting ID verification"
        );

        // Forward the verification request to the participant
        if let Err(e) = self.sfu_server.send_verification_request(&room_id, &peer_id).await {
            tracing::error!(
                room_id = %room_id,
                peer_id = %peer_id,
                error = %e,
                "Failed to send verification request"
            );
        }
    }

    async fn handle_id_verification_result(
        &self,
        room_id: String,
        peer_id: String,
        status: String,
        verified_by: String,
    ) {
        tracing::info!(
            room_id = %room_id,
            peer_id = %peer_id,
            status = %status,
            verified_by = %verified_by,
            "ID verification result"
        );

        // Emit chain event for verification
        self.sfu_server.emit_id_verification(&room_id, &peer_id, &status, &verified_by).await;

        // Notify the participant of verification result
        if let Err(e) = self.sfu_server.send_verification_result(&room_id, &peer_id, &status).await {
            tracing::error!(
                room_id = %room_id,
                peer_id = %peer_id,
                error = %e,
                "Failed to send verification result"
            );
        }
    }

    async fn handle_report_suspicious_activity(
        &self,
        room_id: String,
        peer_id: String,
        activity_type: String,
        details: Option<String>,
    ) {
        tracing::warn!(
            room_id = %room_id,
            peer_id = %peer_id,
            activity_type = %activity_type,
            details = ?details,
            "Suspicious activity reported"
        );

        // Emit chain event for suspicious activity
        self.sfu_server.emit_suspicious_activity(&room_id, &peer_id, &activity_type, details.clone()).await;

        // Acknowledge the report
        let message = SfuMessage::SuspiciousActivityReported {
            room_id,
            peer_id,
            activity_type,
        };
        if let Ok(msg_str) = serde_json::to_string(&message) {
            let _ = self.sender.send(Message::text(msg_str));
        }
    }

    async fn handle_submit_exam_result(
        &self,
        room_id: String,
        peer_id: String,
        score: u64,
        total: u64,
        exam_name: Option<String>,
    ) {
        // Calculate grade in basis points (e.g., 85% = 8500)
        let grade = if total > 0 {
            (score * 10000) / total
        } else {
            0
        };

        let exam_name = exam_name.unwrap_or_else(|| format!("Exam Session {}", room_id));

        tracing::info!(
            room_id = %room_id,
            peer_id = %peer_id,
            score = score,
            total = total,
            grade = grade,
            exam_name = %exam_name,
            "Student submitted exam result"
        );

        // Store the grade for when the student leaves
        self.sfu_server.set_exam_grade(&peer_id, grade, exam_name).await;

        // Send confirmation back to student
        let message = SfuMessage::ExamResultSubmitted {
            room_id,
            peer_id,
            grade,
        };

        if let Ok(msg_str) = serde_json::to_string(&message) {
            let _ = self.sender.send(Message::text(msg_str));
        }
    }

    async fn send_join_success(&self) {
        let message = serde_json::json!({
            "type": "join_success",
            "message": "Successfully connected to SFU"
        });

        if let Ok(msg_str) = serde_json::to_string(&message) {
            let _ = self.sender.send(Message::text(msg_str));
        }
    }

    async fn send_join_request_sent(&self) {
        let message = serde_json::json!({
            "type": "join_request_sent",
            "message": "Join request sent to proctor. Waiting for approval..."
        });

        if let Ok(msg_str) = serde_json::to_string(&message) {
            let _ = self.sender.send(Message::text(msg_str));
        }
    }

    async fn send_error(&self, error: &str) {
        let message = serde_json::json!({
            "type": "error",
            "message": error
        });

        if let Ok(msg_str) = serde_json::to_string(&message) {
            let _ = self.sender.send(Message::text(msg_str));
        }
    }

    pub async fn cleanup(&mut self) {
        if let Some(peer_id) = &self.peer_id {
            let _ = self.sfu_server.remove_peer(peer_id).await;
            self.sfu_server.remove_pending_student(peer_id).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_create_room() {
        let msg = SfuMessage::CreateRoom {
            peer_id: "proctor_123".to_string(),
            name: Some("Dr. Smith".to_string()),
            wallet_address: Some("0x1234567890abcdef1234567890abcdef12345678".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("CreateRoom"));
        assert!(json.contains("proctor_123"));
        assert!(json.contains("Dr. Smith"));
        assert!(json.contains("0x1234567890abcdef"));
    }

    #[test]
    fn test_deserialize_create_room() {
        let json = r#"{"type":"CreateRoom","peer_id":"proctor_123","name":"Dr. Smith","wallet_address":"0x1234"}"#;
        let msg: SfuMessage = serde_json::from_str(json).unwrap();

        match msg {
            SfuMessage::CreateRoom { peer_id, name, wallet_address } => {
                assert_eq!(peer_id, "proctor_123");
                assert_eq!(name, Some("Dr. Smith".to_string()));
                assert_eq!(wallet_address, Some("0x1234".to_string()));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_serialize_join() {
        let msg = SfuMessage::Join {
            room_id: "123456".to_string(),
            peer_id: "student_789".to_string(),
            name: Some("John Doe".to_string()),
            role: "student".to_string(),
            wallet_address: Some("0xabcdef".to_string()),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Join"));
        assert!(json.contains("123456"));
        assert!(json.contains("student_789"));
        assert!(json.contains("0xabcdef"));
    }

    #[test]
    fn test_deserialize_join() {
        let json = r#"{"type":"Join","room_id":"123456","peer_id":"student_789","name":"John Doe","role":"student","wallet_address":"0xabcdef"}"#;
        let msg: SfuMessage = serde_json::from_str(json).unwrap();

        match msg {
            SfuMessage::Join { room_id, peer_id, name, role, wallet_address } => {
                assert_eq!(room_id, "123456");
                assert_eq!(peer_id, "student_789");
                assert_eq!(name, Some("John Doe".to_string()));
                assert_eq!(role, "student");
                assert_eq!(wallet_address, Some("0xabcdef".to_string()));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_serialize_offer() {
        let msg = SfuMessage::Offer {
            sdp: "v=0\r\no=- 123".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Offer"));
        assert!(json.contains("v=0"));
    }

    #[test]
    fn test_deserialize_answer() {
        let json = r#"{"type":"Answer","peer_id":"peer_123","sdp":"v=0\r\no=- 456"}"#;
        let msg: SfuMessage = serde_json::from_str(json).unwrap();

        match msg {
            SfuMessage::Answer { peer_id, sdp } => {
                assert_eq!(peer_id, "peer_123");
                assert!(sdp.contains("v=0"));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_serialize_ice_candidate() {
        let msg = SfuMessage::IceCandidate {
            peer_id: "peer_123".to_string(),
            candidate: "candidate:0 1 UDP 123456 192.168.1.1 54321 typ host".to_string(),
            sdp_mid: Some("0".to_string()),
            sdp_mline_index: Some(0),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("IceCandidate"));
        assert!(json.contains("peer_123"));
        assert!(json.contains("candidate"));
    }

    #[test]
    fn test_deserialize_ice_candidate() {
        let json = r#"{"type":"IceCandidate","peer_id":"peer_123","candidate":"candidate:0 1 UDP 123456 192.168.1.1 54321 typ host","sdp_mid":"0","sdp_mline_index":0}"#;
        let msg: SfuMessage = serde_json::from_str(json).unwrap();

        match msg {
            SfuMessage::IceCandidate { peer_id, candidate, sdp_mid, sdp_mline_index } => {
                assert_eq!(peer_id, "peer_123");
                assert!(candidate.contains("candidate:"));
                assert_eq!(sdp_mid, Some("0".to_string()));
                assert_eq!(sdp_mline_index, Some(0));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_serialize_media_ready() {
        let msg = SfuMessage::MediaReady {
            peer_id: "peer_123".to_string(),
            has_video: true,
            has_audio: true,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("MediaReady"));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_serialize_leave() {
        let msg = SfuMessage::Leave {
            peer_id: "peer_123".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("Leave"));
        assert!(json.contains("peer_123"));
    }

    #[test]
    fn test_serialize_room_created() {
        let msg = SfuMessage::RoomCreated {
            room_id: "123456".to_string(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("RoomCreated"));
        assert!(json.contains("123456"));
    }
}