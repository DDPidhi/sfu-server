use std::sync::Arc;
use tokio::sync::mpsc;
use warp::ws::Message;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::{APIBuilder, API};
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType};

pub fn create_api() -> Arc<API> {
    let mut media_engine = MediaEngine::default();


    media_engine.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: "video/VP8".to_string(),
                clock_rate: 90000,
                channels: 0,
                sdp_fmtp_line: "".to_string(),
                rtcp_feedback: vec![],
            },
            payload_type: 96,
            ..Default::default()
        },
        RTPCodecType::Video,
    ).expect("Failed to register VP8");


    media_engine.register_codec(
        RTCRtpCodecParameters {
            capability: RTCRtpCodecCapability {
                mime_type: "audio/opus".to_string(),
                clock_rate: 48000,
                channels: 2,
                sdp_fmtp_line: "minptime=10;useinbandfec=1".to_string(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        },
        RTPCodecType::Audio,
    ).expect("Failed to register Opus");

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)
        .expect("Failed to register interceptors");

    Arc::new(
        APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build(),
    )
}


pub struct BasicSfuConnection {
    pub peer_id: String,
    pub peer_connection: Arc<RTCPeerConnection>,
    pub sender: mpsc::UnboundedSender<Message>,
}

impl BasicSfuConnection {

    pub async fn new(
        peer_id: String,
        sender: mpsc::UnboundedSender<Message>,
        api: &Arc<API>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {


        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_string()],
                ..Default::default()
            }],
            ..Default::default()
        };

        let peer_connection = Arc::new(api.new_peer_connection(config).await?);


        peer_connection.add_transceiver_from_kind(RTPCodecType::Video, None).await?;
        peer_connection.add_transceiver_from_kind(RTPCodecType::Audio, None).await?;


        {
            let _peer_id_clone = peer_id.clone();
            peer_connection.on_track(Box::new(move |track, _receiver, _transceiver| {
                let _track_id = track.id();
                let _track_kind = track.kind();
                Box::pin(async move {})
            }));
        }

        let peer_id_clone = peer_id.clone();
        peer_connection.on_ice_connection_state_change(Box::new(move |state| {
            println!("🧊 ICE state for {}: {}", peer_id_clone, state);
            Box::pin(async {})
        }));


        Ok(Self {
            peer_id,
            peer_connection,
            sender,
        })
    }

    pub async fn create_offer(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let offer = self.peer_connection.create_offer(None).await?;
        self.peer_connection.set_local_description(offer.clone()).await?;

        Ok(offer.sdp)
    }

    pub async fn handle_answer(&self, sdp: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

        let answer = RTCSessionDescription::answer(sdp.to_string())?;
        self.peer_connection.set_remote_description(answer).await?;
        Ok(())
    }

    pub async fn add_ice_candidate(
        &self,
        candidate: &str,
        sdp_mid: Option<String>,
        sdp_mline_index: Option<u16>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let ice_candidate = webrtc::ice_transport::ice_candidate::RTCIceCandidateInit {
            candidate: candidate.to_string(),
            sdp_mid,
            sdp_mline_index,
            username_fragment: None,
        };

        self.peer_connection.add_ice_candidate(ice_candidate).await?;

        Ok(())
    }

    pub async fn send_message(&self, message: Message) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.sender.send(message)?;
        Ok(())
    }

    pub async fn close(&self) {
        println!("🔌 Closing connection for: {}", self.peer_id);
        let _ = self.peer_connection.close().await;
    }
}
