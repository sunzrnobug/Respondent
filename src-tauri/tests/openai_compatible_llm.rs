use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use respondent_lib::llm::client::LlmError;
use respondent_lib::llm::client::{
    ReplyEvent, ReplyGeneration, ReplyPoll, ReplyRequest, StreamingReplyClient,
};
use respondent_lib::llm::openai_compatible::{
    build_chat_body, chat_map, join_chat_url, ChatTransport, OpenAiCompatibleReplyClient,
    ProviderConfig,
};
use respondent_lib::llm::streaming::{ReplyChunk, SseValueStream};
use serde_json::{json, Value};

fn config() -> ProviderConfig {
    ProviderConfig {
        base_url: "https://example.test/v1".into(),
        api_key: "secret-key".into(),
        model: "test-model".into(),
    }
}

fn request() -> ReplyRequest {
    ReplyRequest {
        session_id: "s1".into(),
        generation_id: "gen-1".into(),
        transcript: "What next?".into(),
        context: vec!["What next?".into()],
    }
}

#[test]
fn join_chat_url_handles_trailing_slash() {
    assert_eq!(
        join_chat_url("https://x/v1"),
        "https://x/v1/chat/completions"
    );
    assert_eq!(
        join_chat_url("https://x/v1/"),
        "https://x/v1/chat/completions"
    );
}

#[test]
fn build_chat_body_has_stream_model_messages() {
    let body = build_chat_body(&config(), &request());
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["stream"], true);
    let messages = body["messages"].as_array().expect("messages");
    assert_eq!(messages[0]["role"], "system");
    assert!(messages[1]["content"]
        .as_str()
        .unwrap()
        .contains("What next?"));
}

#[test]
fn chat_map_tolerates_provider_quirks() {
    assert!(matches!(
        chat_map(&json!({"choices":[{"delta":{"content":"hi"}}]})),
        ReplyChunk::Token(t) if t == "hi"
    ));
    // role-only / missing content
    assert!(matches!(
        chat_map(&json!({"choices":[{"delta":{"role":"assistant"}}]})),
        ReplyChunk::Ignore
    ));
    // null content
    assert!(matches!(
        chat_map(&json!({"choices":[{"delta":{"content":null}}]})),
        ReplyChunk::Ignore
    ));
    // reasoning content must not pollute the reply
    assert!(matches!(
        chat_map(&json!({"choices":[{"delta":{"reasoning_content":"thinking"}}]})),
        ReplyChunk::Ignore
    ));
    // trailing usage chunk with empty choices
    assert!(matches!(
        chat_map(&json!({"choices":[],"usage":{"total_tokens":5}})),
        ReplyChunk::Ignore
    ));
    // top-level error
    assert!(matches!(
        chat_map(&json!({"error":{"message":"bad key"}})),
        ReplyChunk::Error
    ));
}

#[test]
fn compatible_client_streams_tokens_and_final() {
    let client = OpenAiCompatibleReplyClient::with_transport(
        config(),
        Arc::new(FakeChatTransport::new(vec![
            json!({"choices":[{"delta":{"role":"assistant"}}]}),
            json!({"choices":[{"delta":{"content":"Hi "}}]}),
            json!({"choices":[{"delta":{"content":"there."}}]}),
            json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
        ])),
    )
    .expect("client");

    let events = collect(client.start(request()));
    assert!(matches!(events.first(), Some(ReplyEvent::Started { .. })));
    let tokens: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            ReplyEvent::Token { token, .. } => Some(token.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(tokens, ["Hi ", "there."]);
    assert!(matches!(events.last(), Some(ReplyEvent::Final { text, .. }) if text == "Hi there."));
}

#[test]
fn compatible_client_error_does_not_leak_key() {
    let client = OpenAiCompatibleReplyClient::with_transport(
        config(),
        Arc::new(FakeChatTransport::new(vec![
            json!({"error":{"message":"secret-key is invalid"}}),
        ])),
    )
    .expect("client");
    let events = collect(client.start(request()));
    let final_text = events
        .iter()
        .find_map(|e| match e {
            ReplyEvent::Final { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .expect("final");
    assert!(final_text.contains("Reply generation failed"));
    assert!(!final_text.contains("secret-key"));
}

#[test]
fn with_transport_rejects_empty_api_key() {
    let mut cfg = config();
    cfg.api_key = "".into();
    let result =
        OpenAiCompatibleReplyClient::with_transport(cfg, Arc::new(FakeChatTransport::new(vec![])));
    assert!(result.is_err());
}

#[test]
fn with_transport_rejects_empty_base_url() {
    let mut cfg = config();
    cfg.base_url = "".into();
    let result =
        OpenAiCompatibleReplyClient::with_transport(cfg, Arc::new(FakeChatTransport::new(vec![])));
    assert!(result.is_err());
}

fn collect(mut gen: Box<dyn ReplyGeneration>) -> Vec<ReplyEvent> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut out = Vec::new();
    while Instant::now() < deadline {
        match gen.poll() {
            ReplyPoll::Event(e) => out.push(e),
            ReplyPoll::Pending => thread::sleep(Duration::from_millis(2)),
            ReplyPoll::Done => return out,
        }
    }
    panic!("timed out");
}

struct FakeChatTransport {
    events: Arc<Mutex<VecDeque<Value>>>,
}
impl FakeChatTransport {
    fn new(events: Vec<Value>) -> Self {
        Self {
            events: Arc::new(Mutex::new(events.into())),
        }
    }
}
impl ChatTransport for FakeChatTransport {
    fn stream(
        &self,
        _config: &ProviderConfig,
        _request: &ReplyRequest,
    ) -> Result<Box<dyn SseValueStream>, LlmError> {
        Ok(Box::new(FakeChatStream {
            events: Arc::clone(&self.events),
        }))
    }
}
struct FakeChatStream {
    events: Arc<Mutex<VecDeque<Value>>>,
}
impl SseValueStream for FakeChatStream {
    fn next_value(&mut self) -> Result<Option<Value>, LlmError> {
        Ok(self.events.lock().expect("lock").pop_front())
    }
}
