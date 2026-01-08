use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
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

impl ForwardedTrack {
    /// Check if this is a video track
    pub fn is_video(&self) -> bool {
        self.remote_track.kind() == RTPCodecType::Video
    }

    /// Get the SSRC of the remote track
    pub fn ssrc(&self) -> u32 {
        self.remote_track.ssrc()
    }
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


    pub async fn add_track(
        &self,
        track_id: String,
        source_peer_id: String,
        remote_track: Arc<TrackRemote>,
    ) {
        let forwarded_track = ForwardedTrack {
            id: track_id.clone(),
            kind: remote_track.kind().to_string(),
            source_peer_id,
            remote_track,
            local_tracks: HashMap::new(),
        };

        let mut tracks = self.tracks.write().await;
        tracks.insert(track_id, forwarded_track);
    }

    /// Create a local track for forwarding to a peer.
    /// Returns (local_track, is_new_subscriber, is_video, ssrc, source_peer_id)
    pub async fn create_local_track_for_peer(
        &self,
        track_id: &str,
        target_peer_id: &str,
    ) -> Option<(Arc<TrackLocalStaticRTP>, bool, bool, u32, String)> {
        let mut tracks = self.tracks.write().await;

        if let Some(forwarded_track) = tracks.get_mut(track_id) {
            if forwarded_track.source_peer_id == target_peer_id {
                return None;
            }

            let is_video = forwarded_track.is_video();
            let ssrc = forwarded_track.ssrc();
            let source_peer_id = forwarded_track.source_peer_id.clone();

            if let Some(existing_track) = forwarded_track.local_tracks.get(target_peer_id) {
                return Some((existing_track.clone(), false, is_video, ssrc, source_peer_id));
            }

            let codec = forwarded_track.remote_track.codec();
            let local_track = Arc::new(TrackLocalStaticRTP::new(
                codec.capability.clone(),
                track_id.to_string(), // Keep the original track ID which includes source peer ID
                format!("{}_stream", forwarded_track.source_peer_id),
            ));

            forwarded_track.local_tracks.insert(target_peer_id.to_string(), local_track.clone());
            Some((local_track, true, is_video, ssrc, source_peer_id))
        } else {
            None
        }
    }


    pub async fn get_tracks_from_peer(&self, peer_id: &str) -> Vec<String> {
        let tracks = self.tracks.read().await;
        tracks
            .values()
            .filter(|track| track.source_peer_id == peer_id)
            .map(|track| track.id.clone())
            .collect()
    }


    pub async fn remove_peer_tracks(&self, peer_id: &str) {
        let mut tracks = self.tracks.write().await;
        tracks.retain(|_, track| track.source_peer_id != peer_id);
    }

    pub async fn get_track(&self, track_id: &str) -> Option<ForwardedTrack> {
        let tracks = self.tracks.read().await;
        tracks.get(track_id).cloned()
    }


    pub async fn get_all_track_ids(&self) -> Vec<String> {
        let tracks = self.tracks.read().await;
        tracks.keys().cloned().collect()
    }
}