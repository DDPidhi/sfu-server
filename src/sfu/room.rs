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

    /// Generate a random room ID
    fn generate_room_id() -> String {
        let mut rng = rand::thread_rng();
        format!("{:06}", rng.gen_range(100000..999999))
    }

    /// Create a new room with a proctor
    pub async fn create_room(&self, proctor_id: String, proctor_name: Option<String>) -> Result<String, String> {
        let room_id = Self::generate_room_id();

        let room = Room {
            id: room_id.clone(),
            proctor_id: proctor_id.clone(),
            students: Vec::new(),
            created_at: std::time::SystemTime::now(),
        };

        let peer = Peer {
            id: proctor_id.clone(),
            role: PeerRole::Proctor,
            room_id: room_id.clone(),
            name: proctor_name,
        };

        let mut rooms = self.rooms.write().await;
        let mut peers = self.peers.write().await;

        // Check if room ID already exists (unlikely but possible)
        if rooms.contains_key(&room_id) {
            return Err("Room ID collision, please try again".to_string());
        }

        rooms.insert(room_id.clone(), room);
        peers.insert(proctor_id, peer);

        tracing::info!(room_id = %room_id, "Room created by proctor");
        Ok(room_id)
    }

    /// Join an existing room as a student
    pub async fn join_room(&self, room_id: String, student_id: String, student_name: Option<String>) -> Result<(), String> {
        let mut rooms = self.rooms.write().await;
        let mut peers = self.peers.write().await;

        let room = rooms.get_mut(&room_id)
            .ok_or_else(|| format!("Room {} does not exist", room_id))?;

        // Check if student is already in the room
        if room.students.contains(&student_id) {
            return Ok(()); // Already in room
        }

        room.students.push(student_id.clone());

        let peer = Peer {
            id: student_id.clone(),
            role: PeerRole::Student,
            room_id: room_id.clone(),
            name: student_name,
        };

        peers.insert(student_id.clone(), peer);

        tracing::info!(student_id = %student_id, room_id = %room_id, "Student joined room");
        Ok(())
    }

    /// Get peer information
    pub async fn get_peer(&self, peer_id: &str) -> Option<Peer> {
        let peers = self.peers.read().await;
        peers.get(peer_id).cloned()
    }

    /// Get room information
    pub async fn get_room(&self, room_id: &str) -> Option<Room> {
        let rooms = self.rooms.read().await;
        rooms.get(room_id).cloned()
    }

    /// Remove a peer from their room
    /// Returns (room_id, role, name) if peer was found
    pub async fn remove_peer(&self, peer_id: &str) -> Option<(String, PeerRole, Option<String>)> {
        let mut peers = self.peers.write().await;

        if let Some(peer) = peers.remove(peer_id) {
            let mut rooms = self.rooms.write().await;

            if let Some(room) = rooms.get_mut(&peer.room_id) {
                match peer.role {
                    PeerRole::Proctor => {
                        // If proctor leaves, remove the entire room
                        tracing::info!(room_id = %peer.room_id, "Proctor left, closing room");
                        rooms.remove(&peer.room_id);

                        // Remove all students from this room
                        let students_to_remove: Vec<String> = peers
                            .iter()
                            .filter(|(_, p)| p.room_id == peer.room_id)
                            .map(|(id, _)| id.clone())
                            .collect();

                        for student_id in students_to_remove {
                            peers.remove(&student_id);
                        }
                    },
                    PeerRole::Student => {
                        // Remove student from room's student list
                        room.students.retain(|id| id != peer_id);
                        tracing::info!(
                            student_id = %peer_id,
                            room_id = %peer.room_id,
                            "Student left room"
                        );
                    },
                }
            }

            return Some((peer.room_id, peer.role, peer.name));
        }

        None
    }

    /// Get all peers in a room
    pub async fn get_room_peers(&self, room_id: &str) -> Vec<Peer> {
        let peers = self.peers.read().await;
        peers.values()
            .filter(|p| p.room_id == room_id)
            .cloned()
            .collect()
    }

    /// Check if a room exists
    pub async fn room_exists(&self, room_id: &str) -> bool {
        let rooms = self.rooms.read().await;
        rooms.contains_key(room_id)
    }

    /// Get proctor ID for a room
    pub async fn get_room_proctor(&self, room_id: &str) -> Option<String> {
        let rooms = self.rooms.read().await;
        rooms.get(room_id).map(|r| r.proctor_id.clone())
    }

    /// Check who should receive video from whom based on roles
    pub async fn should_forward_track(&self, from_peer_id: &str, to_peer_id: &str) -> bool {
        if from_peer_id == to_peer_id {
            return false; // Don't forward to self
        }

        let peers = self.peers.read().await;

        let from_peer = match peers.get(from_peer_id) {
            Some(p) => p,
            None => return false,
        };

        let to_peer = match peers.get(to_peer_id) {
            Some(p) => p,
            None => return false,
        };

        // Must be in the same room
        if from_peer.room_id != to_peer.room_id {
            return false;
        }

        // Apply role-based rules:
        match (&from_peer.role, &to_peer.role) {
            (PeerRole::Proctor, _) => true, // Everyone can see proctor
            (PeerRole::Student, PeerRole::Proctor) => true, // Proctor can see all students
            (PeerRole::Student, PeerRole::Student) => false, // Students cannot see each other
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_room() {
        let room_manager = RoomManager::new();
        let proctor_id = "proctor_123".to_string();
        let proctor_name = Some("Dr. Smith".to_string());

        let result = room_manager.create_room(proctor_id.clone(), proctor_name).await;
        assert!(result.is_ok());

        let room_id = result.unwrap();
        assert_eq!(room_id.len(), 6); // Room ID should be 6 digits

        // Verify room exists
        assert!(room_manager.room_exists(&room_id).await);

        // Verify proctor is registered
        let peer = room_manager.get_peer(&proctor_id).await;
        assert!(peer.is_some());
        let peer = peer.unwrap();
        assert_eq!(peer.id, proctor_id);
        matches!(peer.role, PeerRole::Proctor);
    }

    #[tokio::test]
    async fn test_join_room() {
        let room_manager = RoomManager::new();
        let proctor_id = "proctor_123".to_string();

        // Create room first
        let room_id = room_manager.create_room(proctor_id, None).await.unwrap();

        // Join as student
        let student_id = "student_456".to_string();
        let result = room_manager.join_room(room_id.clone(), student_id.clone(), Some("John Doe".to_string())).await;
        assert!(result.is_ok());

        // Verify student is in room
        let peer = room_manager.get_peer(&student_id).await;
        assert!(peer.is_some());
        let peer = peer.unwrap();
        matches!(peer.role, PeerRole::Student);
        assert_eq!(peer.room_id, room_id);
    }

    #[tokio::test]
    async fn test_join_nonexistent_room() {
        let room_manager = RoomManager::new();
        let student_id = "student_456".to_string();

        let result = room_manager.join_room("999999".to_string(), student_id, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_remove_student() {
        let room_manager = RoomManager::new();
        let proctor_id = "proctor_123".to_string();
        let room_id = room_manager.create_room(proctor_id, None).await.unwrap();

        let student_id = "student_456".to_string();
        room_manager.join_room(room_id.clone(), student_id.clone(), None).await.unwrap();

        // Remove student
        let result = room_manager.remove_peer(&student_id).await;
        assert!(result.is_some());
        let (removed_room_id, role, _wallet) = result.unwrap();
        assert_eq!(removed_room_id, room_id);
        matches!(role, PeerRole::Student);

        // Verify student is removed
        let peer = room_manager.get_peer(&student_id).await;
        assert!(peer.is_none());

        // Room should still exist
        assert!(room_manager.room_exists(&room_id).await);
    }

    #[tokio::test]
    async fn test_remove_proctor_closes_room() {
        let room_manager = RoomManager::new();
        let proctor_id = "proctor_123".to_string();
        let room_id = room_manager.create_room(proctor_id.clone(), None).await.unwrap();

        let student_id = "student_456".to_string();
        room_manager.join_room(room_id.clone(), student_id.clone(), None).await.unwrap();

        // Remove proctor
        let result = room_manager.remove_peer(&proctor_id).await;
        assert!(result.is_some());

        // Room should be closed
        assert!(!room_manager.room_exists(&room_id).await);

        // All students should be removed
        let student_peer = room_manager.get_peer(&student_id).await;
        assert!(student_peer.is_none());
    }

    #[tokio::test]
    async fn test_get_room_peers() {
        let room_manager = RoomManager::new();
        let proctor_id = "proctor_123".to_string();
        let room_id = room_manager.create_room(proctor_id, None).await.unwrap();

        let student1 = "student_1".to_string();
        let student2 = "student_2".to_string();
        room_manager.join_room(room_id.clone(), student1, None).await.unwrap();
        room_manager.join_room(room_id.clone(), student2, None).await.unwrap();

        let peers = room_manager.get_room_peers(&room_id).await;
        assert_eq!(peers.len(), 3); // 1 proctor + 2 students
    }

    #[tokio::test]
    async fn test_should_forward_track_proctor_to_all() {
        let room_manager = RoomManager::new();
        let proctor_id = "proctor_123".to_string();
        let room_id = room_manager.create_room(proctor_id.clone(), None).await.unwrap();

        let student_id = "student_456".to_string();
        room_manager.join_room(room_id, student_id.clone(), None).await.unwrap();

        // Proctor's video should be forwarded to student
        let should_forward = room_manager.should_forward_track(&proctor_id, &student_id).await;
        assert!(should_forward);
    }

    #[tokio::test]
    async fn test_should_forward_track_student_to_proctor() {
        let room_manager = RoomManager::new();
        let proctor_id = "proctor_123".to_string();
        let room_id = room_manager.create_room(proctor_id.clone(), None).await.unwrap();

        let student_id = "student_456".to_string();
        room_manager.join_room(room_id, student_id.clone(), None).await.unwrap();

        // Student's video should be forwarded to proctor
        let should_forward = room_manager.should_forward_track(&student_id, &proctor_id).await;
        assert!(should_forward);
    }

    #[tokio::test]
    async fn test_should_not_forward_track_student_to_student() {
        let room_manager = RoomManager::new();
        let proctor_id = "proctor_123".to_string();
        let room_id = room_manager.create_room(proctor_id, None).await.unwrap();

        let student1 = "student_1".to_string();
        let student2 = "student_2".to_string();
        room_manager.join_room(room_id.clone(), student1.clone(), None).await.unwrap();
        room_manager.join_room(room_id, student2.clone(), None).await.unwrap();

        // Students should not see each other
        let should_forward = room_manager.should_forward_track(&student1, &student2).await;
        assert!(!should_forward);
    }

    #[tokio::test]
    async fn test_should_not_forward_to_self() {
        let room_manager = RoomManager::new();
        let proctor_id = "proctor_123".to_string();
        room_manager.create_room(proctor_id.clone(), None).await.unwrap();

        // Should not forward to self
        let should_forward = room_manager.should_forward_track(&proctor_id, &proctor_id).await;
        assert!(!should_forward);
    }

    #[tokio::test]
    async fn test_should_not_forward_across_rooms() {
        let room_manager = RoomManager::new();
        let proctor1 = "proctor_1".to_string();
        let proctor2 = "proctor_2".to_string();

        let room1 = room_manager.create_room(proctor1.clone(), None).await.unwrap();
        let room2 = room_manager.create_room(proctor2.clone(), None).await.unwrap();

        let student1 = "student_1".to_string();
        let student2 = "student_2".to_string();
        room_manager.join_room(room1, student1.clone(), None).await.unwrap();
        room_manager.join_room(room2, student2.clone(), None).await.unwrap();

        // Should not forward tracks across different rooms
        let should_forward = room_manager.should_forward_track(&student1, &student2).await;
        assert!(!should_forward);
    }
}