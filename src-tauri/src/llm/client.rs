use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ReplyRequest {
    pub session_id: String,
    pub generation_id: String,
    pub transcript: String,
    pub context: Vec<String>,
}

/// Streaming reply events. The wire shape mirrors the frontend RealtimeEvent
/// contract: an internally tagged "type" plus camelCase fields.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum ReplyEvent {
    #[serde(rename = "reply.started")]
    Started {
        session_id: String,
        generation_id: String,
        based_on_transcript_event_id: String,
        received_at_ms: i64,
    },
    #[serde(rename = "reply.token")]
    Token {
        session_id: String,
        generation_id: String,
        token: String,
        received_at_ms: i64,
    },
    #[serde(rename = "reply.final")]
    Final {
        session_id: String,
        generation_id: String,
        text: String,
        received_at_ms: i64,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("reply stream closed")]
    Closed,
    #[error("llm provider error: {0}")]
    Provider(String),
}

/// One pull from a `ReplyGeneration`.
#[derive(Debug)]
pub enum ReplyPoll {
    Event(ReplyEvent),
    /// No event yet, but generation is still in progress (real adapters that
    /// await network tokens return this; the mock never does).
    Pending,
    Done,
}

/// A single in-progress reply generation. Pull events with `poll`; dropping
/// the value cancels the generation.
pub trait ReplyGeneration: Send {
    fn poll(&mut self) -> ReplyPoll;
}

pub trait StreamingReplyClient: Send {
    fn name(&self) -> &'static str;
    /// Begin generating a reply for `request`; returns the pull handle.
    fn start(&self, request: ReplyRequest) -> Box<dyn ReplyGeneration>;
}
