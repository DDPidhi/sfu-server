use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use ethers::types::Address;

use super::client::{
    ContractClient, LeaveReason, Role, RoomCloseReason, SuspiciousActivityType, VerificationStatus,
};

/// Delay between dependent transactions to avoid nonce conflicts on Moonbase Alpha
/// Based on testing, 3 seconds is sufficient to allow each transaction to be
/// properly confirmed before sending the next one
const TX_DELAY: Duration = Duration::from_secs(3);

/// Events that can be queued for blockchain submission
/// All participant identifiers are wallet addresses for NFT generation support
#[derive(Debug, Clone)]
pub enum ChainEvent {
    RoomCreated {
        room_id: String,
        proctor: Address,
        proctor_name: Option<String>,
    },
    ParticipantJoined {
        room_id: String,
        participant: Address,
        name: Option<String>,
        role: Role,
    },
    ParticipantLeft {
        room_id: String,
        participant: Address,
        reason: LeaveReason,
    },
    ParticipantKicked {
        room_id: String,
        proctor: Address,
        kicked: Address,
        reason: Option<String>,
    },
    IdVerification {
        room_id: String,
        participant: Address,
        status: VerificationStatus,
        verified_by: String,
    },
    SuspiciousActivity {
        room_id: String,
        participant: Address,
        activity_type: SuspiciousActivityType,
        details: Option<String>,
    },
    RecordingStarted {
        room_id: String,
        participant: Address,
    },
    RecordingStopped {
        room_id: String,
        participant: Address,
        duration_secs: u64,
        ipfs_cid: Option<String>,
    },
    RoomClosed {
        room_id: String,
        reason: RoomCloseReason,
    },
    /// Create a new exam result for a participant
    CreateExamResult {
        room_id: String,
        participant: Address,
        grade: u64,
        exam_name: String,
    },
    /// Add a single recording CID to an exam result
    AddRecordingToResult {
        result_id: u64,
        ipfs_cid: String,
    },
    /// Add multiple recording CIDs to an exam result
    AddRecordingsToResult {
        result_id: u64,
        ipfs_cids: Vec<String>,
    },
    /// Update the grade of an exam result
    UpdateExamResultGrade {
        result_id: u64,
        new_grade: u64,
    },
    /// Mark an NFT as minted for an exam result
    MarkNftMinted {
        result_id: u64,
    },
}

impl ChainEvent {
    /// Returns the dependency key for this event.
    /// Events with the same key must be serialized with delays between them.
    /// Format: "room:<room_id>" for room-level events, "room:<room_id>:participant:<address>" for participant events
    fn dependency_key(&self) -> Option<String> {
        match self {
            // Room-level events - all participants in this room depend on these
            ChainEvent::RoomCreated { room_id, .. } => {
                Some(format!("room:{}", room_id))
            }
            ChainEvent::RoomClosed { room_id, .. } => {
                Some(format!("room:{}", room_id))
            }
            // Participant-level events - only this participant's events depend on each other
            ChainEvent::ParticipantJoined { room_id, participant, .. } => {
                Some(format!("room:{}:participant:{:?}", room_id, participant))
            }
            ChainEvent::ParticipantLeft { room_id, participant, .. } => {
                Some(format!("room:{}:participant:{:?}", room_id, participant))
            }
            ChainEvent::ParticipantKicked { room_id, kicked, .. } => {
                Some(format!("room:{}:participant:{:?}", room_id, kicked))
            }
            ChainEvent::IdVerification { room_id, participant, .. } => {
                Some(format!("room:{}:participant:{:?}", room_id, participant))
            }
            ChainEvent::SuspiciousActivity { room_id, participant, .. } => {
                Some(format!("room:{}:participant:{:?}", room_id, participant))
            }
            ChainEvent::RecordingStarted { room_id, participant } => {
                Some(format!("room:{}:participant:{:?}", room_id, participant))
            }
            ChainEvent::RecordingStopped { room_id, participant, .. } => {
                Some(format!("room:{}:participant:{:?}", room_id, participant))
            }
            ChainEvent::CreateExamResult { room_id, participant, .. } => {
                Some(format!("room:{}:participant:{:?}", room_id, participant))
            }
            // Result-level events - these depend on the result ID, not room/participant
            ChainEvent::AddRecordingToResult { result_id, .. } => {
                Some(format!("result:{}", result_id))
            }
            ChainEvent::AddRecordingsToResult { result_id, .. } => {
                Some(format!("result:{}", result_id))
            }
            ChainEvent::UpdateExamResultGrade { result_id, .. } => {
                Some(format!("result:{}", result_id))
            }
            ChainEvent::MarkNftMinted { result_id } => {
                Some(format!("result:{}", result_id))
            }
        }
    }

    /// Returns the room ID if this event depends on RoomCreated completing first
    fn room_dependency(&self) -> Option<&str> {
        match self {
            ChainEvent::RoomCreated { .. } => None, // RoomCreated has no room dependency
            ChainEvent::ParticipantJoined { room_id, .. } => Some(room_id),
            ChainEvent::ParticipantLeft { room_id, .. } => Some(room_id),
            ChainEvent::ParticipantKicked { room_id, .. } => Some(room_id),
            ChainEvent::IdVerification { room_id, .. } => Some(room_id),
            ChainEvent::SuspiciousActivity { room_id, .. } => Some(room_id),
            ChainEvent::RecordingStarted { room_id, .. } => Some(room_id),
            ChainEvent::RecordingStopped { room_id, .. } => Some(room_id),
            ChainEvent::RoomClosed { room_id, .. } => Some(room_id),
            ChainEvent::CreateExamResult { room_id, .. } => Some(room_id),
            // Result events don't have room dependency (they depend on CreateExamResult)
            ChainEvent::AddRecordingToResult { .. } => None,
            ChainEvent::AddRecordingsToResult { .. } => None,
            ChainEvent::UpdateExamResultGrade { .. } => None,
            ChainEvent::MarkNftMinted { .. } => None,
        }
    }
}

/// Tracks the last transaction time for each dependency key
struct TransactionTracker {
    /// Maps dependency key -> last transaction completion time
    last_tx_times: HashMap<String, Instant>,
    /// Maps room_id -> whether RoomCreated has completed
    room_created: HashMap<String, bool>,
}

impl TransactionTracker {
    fn new() -> Self {
        Self {
            last_tx_times: HashMap::new(),
            room_created: HashMap::new(),
        }
    }

    /// Check if we need to wait before processing this event
    fn needs_delay(&self, event: &ChainEvent) -> Option<Duration> {
        // First check if this event depends on RoomCreated
        if let Some(room_id) = event.room_dependency() {
            if !self.room_created.get(room_id).copied().unwrap_or(false) {
                // Room not yet created, we need to wait for it
                // Return full delay to allow RoomCreated to complete
                return Some(TX_DELAY);
            }
        }

        // Check if there was a recent transaction for the same dependency key
        if let Some(key) = event.dependency_key() {
            if let Some(last_time) = self.last_tx_times.get(&key) {
                let elapsed = last_time.elapsed();
                if elapsed < TX_DELAY {
                    return Some(TX_DELAY - elapsed);
                }
            }
        }

        None
    }

    /// Record that a transaction completed for this event
    fn record_completion(&mut self, event: &ChainEvent) {
        // Mark RoomCreated as complete
        if let ChainEvent::RoomCreated { room_id, .. } = event {
            self.room_created.insert(room_id.clone(), true);
        }

        // Record the completion time for this dependency key
        if let Some(key) = event.dependency_key() {
            self.last_tx_times.insert(key, Instant::now());
        }
    }
}

/// Non-blocking event queue for submitting events to the blockchain
///
/// This queue allows the SFU server to emit events without blocking
/// on blockchain confirmation. Events are processed in the background.
///
/// Delays are only applied between dependent events:
/// - Events for the same (room, participant) pair are serialized with delays
/// - Events for different participants can be processed without waiting
/// - All participant events wait for RoomCreated to complete first
pub struct EventQueue {
    sender: mpsc::UnboundedSender<ChainEvent>,
}

impl EventQueue {
    /// Creates a new event queue with a background processor
    pub fn new(client: Arc<ContractClient>) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        // Spawn background processor
        tokio::spawn(Self::process_events(client, receiver));

        Self { sender }
    }

    /// Queues an event for blockchain submission
    ///
    /// This method is non-blocking and returns immediately.
    /// Events are processed in the background.
    pub fn emit(&self, event: ChainEvent) {
        tracing::info!(event = ?event, "Queueing chain event");
        if let Err(e) = self.sender.send(event) {
            tracing::error!(error = %e, "Failed to queue chain event");
        }
    }

    /// Background processor that handles queued events
    async fn process_events(
        client: Arc<ContractClient>,
        mut receiver: mpsc::UnboundedReceiver<ChainEvent>,
    ) {
        tracing::info!(
            tx_delay_secs = TX_DELAY.as_secs(),
            "Chain event processor started (per-participant tracking enabled)"
        );

        let tracker = Arc::new(RwLock::new(TransactionTracker::new()));

        while let Some(event) = receiver.recv().await {
            // Check if we need to delay for dependencies
            let delay = {
                let tracker_read = tracker.read().await;
                tracker_read.needs_delay(&event)
            };

            if let Some(delay_duration) = delay {
                tracing::debug!(
                    delay_ms = delay_duration.as_millis(),
                    event = ?event,
                    "Waiting for dependent transaction"
                );
                sleep(delay_duration).await;
            }

            tracing::info!(event = ?event, "Processing chain event");

            let result = Self::handle_event(&client, &event).await;

            // Record completion regardless of success/failure
            // This prevents indefinite blocking on failed events
            {
                let mut tracker_write = tracker.write().await;
                tracker_write.record_completion(&event);
            }

            match result {
                Ok(()) => tracing::info!("Chain event processed successfully"),
                Err(e) => tracing::error!(error = %e, "Failed to process chain event"),
            }
        }

        tracing::info!("Chain event processor stopped");
    }

    /// Handles a single event by calling the appropriate contract method
    async fn handle_event(
        client: &ContractClient,
        event: &ChainEvent,
    ) -> crate::error::Result<()> {
        match event {
            ChainEvent::RoomCreated {
                room_id,
                proctor,
                proctor_name,
            } => {
                client
                    .record_room_created(room_id, *proctor, proctor_name.as_deref())
                    .await
            }
            ChainEvent::ParticipantJoined {
                room_id,
                participant,
                name,
                role,
            } => {
                client
                    .record_participant_joined(room_id, *participant, name.as_deref(), *role)
                    .await
            }
            ChainEvent::ParticipantLeft {
                room_id,
                participant,
                reason,
            } => {
                client
                    .record_participant_left(room_id, *participant, *reason)
                    .await
            }
            ChainEvent::ParticipantKicked {
                room_id,
                proctor,
                kicked,
                reason,
            } => {
                client
                    .record_participant_kicked(
                        room_id,
                        *proctor,
                        *kicked,
                        reason.as_deref(),
                    )
                    .await
            }
            ChainEvent::IdVerification {
                room_id,
                participant,
                status,
                verified_by,
            } => {
                client
                    .record_id_verification(room_id, *participant, *status, verified_by)
                    .await
            }
            ChainEvent::SuspiciousActivity {
                room_id,
                participant,
                activity_type,
                details,
            } => {
                client
                    .record_suspicious_activity(room_id, *participant, *activity_type, details.as_deref())
                    .await
            }
            ChainEvent::RecordingStarted { room_id, participant } => {
                client.record_recording_started(room_id, *participant).await
            }
            ChainEvent::RecordingStopped {
                room_id,
                participant,
                duration_secs,
                ipfs_cid,
            } => {
                client
                    .record_recording_stopped(room_id, *participant, *duration_secs, ipfs_cid.as_deref())
                    .await
            }
            ChainEvent::RoomClosed { room_id, reason } => {
                client.close_room(room_id, *reason).await
            }
            ChainEvent::CreateExamResult {
                room_id,
                participant,
                grade,
                exam_name,
            } => {
                client
                    .create_exam_result(room_id, *participant, *grade, exam_name)
                    .await
            }
            ChainEvent::AddRecordingToResult { result_id, ipfs_cid } => {
                client.add_recording_to_result(*result_id, ipfs_cid).await
            }
            ChainEvent::AddRecordingsToResult { result_id, ipfs_cids } => {
                client.add_recordings_to_result(*result_id, ipfs_cids.clone()).await
            }
            ChainEvent::UpdateExamResultGrade { result_id, new_grade } => {
                client.update_exam_result_grade(*result_id, *new_grade).await
            }
            ChainEvent::MarkNftMinted { result_id } => {
                client.mark_nft_minted(*result_id).await
            }
        }
    }
}

impl Clone for EventQueue {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_event_debug() {
        let event = ChainEvent::RoomCreated {
            room_id: "room_123".to_string(),
            proctor: Address::zero(),
            proctor_name: Some("Dr. Smith".to_string()),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("RoomCreated"));
        assert!(debug_str.contains("room_123"));
    }

    #[test]
    fn test_chain_event_clone() {
        let event = ChainEvent::ParticipantJoined {
            room_id: "room_123".to_string(),
            participant: Address::zero(),
            name: Some("John".to_string()),
            role: Role::Student,
        };
        let cloned = event.clone();
        match cloned {
            ChainEvent::ParticipantJoined { room_id, .. } => {
                assert_eq!(room_id, "room_123");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_dependency_key_room_events() {
        let room_created = ChainEvent::RoomCreated {
            room_id: "room_1".to_string(),
            proctor: Address::zero(),
            proctor_name: None,
        };
        assert_eq!(room_created.dependency_key(), Some("room:room_1".to_string()));

        let room_closed = ChainEvent::RoomClosed {
            room_id: "room_1".to_string(),
            reason: RoomCloseReason::ProctorLeft,
        };
        assert_eq!(room_closed.dependency_key(), Some("room:room_1".to_string()));
    }

    #[test]
    fn test_dependency_key_participant_events() {
        let participant = Address::zero();

        let joined = ChainEvent::ParticipantJoined {
            room_id: "room_1".to_string(),
            participant,
            name: None,
            role: Role::Student,
        };
        let key = joined.dependency_key().unwrap();
        assert!(key.contains("room:room_1"));
        assert!(key.contains("participant:"));

        let recording = ChainEvent::RecordingStarted {
            room_id: "room_1".to_string(),
            participant,
        };
        // Same participant in same room should have same key
        assert_eq!(joined.dependency_key(), recording.dependency_key());
    }

    #[test]
    fn test_different_participants_different_keys() {
        let participant_a = Address::from_low_u64_be(1);
        let participant_b = Address::from_low_u64_be(2);

        let event_a = ChainEvent::ParticipantJoined {
            room_id: "room_1".to_string(),
            participant: participant_a,
            name: None,
            role: Role::Student,
        };

        let event_b = ChainEvent::ParticipantJoined {
            room_id: "room_1".to_string(),
            participant: participant_b,
            name: None,
            role: Role::Student,
        };

        // Different participants should have different dependency keys
        assert_ne!(event_a.dependency_key(), event_b.dependency_key());
    }

    #[test]
    fn test_room_dependency() {
        let room_created = ChainEvent::RoomCreated {
            room_id: "room_1".to_string(),
            proctor: Address::zero(),
            proctor_name: None,
        };
        assert!(room_created.room_dependency().is_none());

        let participant_joined = ChainEvent::ParticipantJoined {
            room_id: "room_1".to_string(),
            participant: Address::zero(),
            name: None,
            role: Role::Student,
        };
        assert_eq!(participant_joined.room_dependency(), Some("room_1"));
    }

    #[test]
    fn test_create_exam_result_event() {
        let event = ChainEvent::CreateExamResult {
            room_id: "exam_room_1".to_string(),
            participant: Address::zero(),
            grade: 8750, // 87.50%
            exam_name: "Final Exam".to_string(),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("CreateExamResult"));
        assert!(debug_str.contains("8750"));
    }

    #[test]
    fn test_add_recording_event() {
        let event = ChainEvent::AddRecordingToResult {
            result_id: 1,
            ipfs_cid: "QmTest123".to_string(),
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("AddRecordingToResult"));
        assert!(debug_str.contains("QmTest123"));
    }

    #[test]
    fn test_add_recordings_event() {
        let event = ChainEvent::AddRecordingsToResult {
            result_id: 1,
            ipfs_cids: vec!["QmCid1".to_string(), "QmCid2".to_string()],
        };
        let debug_str = format!("{:?}", event);
        assert!(debug_str.contains("AddRecordingsToResult"));
        assert!(debug_str.contains("QmCid1"));
    }
}
