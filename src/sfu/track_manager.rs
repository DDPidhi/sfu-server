use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_remote::TrackRemote;

#[derive(Clone)]
pub struct ForwardedTrack {
    pub id: String,
    pub kind: String,
    pub source_peer_id: String,
    pub remote_track: Arc<TrackRemote>,
    pub local_tracks: HashMap<String, Arc<TrackLocalStaticRTP>>,
}

pub struct TrackManager {
    tracks: Arc<RwLock<HashMap<String, ForwardedTrack>>>,
}

impl TrackManager {
    pub fn new() -> Self {
        Self {
            tracks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}