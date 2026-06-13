use std::sync::{Arc, Mutex};

use crate::asr::client::AsrEvent;
use crate::docs::format_document_context;
use crate::docs::store::DocumentStore;
use crate::reply_style_settings::ReplyStyleSettingsStore;

use super::client::ReplyRequest;

const MAX_CONTEXT_TURNS: usize = 6;
const RETRIEVAL_CONTEXT_TURNS: usize = 3;

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
    doc_store: Arc<Mutex<DocumentStore>>,
    style_store: Arc<ReplyStyleSettingsStore>,
}

impl ReplyTrigger {
    pub fn new(
        session_id: impl Into<String>,
        doc_store: Arc<Mutex<DocumentStore>>,
        style_store: Arc<ReplyStyleSettingsStore>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            endpoint_armed: false,
            context: Vec::new(),
            generation_counter: 0,
            doc_store,
            style_store,
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
                    let document_context = self.retrieve_document_context(text);
                    let reply_style = Some(self.style_store.get());
                    Some(ReplyRequest {
                        session_id: self.session_id.clone(),
                        generation_id: format!("gen-{}", self.generation_counter),
                        transcript: text.clone(),
                        context: self.context.clone(),
                        document_context,
                        reply_style,
                    })
                } else {
                    None
                }
            }
            AsrEvent::Partial { .. } => None,
        }
    }

    fn retrieve_document_context(&self, current_transcript: &str) -> Option<String> {
        let store = match self.doc_store.lock() {
            Ok(guard) => guard,
            Err(err) => {
                eprintln!("[reply_trigger] DocumentStore mutex poisoned: {err}");
                return None;
            }
        };
        if store.is_empty() {
            return None;
        }
        let query = build_retrieval_query(&self.context, current_transcript);
        let chunks = store.query(&query, 5);
        format_document_context(&chunks)
    }

    /// Re-run reply generation for the latest final turn without mutating context.
    pub fn retry_last(&mut self) -> Option<ReplyRequest> {
        let transcript = self.context.last()?.clone();
        self.generation_counter += 1;
        let document_context = self.retrieve_document_context(&transcript);
        let reply_style = Some(self.style_store.get());
        Some(ReplyRequest {
            session_id: self.session_id.clone(),
            generation_id: format!("gen-{}", self.generation_counter),
            transcript,
            context: self.context.clone(),
            document_context,
            reply_style,
        })
    }
}

pub fn build_retrieval_query(context: &[String], transcript: &str) -> String {
    let recent: Vec<&str> = context
        .iter()
        .rev()
        .take(RETRIEVAL_CONTEXT_TURNS)
        .rev()
        .map(|s| s.as_str())
        .collect();
    format!("{} {}", recent.join(" "), transcript)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::client::AsrEvent;
    use crate::reply_style_settings::ReplyStyleSettingsStore;
    use std::sync::Arc;

    fn style_store() -> Arc<ReplyStyleSettingsStore> {
        Arc::new(ReplyStyleSettingsStore::with_settings(
            Default::default(),
        ))
    }

    fn armed_trigger_with_auth_doc() -> ReplyTrigger {
        let store = Arc::new(Mutex::new(DocumentStore::default()));
        store.lock().unwrap().load(
            "auth.md".into(),
            "## API Authentication\nUse Bearer token in the Authorization header. The accessToken expires after 1 hour and must be refreshed periodically.\n".into(),
        );
        let mut trigger = ReplyTrigger::new("session-1", store, style_store());
        trigger.observe(&AsrEvent::Endpoint {
            session_id: "session-1".into(),
            silence_ms: 300,
            detected_at_ms: 0,
        });
        trigger
    }

    #[test]
    fn no_request_without_endpoint() {
        let store = Arc::new(Mutex::new(DocumentStore::default()));
        let mut trigger = ReplyTrigger::new("s", store, style_store());
        let result = trigger.observe(&AsrEvent::Final {
            session_id: "s".into(),
            text: "hello".into(),
            started_at_ms: 0,
            ended_at_ms: 100,
            received_at_ms: 0,
        });
        assert!(result.is_none());
    }

    #[test]
    fn request_after_endpoint_includes_doc_context() {
        let mut trigger = armed_trigger_with_auth_doc();
        let request = trigger
            .observe(&AsrEvent::Final {
                session_id: "session-1".into(),
                text: "how do I authenticate the API?".into(),
                started_at_ms: 0,
                ended_at_ms: 1000,
                received_at_ms: 0,
            })
            .expect("expected a ReplyRequest");
        let ctx = request
            .document_context
            .expect("document_context should be Some");
        assert!(
            ctx.contains("Bearer"),
            "expected auth content in context: {ctx}"
        );
    }

    #[test]
    fn no_document_context_when_store_empty() {
        let store = Arc::new(Mutex::new(DocumentStore::default()));
        let mut trigger = ReplyTrigger::new("s", store, style_store());
        trigger.observe(&AsrEvent::Endpoint {
            session_id: "s".into(),
            silence_ms: 300,
            detected_at_ms: 0,
        });
        let request = trigger
            .observe(&AsrEvent::Final {
                session_id: "s".into(),
                text: "authenticate token bearer".into(),
                started_at_ms: 0,
                ended_at_ms: 500,
                received_at_ms: 0,
            })
            .unwrap();
        assert!(request.document_context.is_none());
    }

    #[test]
    fn retry_last_reuses_latest_turn_without_growing_context() {
        let store = Arc::new(Mutex::new(DocumentStore::default()));
        let mut trigger = ReplyTrigger::new("s1", store, style_store());
        trigger.observe(&AsrEvent::Endpoint {
            session_id: "s1".into(),
            silence_ms: 300,
            detected_at_ms: 0,
        });
        trigger
            .observe(&AsrEvent::Final {
                session_id: "s1".into(),
                text: "first question".into(),
                started_at_ms: 0,
                ended_at_ms: 100,
                received_at_ms: 0,
            })
            .expect("first reply");
        trigger.observe(&AsrEvent::Endpoint {
            session_id: "s1".into(),
            silence_ms: 300,
            detected_at_ms: 200,
        });
        trigger
            .observe(&AsrEvent::Final {
                session_id: "s1".into(),
                text: "second question".into(),
                started_at_ms: 100,
                ended_at_ms: 200,
                received_at_ms: 200,
            })
            .expect("second reply");

        let retry = trigger.retry_last().expect("retry request");
        assert_eq!(retry.generation_id, "gen-3");
        assert_eq!(retry.transcript, "second question");
        assert_eq!(
            retry.context,
            vec!["first question".to_string(), "second question".to_string()]
        );
    }

    #[test]
    fn retry_last_returns_none_when_context_empty() {
        let store = Arc::new(Mutex::new(DocumentStore::default()));
        let mut trigger = ReplyTrigger::new("s", store, style_store());
        assert!(trigger.retry_last().is_none());
    }

    #[test]
    fn retrieval_query_includes_recent_context() {
        let context = vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
            "fourth".to_string(),
        ];
        let q = build_retrieval_query(&context, "current");
        assert!(!q.contains("first"), "oldest turn should be excluded: {q}");
        assert!(q.contains("second"));
        assert!(q.contains("fourth"));
        assert!(q.contains("current"));
    }
}
