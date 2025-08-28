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