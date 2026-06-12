use crossbeam_channel::Receiver;
use serde::Serialize;

use crate::audio::frame::AudioFrame;

/// Streaming ASR events. The wire shape mirrors the frontend RealtimeEvent
/// contract: an internally tagged "type" plus camelCase fields.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum AsrEvent {
    #[serde(rename = "transcript.partial")]
    Partial {
        session_id: String,
        text: String,
        started_at_ms: i64,
        ended_at_ms: i64,
        received_at_ms: i64,
    },
    #[serde(rename = "transcript.final")]
    Final {
        session_id: String,
        text: String,
        started_at_ms: i64,
        ended_at_ms: i64,
        received_at_ms: i64,
    },
    #[serde(rename = "endpoint.detected")]
    Endpoint {
        session_id: String,
        silence_ms: i64,
        detected_at_ms: i64,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("asr stream closed")]
    Closed,
    #[error("asr provider error: {0}")]
    Provider(String),
}

/// A streaming ASR session. One instance serves one transcription session.
/// `events()` carries only `Partial`/`Final`; `Endpoint` is produced by the
/// orchestration's local endpointer, not the ASR client.
pub trait StreamingAsrClient: Send {
    fn name(&self) -> &'static str;
    /// Feed one 16 kHz/mono/i16 audio frame; may produce partials via events().
    fn push_frame(&mut self, frame: &AudioFrame) -> Result<(), AsrError>;
    /// The event stream for this session (clonable).
    fn events(&self) -> Receiver<AsrEvent>;
    /// Close the current utterance, producing a `Final` and arming the next.
    fn finalize(&mut self) -> Result<(), AsrError>;
}
