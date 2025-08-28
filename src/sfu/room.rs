
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerRole {
    Proctor,
    Student,
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub role: PeerRole,
    pub room_id: String,
    pub name: Option<String>,
}
#[derive(Debug, Clone)]
pub struct Room {
    pub id: String,
    pub proctor_id: String,
    pub students: Vec<String>,
    pub created_at: std::time::SystemTime,
}

pub struct RoomManager {
    rooms: Arc<RwLock<HashMap<String, Room>>>,
    peers: Arc<RwLock<HashMap<String, Peer>>>,
}

impl RoomManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            rooms: Arc::new(RwLock::new(HashMap::new())),
            peers: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}
