use respondent_lib::asr::client::AsrError;
use respondent_lib::asr::client::AsrEvent;
use respondent_lib::asr::client::StreamingAsrClient;
use respondent_lib::asr::mock::MockAsrClient;
use respondent_lib::asr::openai_realtime::{
    OpenAiRealtimeAsrClient, OpenAiRealtimeConfig, RealtimeTransport,
};
use respondent_lib::llm::client::{LlmError, ReplyEvent, ReplyRequest, StreamingReplyClient};
use respondent_lib::llm::mock::MockReplyClient;
use respondent_lib::llm::openai_responses::{
    OpenAiReplyClient, OpenAiReplyConfig, ResponsesEventStream, ResponsesTransport,
};
use serde_json::Value;
use std::sync::Arc;

// The Rust provider events must serialize to the exact wire shape the
// frontend's isRealtimeEvent guard accepts: a "type" tag plus camelCase
// fields (sessionId, receivedAtMs, ...). snake_case would be rejected.

struct ContractTransport;
struct ContractResponsesTransport;
struct EmptyResponsesStream;

impl RealtimeTransport for ContractTransport {
    fn send_json(&mut self, _value: Value) -> Result<(), AsrError> {
        Ok(())
    }

    fn try_recv_json(&mut self) -> Result<Option<Value>, AsrError> {
        Ok(None)
    }

    fn close(&mut self) -> Result<(), AsrError> {
        Ok(())
    }
}

impl ResponsesTransport for ContractResponsesTransport {
    fn stream(
        &self,
        _config: &OpenAiReplyConfig,
        _request: &ReplyRequest,
    ) -> Result<Box<dyn ResponsesEventStream>, LlmError> {
        Ok(Box::new(EmptyResponsesStream))
    }
}

impl ResponsesEventStream for EmptyResponsesStream {
    fn next_event(&mut self) -> Result<Option<Value>, LlmError> {
        Ok(None)
    }
}

#[test]
fn asr_partial_serializes_to_frontend_contract() {
    let event = AsrEvent::Partial {
        session_id: "s1".into(),
        text: "hello".into(),
        started_at_ms: 0,
        ended_at_ms: 320,
        received_at_ms: 350,
    };
    let value: serde_json::Value = serde_json::to_value(&event).expect("serialize");

    assert_eq!(value["type"], "transcript.partial");
    assert_eq!(value["sessionId"], "s1");
    assert_eq!(value["startedAtMs"], 0);
    assert_eq!(value["endedAtMs"], 320);
    assert_eq!(value["receivedAtMs"], 350);
    assert!(
        value.get("session_id").is_none(),
        "must not emit snake_case"
    );
}

#[test]
fn reply_token_serializes_to_frontend_contract() {
    let event = ReplyEvent::Token {
        session_id: "s1".into(),
        generation_id: "g1".into(),
        token: "Yes".into(),
        received_at_ms: 500,
    };
    let value: serde_json::Value = serde_json::to_value(&event).expect("serialize");

    assert_eq!(value["type"], "reply.token");
    assert_eq!(value["sessionId"], "s1");
    assert_eq!(value["generationId"], "g1");
    assert_eq!(value["token"], "Yes");
    assert_eq!(value["receivedAtMs"], 500);
}

#[test]
fn mock_clients_report_their_names() {
    assert_eq!(MockAsrClient::new("s1").name(), "mock-asr");
    let openai = OpenAiRealtimeAsrClient::with_transport(
        "s1".to_string(),
        OpenAiRealtimeConfig::from_api_key("test-key"),
        Box::new(ContractTransport),
    )
    .expect("openai client");
    assert_eq!(openai.name(), "openai-realtime-asr");
    assert_eq!(MockReplyClient.name(), "mock-llm");
    let openai_reply = OpenAiReplyClient::with_transport(
        OpenAiReplyConfig::from_api_key("test-key"),
        Arc::new(ContractResponsesTransport),
    )
    .expect("openai reply client");
    assert_eq!(openai_reply.name(), "openai-responses-llm");
}
