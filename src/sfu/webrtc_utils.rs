use std::sync::Arc;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::{APIBuilder, API};
use webrtc::ice::network_type::NetworkType;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTCRtpCodecParameters, RTPCodecType};
use webrtc::rtp_transceiver::RTCPFeedback;

pub struct WebRTCConfig {
    pub stun_servers: Vec<String>,
    pub turn_servers: Vec<TurnServer>,
}

pub struct TurnServer {
    pub urls: Vec<String>,
    pub username: String,
    pub credential: String,
}

impl Default for WebRTCConfig {
    fn default() -> Self {
        let stun_server = std::env::var("STUN_SERVER_URL")
            .unwrap_or_else(|_| "stun:stun.l.google.com:19302".to_string());

        let mut turn_servers = vec![];

        // Check for optional TURN server configuration
        if let (Ok(turn_url), Ok(username), Ok(credential)) = (
            std::env::var("TURN_SERVER_URL"),
            std::env::var("TURN_USERNAME"),
            std::env::var("TURN_CREDENTIAL")
        ) {
            turn_servers.push(TurnServer {
                urls: vec![turn_url],
                username,
                credential,
            });
        }

        Self {
            stun_servers: vec![stun_server],
            turn_servers,
        }
    }
}

pub fn create_webrtc_api() -> Arc<API> {
    let mut media_engine = MediaEngine::default();

    // RTCP feedback mechanisms for video - critical for keyframe recovery
    let video_rtcp_feedback = vec![
        RTCPFeedback {
            typ: "goog-remb".to_string(),
            parameter: "".to_string(),
        },
        RTCPFeedback {
            typ: "ccm".to_string(),
            parameter: "fir".to_string(),
        },
        RTCPFeedback {
            typ: "nack".to_string(),
            parameter: "".to_string(),
        },
        RTCPFeedback {
            typ: "nack".to_string(),
            parameter: "pli".to_string(),
        },
    ];

    media_engine
        .register_codec(
            RTCRtpCodecParameters {
                capability: RTCRtpCodecCapability {
                    mime_type: "video/VP8".to_string(),
                    clock_rate: 90000,
                    channels: 0,
                    sdp_fmtp_line: "".to_string(),
                    rtcp_feedback: video_rtcp_feedback,
                },
                payload_type: 96,
                ..Default::default()
            },
            RTPCodecType::Video,
        )
        .expect("Failed to register VP8 codec");

    media_engine
        .register_codec(
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
        )
        .expect("Failed to register Opus codec");

    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)
        .expect("Failed to register default interceptors");

    // Configure SettingEngine to use IPv4 only to avoid IPv6 binding errors
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_network_types(vec![NetworkType::Udp4, NetworkType::Tcp4]);

    // Disable mDNS to reduce unnecessary warnings
    setting_engine.set_ice_multicast_dns_mode(webrtc::ice::mdns::MulticastDnsMode::Disabled);

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_setting_engine(setting_engine)
        .build();

    Arc::new(api)
}

pub fn get_ice_servers(config: &WebRTCConfig) -> Vec<RTCIceServer> {
    let mut ice_servers = Vec::new();

    for stun_server in &config.stun_servers {
        ice_servers.push(RTCIceServer {
            urls: vec![stun_server.clone()],
            ..Default::default()
        });
    }

    for turn_server in &config.turn_servers {
        ice_servers.push(RTCIceServer {
            urls: turn_server.urls.clone(),
            username: turn_server.username.clone(),
            credential: turn_server.credential.clone(),
            credential_type: webrtc::ice_transport::ice_credential_type::RTCIceCredentialType::Password,
        });
    }

    ice_servers
}