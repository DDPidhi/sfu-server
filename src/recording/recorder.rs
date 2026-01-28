use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::SfuError;
use crate::ipfs::IpfsClient;
use super::pipeline::RecordingPipeline;
use super::state::RecordingState;

/// Key for identifying a recording: (room_id, peer_id)
pub type RecordingKey = (String, String);

/// Result of stopping a recording, including optional IPFS upload info
#[derive(Debug, Clone)]
pub struct RecordingResult {
    pub file_path: PathBuf,
    pub cid: Option<String>,
    pub ipfs_gateway_url: Option<String>,
}

pub struct RecordingManager {
    recordings: Arc<RwLock<HashMap<RecordingKey, Arc<RecordingPipeline>>>>,
    output_dir: String,
    ipfs_client: Option<Arc<IpfsClient>>,
    enabled: bool,
}

impl RecordingManager {
    pub fn new(output_dir: &str, ipfs_client: Option<Arc<IpfsClient>>, enabled: bool) -> Self {
        // Create output directory if it doesn't exist (only if enabled)
        if enabled {
            std::fs::create_dir_all(output_dir).ok();
        }

        Self {
            recordings: Arc::new(RwLock::new(HashMap::new())),
            output_dir: output_dir.to_string(),
            ipfs_client,
            enabled,
        }
    }

    /// Check if recording is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Start recording for a specific peer in a room
    pub async fn start_recording(&self, room_id: &str, peer_id: &str) -> Result<(), SfuError> {
        // Skip if recording is disabled
        if !self.enabled {
            tracing::debug!(
                room_id = %room_id,
                peer_id = %peer_id,
                "Recording disabled, skipping"
            );
            return Ok(());
        }

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
    pub async fn stop_recording(&self, room_id: &str, peer_id: &str) -> Result<RecordingResult, SfuError> {
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

        // Upload to IPFS if configured
        let (cid, ipfs_gateway_url) = if let Some(ref client) = self.ipfs_client {
            match client.upload_file(&output_path, room_id, peer_id).await {
                Ok(result) => {
                    tracing::info!(
                        room_id = %room_id,
                        peer_id = %peer_id,
                        cid = %result.cid,
                        "Uploaded recording to IPFS"
                    );
                    (Some(result.cid), Some(result.gateway_url))
                }
                Err(e) => {
                    tracing::error!(
                        room_id = %room_id,
                        peer_id = %peer_id,
                        error = %e,
                        "Failed to upload recording to IPFS, continuing with local file only"
                    );
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        Ok(RecordingResult {
            file_path: output_path,
            cid,
            ipfs_gateway_url,
        })
    }

    /// Stop all recordings in a room (used when room closes)
    pub async fn stop_all_recordings_in_room(&self, room_id: &str) -> Vec<(String, RecordingResult)> {
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
                    Ok(output_path) => {
                        tracing::info!(
                            room_id = %room_id,
                            peer_id = %peer_id,
                            file = %output_path.display(),
                            "Stopped recording for peer (room cleanup)"
                        );

                        // Upload to IPFS if configured
                        let (cid, ipfs_gateway_url) = if let Some(ref client) = self.ipfs_client {
                            match client.upload_file(&output_path, room_id, &peer_id).await {
                                Ok(result) => {
                                    tracing::info!(
                                        room_id = %room_id,
                                        peer_id = %peer_id,
                                        cid = %result.cid,
                                        "Uploaded recording to IPFS (room cleanup)"
                                    );
                                    (Some(result.cid), Some(result.gateway_url))
                                }
                                Err(e) => {
                                    tracing::error!(
                                        room_id = %room_id,
                                        peer_id = %peer_id,
                                        error = %e,
                                        "Failed to upload recording to IPFS during room cleanup"
                                    );
                                    (None, None)
                                }
                            }
                        } else {
                            (None, None)
                        };

                        stopped.push((peer_id, RecordingResult {
                            file_path: output_path,
                            cid,
                            ipfs_gateway_url,
                        }));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recording_result_debug() {
        let result = RecordingResult {
            file_path: PathBuf::from("/tmp/test.webm"),
            cid: Some("QmTest123".to_string()),
            ipfs_gateway_url: Some("http://localhost:8080/ipfs/QmTest123".to_string()),
        };
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("test.webm"));
        assert!(debug_str.contains("QmTest123"));
    }

    #[test]
    fn test_recording_result_clone() {
        let result = RecordingResult {
            file_path: PathBuf::from("/tmp/test.webm"),
            cid: Some("QmTest123".to_string()),
            ipfs_gateway_url: Some("http://localhost:8080/ipfs/QmTest123".to_string()),
        };
        let cloned = result.clone();
        assert_eq!(result.file_path, cloned.file_path);
        assert_eq!(result.cid, cloned.cid);
        assert_eq!(result.ipfs_gateway_url, cloned.ipfs_gateway_url);
    }

    #[test]
    fn test_recording_result_without_ipfs() {
        let result = RecordingResult {
            file_path: PathBuf::from("/tmp/test.webm"),
            cid: None,
            ipfs_gateway_url: None,
        };
        assert!(result.cid.is_none());
        assert!(result.ipfs_gateway_url.is_none());
    }

    #[test]
    fn test_recording_key_type() {
        let key: RecordingKey = ("room1".to_string(), "peer1".to_string());
        assert_eq!(key.0, "room1");
        assert_eq!(key.1, "peer1");
    }

    #[tokio::test]
    async fn test_recording_manager_disabled() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);
        assert!(!manager.is_enabled());
    }

    #[tokio::test]
    async fn test_recording_manager_enabled() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, true);
        assert!(manager.is_enabled());
    }

    #[tokio::test]
    async fn test_start_recording_when_disabled() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);

        // Starting recording when disabled should succeed silently
        let result = manager.start_recording("room1", "peer1").await;
        assert!(result.is_ok());

        // Should not actually create a recording
        assert!(!manager.is_recording("room1", "peer1").await);
    }

    #[tokio::test]
    async fn test_is_recording_no_recordings() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);
        assert!(!manager.is_recording("room1", "peer1").await);
    }

    #[tokio::test]
    async fn test_is_room_recording_no_recordings() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);
        assert!(!manager.is_room_recording("room1").await);
    }

    #[tokio::test]
    async fn test_get_recording_peers_empty() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);
        let peers = manager.get_recording_peers("room1").await;
        assert!(peers.is_empty());
    }

    #[tokio::test]
    async fn test_get_recording_state_none() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);
        let state = manager.get_recording_state("room1", "peer1").await;
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn test_stop_recording_not_found() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, true);
        let result = manager.stop_recording("room1", "peer1").await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(err_msg.contains("No recording found"));
    }

    #[tokio::test]
    async fn test_stop_all_recordings_empty_room() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);
        let stopped = manager.stop_all_recordings_in_room("room1").await;
        assert!(stopped.is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_peer_no_recording() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);
        // Should not panic when cleaning up a non-existent recording
        manager.cleanup_peer("room1", "peer1").await;
    }

    #[tokio::test]
    async fn test_cleanup_room_empty() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);
        // Should not panic when cleaning up an empty room
        manager.cleanup_room("room1").await;
    }

    #[tokio::test]
    async fn test_push_rtp_no_recording() {
        let manager = RecordingManager::new("/tmp/test_recordings", None, false);

        // Pushing RTP to non-existent recording should succeed silently
        let video_result = manager.push_video_rtp("room1", "peer1", &[0, 1, 2, 3]).await;
        assert!(video_result.is_ok());

        let audio_result = manager.push_audio_rtp("room1", "peer1", &[0, 1, 2, 3]).await;
        assert!(audio_result.is_ok());
    }
}
