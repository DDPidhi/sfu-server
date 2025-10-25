use thiserror::Error;

/// Custom error types for the SFU server
#[derive(Debug, Error)]
pub enum SfuError {
    /// WebRTC related errors
    #[error("Failed to create peer connection: {0}")]
    PeerConnectionCreation(String),

    #[error("Failed to create offer: {0}")]
    CreateOfferFailed(String),

    #[error("Failed to create answer: {0}")]
    CreateAnswerFailed(String),

    #[error("Invalid SDP format: {0}")]
    InvalidSdp(String),

    #[error("Failed to set local description: {0}")]
    SetLocalDescriptionFailed(String),

    #[error("Failed to set remote description: {0}")]
    SetRemoteDescriptionFailed(String),

    #[error("Failed to add ICE candidate: {0}")]
    AddIceCandidateFailed(String),

    #[error("Failed to create track: {0}")]
    TrackCreationFailed(String),

    #[error("Failed to add track: {0}")]
    AddTrackFailed(String),

    /// Room and peer management errors
    #[error("Room {0} not found")]
    RoomNotFound(String),

    #[error("Room {0} already exists")]
    RoomAlreadyExists(String),

    #[error("Peer {0} not found")]
    PeerNotFound(String),

    #[error("Peer {0} already exists")]
    PeerAlreadyExists(String),

    #[error("Peer {0} not authorized for this operation")]
    Unauthorized(String),

    #[error("Invalid peer role: {0}")]
    InvalidRole(String),

    #[error("Proctor approval required for peer {0}")]
    ApprovalRequired(String),

    /// Signaling errors
    #[error("Invalid signaling message: {0}")]
    InvalidSignalingMessage(String),

    #[error("Failed to serialize message: {0}")]
    SerializationFailed(#[from] serde_json::Error),

    #[error("Signaling state not stable: {0:?}")]
    InvalidSignalingState(String),

    #[error("Renegotiation already in progress for peer {0}")]
    RenegotiationInProgress(String),

    /// Track management errors
    #[error("Track {0} not found")]
    TrackNotFound(String),

    #[error("Failed to register track for peer {0}")]
    TrackRegistrationFailed(String),

    #[error("No tracks available for peer {0}")]
    NoTracksAvailable(String),

    /// Configuration errors
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),

    #[error("Missing required configuration: {0}")]
    MissingConfiguration(String),

    #[error("Failed to parse configuration: {0}")]
    ConfigurationParseFailed(String),

    /// WebRTC API errors
    #[error("WebRTC API error: {0}")]
    WebRtcApi(String),

    #[error("Failed to create media engine: {0}")]
    MediaEngineCreation(String),

    #[error("Failed to register codec: {0}")]
    CodecRegistrationFailed(String),

    /// Network errors
    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Connection timeout for peer {0}")]
    ConnectionTimeout(String),

    #[error("ICE connection failed for peer {0}")]
    IceConnectionFailed(String),

    /// Generic errors
    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("Operation timed out: {0}")]
    Timeout(String),

    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Convenience type alias for Results using SfuError
pub type Result<T> = std::result::Result<T, SfuError>;

impl SfuError {
    /// Helper to create Internal errors with context
    pub fn internal(msg: impl Into<String>) -> Self {
        SfuError::Internal(msg.into())
    }

    /// Helper to create WebRTC API errors
    pub fn webrtc_api(msg: impl Into<String>) -> Self {
        SfuError::WebRtcApi(msg.into())
    }

    /// Helper to create network errors
    pub fn network(msg: impl Into<String>) -> Self {
        SfuError::NetworkError(msg.into())
    }
}

/// Convert webrtc::Error to SfuError
impl From<webrtc::Error> for SfuError {
    fn from(err: webrtc::Error) -> Self {
        SfuError::WebRtcApi(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = SfuError::RoomNotFound("test-room".to_string());
        assert_eq!(err.to_string(), "Room test-room not found");
    }

    #[test]
    fn test_error_helpers() {
        let err = SfuError::internal("Something went wrong");
        assert!(matches!(err, SfuError::Internal(_)));
    }
}
