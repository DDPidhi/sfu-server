use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::error::SfuError;
use super::state::RecordingState;

pub struct RecordingPipeline {
    pipeline: gst::Pipeline,
    video_appsrc: Option<gst_app::AppSrc>,
    audio_appsrc: Option<gst_app::AppSrc>,
    output_path: PathBuf,
    state: Arc<Mutex<RecordingState>>,
}

impl RecordingPipeline {
    pub fn new(room_id: &str, peer_id: &str, output_dir: &str) -> Result<Self, SfuError> {
        gst::init().map_err(|e| SfuError::Internal(format!("GStreamer init failed: {}", e)))?;

        // Create nested directory structure: recordings/{room_id}/
        let room_dir = PathBuf::from(output_dir).join(room_id);
        std::fs::create_dir_all(&room_dir)
            .map_err(|e| SfuError::Internal(format!("Failed to create recording directory: {}", e)))?;

        // Generate timestamp for unique filename per session
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);

        // Output file: recordings/{room_id}/{peer_id}_{timestamp}.webm
        let output_path = room_dir.join(format!("{}_{}.webm", peer_id, timestamp));

        let pipeline = gst::Pipeline::new();

        // Video branch: appsrc -> rtpvp8depay -> vp8dec -> vp8enc -> webmmux
        let video_appsrc = gst::ElementFactory::make("appsrc")
            .name("video_src")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create video appsrc: {}", e)))?;

        let video_appsrc = video_appsrc
            .dynamic_cast::<gst_app::AppSrc>()
            .map_err(|_| SfuError::Internal("Failed to cast to AppSrc".into()))?;

        // Configure video appsrc for RTP VP8
        video_appsrc.set_format(gst::Format::Time);
        video_appsrc.set_is_live(true);
        video_appsrc.set_do_timestamp(true);

        let video_caps = gst::Caps::builder("application/x-rtp")
            .field("media", "video")
            .field("encoding-name", "VP8")
            .field("clock-rate", 90000i32)
            .field("payload", 96i32)
            .build();
        video_appsrc.set_caps(Some(&video_caps));

        let rtpvp8depay = gst::ElementFactory::make("rtpvp8depay")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create rtpvp8depay: {}", e)))?;

        let vp8dec = gst::ElementFactory::make("vp8dec")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create vp8dec: {}", e)))?;

        let videoconvert = gst::ElementFactory::make("videoconvert")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create videoconvert: {}", e)))?;

        let vp8enc = gst::ElementFactory::make("vp8enc")
            .property("deadline", 1i64)
            .property("cpu-used", 4i32)
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create vp8enc: {}", e)))?;

        // Audio branch: appsrc -> rtpopusdepay -> opusdec -> opusenc -> webmmux
        let audio_appsrc = gst::ElementFactory::make("appsrc")
            .name("audio_src")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create audio appsrc: {}", e)))?;

        let audio_appsrc = audio_appsrc
            .dynamic_cast::<gst_app::AppSrc>()
            .map_err(|_| SfuError::Internal("Failed to cast to AppSrc".into()))?;

        // Configure audio appsrc for RTP Opus
        audio_appsrc.set_format(gst::Format::Time);
        audio_appsrc.set_is_live(true);
        audio_appsrc.set_do_timestamp(true);

        let audio_caps = gst::Caps::builder("application/x-rtp")
            .field("media", "audio")
            .field("encoding-name", "OPUS")
            .field("clock-rate", 48000i32)
            .field("payload", 111i32)
            .build();
        audio_appsrc.set_caps(Some(&audio_caps));

        let rtpopusdepay = gst::ElementFactory::make("rtpopusdepay")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create rtpopusdepay: {}", e)))?;

        let opusdec = gst::ElementFactory::make("opusdec")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create opusdec: {}", e)))?;

        let audioconvert = gst::ElementFactory::make("audioconvert")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create audioconvert: {}", e)))?;

        let opusenc = gst::ElementFactory::make("opusenc")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create opusenc: {}", e)))?;

        // Muxer and sink
        let webmmux = gst::ElementFactory::make("webmmux")
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create webmmux: {}", e)))?;

        let filesink = gst::ElementFactory::make("filesink")
            .property("location", output_path.to_str().unwrap())
            .build()
            .map_err(|e| SfuError::Internal(format!("Failed to create filesink: {}", e)))?;

        // Add all elements to pipeline
        pipeline.add_many([
            video_appsrc.upcast_ref(),
            &rtpvp8depay,
            &vp8dec,
            &videoconvert,
            &vp8enc,
            audio_appsrc.upcast_ref(),
            &rtpopusdepay,
            &opusdec,
            &audioconvert,
            &opusenc,
            &webmmux,
            &filesink,
        ]).map_err(|e| SfuError::Internal(format!("Failed to add elements: {}", e)))?;

        // Link video branch
        gst::Element::link_many([
            video_appsrc.upcast_ref(),
            &rtpvp8depay,
            &vp8dec,
            &videoconvert,
            &vp8enc,
        ]).map_err(|e| SfuError::Internal(format!("Failed to link video elements: {}", e)))?;

        // Link audio branch
        gst::Element::link_many([
            audio_appsrc.upcast_ref(),
            &rtpopusdepay,
            &opusdec,
            &audioconvert,
            &opusenc,
        ]).map_err(|e| SfuError::Internal(format!("Failed to link audio elements: {}", e)))?;

        // Link to muxer using request pads
        let video_pad = webmmux.request_pad_simple("video_%u")
            .ok_or_else(|| SfuError::Internal("Failed to get video pad".into()))?;
        let vp8enc_src = vp8enc.static_pad("src")
            .ok_or_else(|| SfuError::Internal("Failed to get vp8enc src pad".into()))?;
        vp8enc_src.link(&video_pad)
            .map_err(|e| SfuError::Internal(format!("Failed to link video to mux: {}", e)))?;

        let audio_pad = webmmux.request_pad_simple("audio_%u")
            .ok_or_else(|| SfuError::Internal("Failed to get audio pad".into()))?;
        let opusenc_src = opusenc.static_pad("src")
            .ok_or_else(|| SfuError::Internal("Failed to get opusenc src pad".into()))?;
        opusenc_src.link(&audio_pad)
            .map_err(|e| SfuError::Internal(format!("Failed to link audio to mux: {}", e)))?;

        // Link muxer to filesink
        webmmux.link(&filesink)
            .map_err(|e| SfuError::Internal(format!("Failed to link mux to sink: {}", e)))?;

        tracing::info!(
            room_id = %room_id,
            peer_id = %peer_id,
            output_path = %output_path.display(),
            "Created recording pipeline"
        );

        Ok(Self {
            pipeline,
            video_appsrc: Some(video_appsrc),
            audio_appsrc: Some(audio_appsrc),
            output_path,
            state: Arc::new(Mutex::new(RecordingState::Idle)),
        })
    }

    pub async fn start(&self) -> Result<(), SfuError> {
        let mut state = self.state.lock().await;
        if *state != RecordingState::Idle {
            return Err(SfuError::Internal("Recording already started".into()));
        }

        self.pipeline.set_state(gst::State::Playing)
            .map_err(|e| SfuError::Internal(format!("Failed to start pipeline: {}", e)))?;

        *state = RecordingState::Recording;
        tracing::info!("Recording started: {:?}", self.output_path);
        Ok(())
    }

    pub async fn stop(&self) -> Result<PathBuf, SfuError> {
        let mut state = self.state.lock().await;
        if *state != RecordingState::Recording {
            return Err(SfuError::Internal("Recording not in progress".into()));
        }

        *state = RecordingState::Stopping;

        // Send EOS to appsrcs
        if let Some(ref video_src) = self.video_appsrc {
            let _ = video_src.end_of_stream();
        }
        if let Some(ref audio_src) = self.audio_appsrc {
            let _ = audio_src.end_of_stream();
        }

        // Wait for EOS on bus
        let bus = self.pipeline.bus().unwrap();
        for msg in bus.iter_timed(gst::ClockTime::from_seconds(5)) {
            if let gst::MessageView::Eos(_) = msg.view() {
                break;
            }
        }

        self.pipeline.set_state(gst::State::Null)
            .map_err(|e| SfuError::Internal(format!("Failed to stop pipeline: {}", e)))?;

        *state = RecordingState::Stopped;
        tracing::info!("Recording stopped: {:?}", self.output_path);
        Ok(self.output_path.clone())
    }

    pub fn push_video_rtp(&self, data: &[u8]) -> Result<(), SfuError> {
        if let Some(ref appsrc) = self.video_appsrc {
            let buffer = gst::Buffer::from_slice(data.to_vec());
            appsrc.push_buffer(buffer)
                .map_err(|e| SfuError::Internal(format!("Failed to push video: {}", e)))?;
        }
        Ok(())
    }

    pub fn push_audio_rtp(&self, data: &[u8]) -> Result<(), SfuError> {
        if let Some(ref appsrc) = self.audio_appsrc {
            let buffer = gst::Buffer::from_slice(data.to_vec());
            appsrc.push_buffer(buffer)
                .map_err(|e| SfuError::Internal(format!("Failed to push audio: {}", e)))?;
        }
        Ok(())
    }

    pub async fn get_state(&self) -> RecordingState {
        self.state.lock().await.clone()
    }

    pub fn output_path(&self) -> &PathBuf {
        &self.output_path
    }
}
