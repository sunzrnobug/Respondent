use respondent_lib::asr::client::AsrEvent;
use respondent_lib::asr::mock::MockAsrClient;
use respondent_lib::asr::client::StreamingAsrClient;
use respondent_lib::llm::client::{ReplyEvent, StreamingReplyClient};
use respondent_lib::llm::mock::MockReplyClient;

// The Rust provider events must serialize to the exact wire shape the
// frontend's isRealtimeEvent guard accepts: a "type" tag plus camelCase
// fields (sessionId, receivedAtMs, ...). snake_case would be rejected.

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
    assert!(value.get("session_id").is_none(), "must not emit snake_case");
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
    assert_eq!(MockReplyClient.name(), "mock-llm");
}
