use std::sync::Arc;
use tokio::sync::mpsc;
use warp::ws::Message;
use webrtc::api::API;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::track::track_local::TrackLocalWriter;

use super::track_manager::TrackManager;
use super::webrtc_utils::get_ice_servers;


pub type TrackNotificationSender = mpsc::UnboundedSender<(String, String)>;

pub struct SfuConnection {
    pub peer_id: String,
    pub peer_connection: Arc<RTCPeerConnection>,
    pub sender: mpsc::UnboundedSender<Message>,
}

impl SfuConnection {
    pub async fn new(
        peer_id: String,
        sender: mpsc::UnboundedSender<Message>,
        api: &Arc<API>,
        track_manager: Arc<TrackManager>,
        track_notification_sender: Option<TrackNotificationSender>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let config = RTCConfiguration {
            ice_servers: get_ice_servers(&Default::default()),
            ..Default::default()
        };

        let peer_connection = Arc::new(api.new_peer_connection(config).await?);

        peer_connection.add_transceiver_from_kind(RTPCodecType::Video, None).await?;
        peer_connection.add_transceiver_from_kind(RTPCodecType::Audio, None).await?;


        let peer_id_clone = peer_id.clone();
        let track_manager_clone = track_manager.clone();
        let pc_clone = peer_connection.clone();
        let notification_sender = track_notification_sender.clone();

        peer_connection.on_track(Box::new(move |track, _receiver, _transceiver| {
            let peer_id = peer_id_clone.clone();
            let track_manager = track_manager_clone.clone();
            let pc = pc_clone.clone();
            let track = track.clone();
            let sender = notification_sender.clone();

            Box::pin(async move {
                // Create a unique track ID that includes the peer ID
                let original_track_id = track.id();
                let track_kind = track.kind().to_string();
                let track_id = format!("{}_{}_{}",
                                       peer_id,
                                       track_kind,
                                       original_track_id
                );
                tracing::info!(
                    peer_id = %peer_id,
                    track_kind = %track_kind,
                    original_track_id = %original_track_id,
                    track_id = %track_id,
                    "SFU received track from peer"
                );

                track_manager.add_track(track_id.clone(), peer_id.clone(), track.clone()).await;

                Self::start_track_forwarding(track, track_id.clone(), peer_id.clone(), track_manager.clone(), pc).await;

                if let Some(tx) = sender {
                    if let Err(_) = tx.send((peer_id.clone(), track_id.clone())) {
                        tracing::error!(
                            peer_id = %peer_id,
                            track_id = %track_id,
                            "Failed to notify SFU server about new track"
                        );
                    }
                }
            })
        }));

        let sender_clone = sender.clone();
        let peer_id_for_ice = peer_id.clone();
        peer_connection.on_ice_candidate(Box::new(move |candidate| {
            let sender = sender_clone.clone();
            let peer_id = peer_id_for_ice.clone();
            Box::pin(async move {
                if let Some(candidate) = candidate {
                    tracing::debug!(peer_id = %peer_id, "Generating ICE candidate for peer");
                    if let Ok(candidate_json) = candidate.to_json() {
                        let ice_message = serde_json::json!({
                            "type": "IceCandidate",
                            "peer_id": "sfu",
                            "candidate": candidate_json.candidate,
                            "sdp_mid": candidate_json.sdp_mid,
                            "sdp_mline_index": candidate_json.sdp_mline_index,
                        });

                        if let Ok(msg_str) = serde_json::to_string(&ice_message) {
                            tracing::debug!(peer_id = %peer_id, "Sending ICE candidate to peer");
                            let _ = sender.send(Message::text(msg_str));
                        }
                    }
                } else {
                    tracing::info!(peer_id = %peer_id, "ICE gathering complete for peer");
                }
            })
        }));

        let peer_id_clone = peer_id.clone();
        peer_connection.on_ice_connection_state_change(Box::new(move |state| {
            let peer_id = peer_id_clone.clone();
            Box::pin(async move {
                tracing::info!(peer_id = %peer_id, ?state, "ICE connection state changed");
            })
        }));

        let peer_id_clone = peer_id.clone();
        peer_connection.on_ice_gathering_state_change(Box::new(move |state| {
            let peer_id = peer_id_clone.clone();
            Box::pin(async move {
                tracing::debug!(peer_id = %peer_id, ?state, "ICE gathering state changed");
            })
        }));

        Ok(Self {
            peer_id,
            peer_connection,
            sender,
        })
    }

    async fn start_track_forwarding(
        remote_track: Arc<webrtc::track::track_remote::TrackRemote>,
        track_id: String,
        source_peer_id: String,
        track_manager: Arc<TrackManager>,
        _peer_connection: Arc<RTCPeerConnection>,
    ) {
        tokio::spawn(async move {
            let mut rtp_buf = vec![0u8; 1500];
            let mut packet_count = 0u64;

            loop {
                match remote_track.read(&mut rtp_buf).await {
                    Ok((rtp_packet, _)) => {
                        packet_count += 1;

                        if packet_count <= 5 {
                            tracing::debug!(
                                track_id = %track_id,
                                packet_count = packet_count,
                                "Forwarding packet for track"
                            );
                        }

                        if let Some(forwarded_track) = track_manager.get_track(&track_id).await {
                            for (target_peer_id, local_track) in &forwarded_track.local_tracks {
                                if target_peer_id != &source_peer_id {
                                    if let Err(e) = local_track.write_rtp(&rtp_packet).await {
                                        if packet_count <= 5 {
                                            tracing::warn!(
                                                target_peer_id = %target_peer_id,
                                                error = %e,
                                                "Failed to forward RTP to peer"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            track_id = %track_id,
                            error = %e,
                            "Failed to read RTP packet for track"
                        );
                        break;
                    }
                }
            }

            tracing::info!(
                track_id = %track_id,
                packet_count = packet_count,
                "Stopped forwarding track"
            );
        });
    }

    pub async fn add_existing_tracks(
        &self,
        track_manager: Arc<TrackManager>,
        existing_track_ids: Vec<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for track_id in existing_track_ids {
            if let Some(local_track) = track_manager
                .create_local_track_for_peer(&track_id, &self.peer_id)
                .await
            {
                self.peer_connection.add_track(local_track).await?;
                tracing::info!(
                    track_id = %track_id,
                    peer_id = %self.peer_id,
                    "Added existing track to peer"
                );
            }
        }
        Ok(())
    }

    pub async fn send_message(&self, message: Message) -> Result<(), mpsc::error::SendError<Message>> {
        self.sender.send(message)
    }

    pub async fn close(&self) {
        let _ = self.peer_connection.close().await;
    }
}