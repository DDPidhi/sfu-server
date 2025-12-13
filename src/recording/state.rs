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