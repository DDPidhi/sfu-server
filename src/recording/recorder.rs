use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::SfuError;
use super::pipeline::RecordingPipeline;
use super::state::RecordingState;

/// Key for identifying a recording: (room_id, peer_id)
pub type RecordingKey = (String, String);

pub struct RecordingManager {
    recordings: Arc<RwLock<HashMap<RecordingKey, Arc<RecordingPipeline>>>>,
    output_dir: String,
}

impl RecordingManager {
    pub fn new(output_dir: &str) -> Self {
        // Create output directory if it doesn't exist
        std::fs::create_dir_all(output_dir).ok();

        Self {
            recordings: Arc::new(RwLock::new(HashMap::new())),
            output_dir: output_dir.to_string(),
        }
    }

    /// Start recording for a specific peer in a room
    pub async fn start_recording(&self, room_id: &str, peer_id: &str) -> Result<(), SfuError> {
        let mut recordings = self.recordings.write().await;
        let key = (room_id.to_string(), peer_id.to_string());

        if recordings.contains_key(&key) {
            return Err(SfuError::Internal(format!(
                "Recording already exists for peer {} in room {}",
                peer_id, room_id
            )));
        }

        let pipeline = RecordingPipeline::new(room_id, peer_id, &self.output_dir)?;
        pipeline.start().await?;

        recordings.insert(key, Arc::new(pipeline));
        tracing::info!(
            room_id = %room_id,
            peer_id = %peer_id,
            "Started recording for peer"
        );
        Ok(())
    }

    /// Stop recording for a specific peer in a room
    pub async fn stop_recording(&self, room_id: &str, peer_id: &str) -> Result<PathBuf, SfuError> {
        let mut recordings = self.recordings.write().await;
        let key = (room_id.to_string(), peer_id.to_string());

        let pipeline = recordings.remove(&key).ok_or_else(|| {
            SfuError::Internal(format!(
                "No recording found for peer {} in room {}",
                peer_id, room_id
            ))
        })?;

        let output_path = pipeline.stop().await?;
        tracing::info!(
            room_id = %room_id,
            peer_id = %peer_id,
            file = %output_path.display(),
            "Stopped recording for peer"
        );
        Ok(output_path)
    }

    /// Stop all recordings in a room (used when room closes)
    pub async fn stop_all_recordings_in_room(&self, room_id: &str) -> Vec<(String, PathBuf)> {
        let mut recordings = self.recordings.write().await;
        let mut stopped = Vec::new();

        // Find all recordings for this room
        let keys_to_remove: Vec<RecordingKey> = recordings
            .keys()
            .filter(|(rid, _)| rid == room_id)
            .cloned()
            .collect();

        for key in keys_to_remove {
            let peer_id = key.1.clone();
            if let Some(pipeline) = recordings.remove(&key) {
                match pipeline.stop().await {
                    Ok(path) => {
                        tracing::info!(
                            room_id = %room_id,
                            peer_id = %peer_id,
                            file = %path.display(),
                            "Stopped recording for peer (room cleanup)"
                        );
                        stopped.push((peer_id, path));
                    }
                    Err(e) => {
                        tracing::error!(
                            room_id = %room_id,
                            peer_id = %peer_id,
                            error = %e,
                            "Failed to stop recording during room cleanup"
                        );
                    }
                }
            }
        }

        stopped
    }

    /// Push video RTP data for a specific peer's recording
    pub async fn push_video_rtp(&self, room_id: &str, peer_id: &str, data: &[u8]) -> Result<(), SfuError> {
        let recordings = self.recordings.read().await;
        let key = (room_id.to_string(), peer_id.to_string());

        if let Some(pipeline) = recordings.get(&key) {
            pipeline.push_video_rtp(data)?;
        }
        Ok(())
    }

    /// Push audio RTP data for a specific peer's recording
    pub async fn push_audio_rtp(&self, room_id: &str, peer_id: &str, data: &[u8]) -> Result<(), SfuError> {
        let recordings = self.recordings.read().await;
        let key = (room_id.to_string(), peer_id.to_string());

        if let Some(pipeline) = recordings.get(&key) {
            pipeline.push_audio_rtp(data)?;
        }
        Ok(())
    }

    /// Get the recording state for a specific peer
    pub async fn get_recording_state(&self, room_id: &str, peer_id: &str) -> Option<RecordingState> {
        let recordings = self.recordings.read().await;
        let key = (room_id.to_string(), peer_id.to_string());

        if let Some(pipeline) = recordings.get(&key) {
            Some(pipeline.get_state().await)
        } else {
            None
        }
    }

    /// Check if a specific peer is being recorded
    pub async fn is_recording(&self, room_id: &str, peer_id: &str) -> bool {
        let recordings = self.recordings.read().await;
        let key = (room_id.to_string(), peer_id.to_string());
        recordings.contains_key(&key)
    }

    /// Check if any recording exists in a room
    pub async fn is_room_recording(&self, room_id: &str) -> bool {
        let recordings = self.recordings.read().await;
        recordings.keys().any(|(rid, _)| rid == room_id)
    }

    /// Get all peer IDs being recorded in a room
    pub async fn get_recording_peers(&self, room_id: &str) -> Vec<String> {
        let recordings = self.recordings.read().await;
        recordings
            .keys()
            .filter(|(rid, _)| rid == room_id)
            .map(|(_, pid)| pid.clone())
            .collect()
    }

    /// Cleanup a specific peer's recording (stop if active)
    pub async fn cleanup_peer(&self, room_id: &str, peer_id: &str) {
        if self.is_recording(room_id, peer_id).await {
            if let Err(e) = self.stop_recording(room_id, peer_id).await {
                tracing::error!(
                    room_id = %room_id,
                    peer_id = %peer_id,
                    error = %e,
                    "Failed to stop recording on peer cleanup"
                );
            }
        }
    }

    /// Cleanup all recordings in a room
    pub async fn cleanup_room(&self, room_id: &str) {
        let stopped = self.stop_all_recordings_in_room(room_id).await;
        if !stopped.is_empty() {
            tracing::info!(
                room_id = %room_id,
                count = stopped.len(),
                "Cleaned up all recordings in room"
            );
        }
    }
}
