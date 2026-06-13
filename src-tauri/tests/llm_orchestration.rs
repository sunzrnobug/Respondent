use respondent_lib::asr::client::AsrEvent;
use respondent_lib::llm::reply_trigger::ReplyTrigger;

fn endpoint() -> AsrEvent {
    AsrEvent::Endpoint {
        session_id: "s1".into(),
        silence_ms: 300,
        detected_at_ms: 0,
    }
}

fn final_event(text: &str) -> AsrEvent {
    AsrEvent::Final {
        session_id: "s1".into(),
        text: text.into(),
        started_at_ms: 0,
        ended_at_ms: 0,
        received_at_ms: 0,
    }
}

fn partial(text: &str) -> AsrEvent {
    AsrEvent::Partial {
        session_id: "s1".into(),
        text: text.into(),
        started_at_ms: 0,
        ended_at_ms: 0,
        received_at_ms: 0,
    }
}

#[test]
fn trigger_fires_on_endpoint_then_final() {
    let mut trigger = ReplyTrigger::new("s1");
    assert!(trigger.observe(&endpoint()).is_none());
    let request = trigger.observe(&final_event("hello there")).expect("a request");
    assert_eq!(request.session_id.as_str(), "s1");
    assert_eq!(request.generation_id.as_str(), "gen-1");
    assert_eq!(request.transcript.as_str(), "hello there");
    assert_eq!(request.context, vec!["hello there".to_string()]);
}

#[test]
fn trigger_ignores_final_without_endpoint() {
    let mut trigger = ReplyTrigger::new("s1");
    assert!(trigger.observe(&final_event("no endpoint yet")).is_none());
}

#[test]
fn trigger_ignores_partials() {
    let mut trigger = ReplyTrigger::new("s1");
    assert!(trigger.observe(&partial("typing")).is_none());
}

#[test]
fn trigger_rolls_context_to_six_and_counts_generations() {
    let mut trigger = ReplyTrigger::new("s1");
    let mut last = None;
    for index in 0..7 {
        trigger.observe(&endpoint());
        last = trigger.observe(&final_event(&format!("turn {index}")));
    }
    let request = last.expect("a request");
    assert_eq!(request.generation_id.as_str(), "gen-7");
    assert_eq!(request.transcript.as_str(), "turn 6");
    assert_eq!(
        request.context,
        vec![
            "turn 1".to_string(),
            "turn 2".to_string(),
            "turn 3".to_string(),
            "turn 4".to_string(),
            "turn 5".to_string(),
            "turn 6".to_string(),
        ]
    );
}

#[test]
fn double_endpoint_does_not_double_fire() {
    let mut trigger = ReplyTrigger::new("s1");
    trigger.observe(&endpoint());
    trigger.observe(&endpoint()); // second endpoint while armed — idempotent
    assert!(trigger.observe(&final_event("once")).is_some());
    assert!(trigger.observe(&final_event("twice")).is_none()); // no second fire
}

use respondent_lib::llm::client::{ReplyEvent, ReplyPoll, ReplyRequest, StreamingReplyClient};
use respondent_lib::llm::mock::MockReplyClient;

#[test]
fn mock_reply_streams_started_tokens_final_then_done() {
    let client = MockReplyClient;
    let mut generation = client.start(ReplyRequest {
        session_id: "s1".into(),
        generation_id: "gen-1".into(),
        transcript: "could you summarize the timeline".into(),
        context: vec!["could you summarize the timeline".into()],
    });

    let mut events = Vec::new();
    loop {
        match generation.poll() {
            ReplyPoll::Event(event) => events.push(event),
            ReplyPoll::Done => break,
            ReplyPoll::Pending => panic!("the mock never pends"),
        }
    }

    match events.first() {
        Some(ReplyEvent::Started { generation_id, session_id, .. }) => {
            assert_eq!(generation_id.as_str(), "gen-1");
            assert_eq!(session_id.as_str(), "s1");
        }
        other => panic!("expected started, got {other:?}"),
    }
    let token_texts: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            ReplyEvent::Token { token, .. } => Some(token.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(token_texts, ["Acknowledged: ", "could you summarize"]);
    match events.last() {
        Some(ReplyEvent::Final { generation_id, text, .. }) => {
            assert_eq!(generation_id.as_str(), "gen-1");
            assert_eq!(text.as_str(), "Acknowledged: could you summarize");
        }
        other => panic!("expected final, got {other:?}"),
    }
    assert!(matches!(generation.poll(), ReplyPoll::Done), "poll after done is stable");
}

use std::time::Duration;

use crossbeam_channel::unbounded;
use respondent_lib::llm::session::ReplySession;

#[test]
fn session_streams_started_tokens_final_for_one_trigger() {
    let (tx, rx) = unbounded();
    let session = ReplySession::start(rx, Box::new(MockReplyClient), ReplyTrigger::new("s1"));
    let events = session.events();

    tx.send(partial("hel")).unwrap();
    tx.send(endpoint()).unwrap();
    tx.send(final_event("hello there")).unwrap();
    drop(tx);

    let mut collected = Vec::new();
    while let Ok(event) = events.recv_timeout(Duration::from_secs(2)) {
        collected.push(event);
    }
    session.stop().unwrap();

    assert!(matches!(collected.first(), Some(ReplyEvent::Started { .. })));
    assert!(collected.iter().any(|event| matches!(event, ReplyEvent::Token { .. })));
    match collected.last() {
        Some(ReplyEvent::Final { generation_id, .. }) => {
            assert_eq!(generation_id.as_str(), "gen-1");
        }
        other => panic!("expected final, got {other:?}"),
    }
}

#[test]
fn session_latest_trigger_wins() {
    let (tx, rx) = unbounded();
    let session = ReplySession::start(rx, Box::new(MockReplyClient), ReplyTrigger::new("s1"));
    let events = session.events();

    // Two triggers queued before the worker pumps; the latest must win.
    tx.send(endpoint()).unwrap();
    tx.send(final_event("first")).unwrap();
    tx.send(endpoint()).unwrap();
    tx.send(final_event("second")).unwrap();
    drop(tx);

    let mut collected = Vec::new();
    while let Ok(event) = events.recv_timeout(Duration::from_secs(2)) {
        collected.push(event);
    }
    session.stop().unwrap();

    let last_final_gen = collected.iter().rev().find_map(|event| match event {
        ReplyEvent::Final { generation_id, .. } => Some(generation_id.clone()),
        _ => None,
    });
    assert_eq!(last_final_gen.as_deref(), Some("gen-2"));

    let gen1_finals = collected
        .iter()
        .filter(|event| {
            matches!(event, ReplyEvent::Final { generation_id, .. } if generation_id.as_str() == "gen-1")
        })
        .count();
    assert_eq!(gen1_finals, 0, "gen-1 was superseded and must not produce a final");
}
