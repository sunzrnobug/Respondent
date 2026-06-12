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
