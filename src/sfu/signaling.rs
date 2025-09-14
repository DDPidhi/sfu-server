use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use warp::ws::Message;

use super::server::SfuServer;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SfuMessage {

    CreateRoom {
        peer_id: String,
        name: Option<String>,
    },

    RoomCreated {
        room_id: String,
    },

    JoinRequest {
        room_id: String,
        peer_id: String,
        name: Option<String>,
        role: String,
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
            SfuMessage::CreateRoom { peer_id, name } => {
                self.handle_create_room(peer_id, name).await;
            }
            SfuMessage::Join { room_id, peer_id, name, role } => {
                self.handle_join(room_id, peer_id, name, role).await;
            }
            SfuMessage::JoinRequest { room_id, peer_id, name, role } => {
                self.handle_join_request(room_id, peer_id, name, role).await;
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
            _ => {
                println!("Unhandled SFU message type");
            }
        }
    }

    async fn handle_create_room(&mut self, peer_id: String, name: Option<String>) {
        println!("Proctor {} creating room", peer_id);

        match self.sfu_server.create_room(peer_id.clone(), name).await {
            Ok(room_id) => {
                self.peer_id = Some(peer_id.clone());
                self.room_id = Some(room_id.clone());

                let message = SfuMessage::RoomCreated { room_id: room_id.clone() };
                if let Ok(msg_str) = serde_json::to_string(&message) {
                    println!("Sending RoomCreated message: {}", msg_str);
                    let _ = self.sender.send(Message::text(msg_str));
                } else {
                    println!("Failed to serialize RoomCreated message");
                }

                if let Err(e) = self.sfu_server.add_peer(peer_id, room_id, self.sender.clone()).await {
                    println!("Failed to add proctor to SFU: {}", e);
                    self.send_error(&format!("Failed to setup room: {}", e)).await;
                }
            }
            Err(e) => {
                println!("Failed to create room: {}", e);
                self.send_error(&format!("Failed to create room: {}", e)).await;
            }
        }
    }

    async fn handle_join(&mut self, room_id: String, peer_id: String, name: Option<String>, role: String) {
        println!("{} {} joining room {}", role, peer_id, room_id);

        self.peer_id = Some(peer_id.clone());
        self.room_id = Some(room_id.clone());

        self.sfu_server.remove_pending_student(&peer_id).await;

        // Add peer to SFU with role
        if let Err(e) = self.sfu_server.add_peer_with_role(peer_id.clone(), room_id, role, name, self.sender.clone()).await {
            println!("Failed to add peer to SFU: {}", e);
            self.send_error(&format!("Failed to join: {}", e)).await;
        } else {
            self.send_join_success().await;
        }
    }

    async fn handle_join_request(&mut self, room_id: String, peer_id: String, name: Option<String>, role: String) {
        println!("Student {} requesting to join room {}", peer_id, room_id);

        self.peer_id = Some(peer_id.clone());
        self.room_id = Some(room_id.clone());

        self.sfu_server.track_pending_student(peer_id.clone(), self.sender.clone()).await;

        // Forward the join request to the proctor (but don't add connection to SFU yet)
        if let Err(e) = self.sfu_server.forward_join_request(room_id, peer_id, name, role).await {
            println!("Failed to forward join request: {}", e);
            self.send_error(&format!("Failed to send join request: {}", e)).await;
        } else {
            println!("Join request forwarded to proctor");
            self.send_join_request_sent().await;
        }
    }

    async fn handle_join_response(&mut self, room_id: String, peer_id: String, approved: bool, requester_peer_id: String) {
        println!("Proctor {} responded to join request from {} in room {}: {}",
                 peer_id, requester_peer_id, room_id, if approved { "APPROVED" } else { "DENIED" });

        if let Err(e) = self.sfu_server.send_join_response(room_id, requester_peer_id, approved).await {
            println!("Failed to send join response: {}", e);
            self.send_error(&format!("Failed to send join response: {}", e)).await;
        }
    }

    async fn handle_leave(&mut self, peer_id: String) {
        println!("Client {} leaving", peer_id);

        if let Err(e) = self.sfu_server.remove_peer(&peer_id).await {
            println!("Failed to remove peer from SFU: {}", e);
        }

        self.peer_id = None;
        self.room_id = None;
    }

    async fn handle_answer(&self, peer_id: String, sdp: String) {
        println!("Received answer from client: {}", peer_id);

        if let Err(e) = self.sfu_server.handle_answer(&peer_id, &sdp).await {
            println!("Failed to handle answer: {}", e);
            self.send_error(&format!("Failed to process answer: {}", e)).await;
        } else {
            println!("Successfully processed answer from: {}", peer_id);
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
            println!("Failed to handle ICE candidate: {}", e);
        }
    }

    async fn handle_media_ready(&self, peer_id: String, has_video: bool, has_audio: bool) {
        println!("Client {} media ready (video: {}, audio: {})", peer_id, has_video, has_audio);
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