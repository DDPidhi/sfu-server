use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordingState {
    Idle,
    Recording,
    Stopping,
    Stopped,
    Error(String),
}

impl Default for RecordingState {
    fn default() -> Self {
        Self::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let state = RecordingState::default();
        assert_eq!(state, RecordingState::Idle);
    }

    #[test]
    fn test_state_equality() {
        assert_eq!(RecordingState::Idle, RecordingState::Idle);
        assert_eq!(RecordingState::Recording, RecordingState::Recording);
        assert_eq!(RecordingState::Stopping, RecordingState::Stopping);
        assert_eq!(RecordingState::Stopped, RecordingState::Stopped);
        assert_eq!(
            RecordingState::Error("test".to_string()),
            RecordingState::Error("test".to_string())
        );
        assert_ne!(
            RecordingState::Error("a".to_string()),
            RecordingState::Error("b".to_string())
        );
    }

    #[test]
    fn test_state_clone() {
        let state = RecordingState::Recording;
        let cloned = state.clone();
        assert_eq!(state, cloned);
    }

    #[test]
    fn test_state_serialization() {
        let state = RecordingState::Recording;
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(json, "\"Recording\"");

        let error_state = RecordingState::Error("test error".to_string());
        let error_json = serde_json::to_string(&error_state).unwrap();
        assert!(error_json.contains("Error"));
        assert!(error_json.contains("test error"));
    }

    #[test]
    fn test_state_deserialization() {
        let json = "\"Idle\"";
        let state: RecordingState = serde_json::from_str(json).unwrap();
        assert_eq!(state, RecordingState::Idle);

        let recording_json = "\"Recording\"";
        let recording_state: RecordingState = serde_json::from_str(recording_json).unwrap();
        assert_eq!(recording_state, RecordingState::Recording);
    }
}