use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use respondent_lib::llm::client::{
    LlmError, ReplyEvent, ReplyGeneration, ReplyPoll, ReplyRequest, StreamingReplyClient,
};
use respondent_lib::llm::openai_responses::{
    build_responses_body, OpenAiReplyClient, OpenAiReplyConfig, ResponsesEventStream,
    ResponsesTransport,
};
use serde_json::{json, Value};

#[test]
fn responses_body_includes_stream_model_context_and_current_turn() {
    let body = build_responses_body(
        &OpenAiReplyConfig::from_api_key("test-key"),
        &ReplyRequest {
            session_id: "s1".into(),
            generation_id: "gen-1".into(),
            transcript: "What should we do next?".into(),
            context: vec!["Earlier context".into(), "What should we do next?".into()],
            document_context: None,
        },
    );

    assert_eq!(body["model"], "gpt-5.4-mini");
    assert_eq!(body["stream"], true);
    let input = body["input"].as_array().expect("input messages");
    let system = input[0]["content"].as_str().unwrap();
    assert!(system.contains("live meeting"));
    assert!(system.contains("answer directly"));
    assert!(system.contains("Do not ask"));
    assert!(input[1]["content"]
        .as_str()
        .unwrap()
        .contains("Earlier context"));
    assert!(input[1]["content"]
        .as_str()
        .unwrap()
        .contains("What should we do next?"));
}

#[test]
fn openai_reply_streams_started_tokens_and_final_from_response_events() {
    let client = OpenAiReplyClient::with_transport(
        OpenAiReplyConfig::from_api_key("test-key"),
        Arc::new(FakeTransport::new(vec![
            json!({"type": "response.output_text.delta", "delta": "I would "}),
            json!({"type": "response.output_text.delta", "delta": "ask for timing."}),
            json!({"type": "response.completed"}),
        ])),
    )
    .expect("client");

    let events = collect_events(client.start(request()));

    assert!(matches!(
        events.first(),
        Some(ReplyEvent::Started {
            generation_id,
            ..
        }) if generation_id == "gen-1"
    ));
    let tokens: Vec<&str> = events.iter().filter_map(token_text).collect();
    assert_eq!(tokens, ["I would ", "ask for timing."]);
    assert!(matches!(
        events.last(),
        Some(ReplyEvent::Final { text, .. }) if text == "I would ask for timing."
    ));
}

#[test]
fn openai_reply_reports_provider_error_without_leaking_api_key() {
    let client = OpenAiReplyClient::with_transport(
        OpenAiReplyConfig::from_api_key("secret-key"),
        Arc::new(FakeTransport::new(vec![
            json!({"type": "response.error", "error": {"message": "secret-key is invalid"}}),
        ])),
    )
    .expect("client");

    let events = collect_events(client.start(request()));
    let final_text = events
        .iter()
        .find_map(|event| match event {
            ReplyEvent::Final { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .expect("final error event");

    assert!(final_text.contains("回复生成失败"));
    assert!(!final_text.contains("secret-key"));
}

fn request() -> ReplyRequest {
    ReplyRequest {
        session_id: "s1".into(),
        generation_id: "gen-1".into(),
        transcript: "What should we do next?".into(),
        context: vec!["What should we do next?".into()],
        document_context: None,
    }
}

fn collect_events(mut generation: Box<dyn ReplyGeneration>) -> Vec<ReplyEvent> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut events = Vec::new();
    while Instant::now() < deadline {
        match generation.poll() {
            ReplyPoll::Event(event) => events.push(event),
            ReplyPoll::Pending => thread::sleep(Duration::from_millis(5)),
            ReplyPoll::Done => return events,
        }
    }
    panic!("timed out waiting for reply generation");
}

fn token_text(event: &ReplyEvent) -> Option<&str> {
    match event {
        ReplyEvent::Token { token, .. } => Some(token.as_str()),
        _ => None,
    }
}

struct FakeTransport {
    events: Arc<Mutex<VecDeque<Value>>>,
}

impl FakeTransport {
    fn new(events: Vec<Value>) -> Self {
        Self {
            events: Arc::new(Mutex::new(events.into())),
        }
    }
}

impl ResponsesTransport for FakeTransport {
    fn stream(
        &self,
        _config: &OpenAiReplyConfig,
        _request: &ReplyRequest,
    ) -> Result<Box<dyn ResponsesEventStream>, LlmError> {
        Ok(Box::new(FakeStream {
            events: Arc::clone(&self.events),
        }))
    }
}

struct FakeStream {
    events: Arc<Mutex<VecDeque<Value>>>,
}

impl ResponsesEventStream for FakeStream {
    fn next_event(&mut self) -> Result<Option<Value>, LlmError> {
        Ok(self.events.lock().expect("events lock").pop_front())
    }
}
