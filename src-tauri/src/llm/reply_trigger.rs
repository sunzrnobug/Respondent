use crate::asr::client::AsrEvent;

use super::client::ReplyRequest;

const MAX_CONTEXT_TURNS: usize = 6;

/// Endpoint-triggered reply policy (ports the frontend replyEngine): a reply
/// is requested only on a `Final` that follows an `Endpoint`, carrying a
/// rolling window of recent final turns as context.
///
/// The emitted `ReplyRequest.context` includes the current turn as its final
/// element (matching the frontend), and `ReplyRequest.transcript` is that same
/// current turn. Consumers should treat `context` as history-including-current
/// and must not re-append `transcript` to it.
pub struct ReplyTrigger {
    session_id: String,
    endpoint_armed: bool,
    context: Vec<String>,
    generation_counter: u64,
}

impl ReplyTrigger {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            endpoint_armed: false,
            context: Vec::new(),
            generation_counter: 0,
        }
    }

    pub fn observe(&mut self, event: &AsrEvent) -> Option<ReplyRequest> {
        match event {
            AsrEvent::Endpoint { .. } => {
                self.endpoint_armed = true;
                None
            }
            AsrEvent::Final { text, .. } => {
                self.context.push(text.clone());
                while self.context.len() > MAX_CONTEXT_TURNS {
                    self.context.remove(0);
                }
                if self.endpoint_armed {
                    self.endpoint_armed = false;
                    self.generation_counter += 1;
                    Some(ReplyRequest {
                        session_id: self.session_id.clone(),
                        generation_id: format!("gen-{}", self.generation_counter),
                        transcript: text.clone(),
                        context: self.context.clone(),
                    })
                } else {
                    None
                }
            }
            AsrEvent::Partial { .. } => None,
        }
    }
}
