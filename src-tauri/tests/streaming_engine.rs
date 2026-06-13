use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use respondent_lib::llm::client::{LlmError, ReplyEvent, ReplyGeneration, ReplyPoll, ReplyRequest};
use respondent_lib::llm::streaming::{spawn_streaming_reply, ReplyChunk, SseValueStream};
use serde_json::{json, Value};

fn request() -> ReplyRequest {
    ReplyRequest {
        session_id: "s1".into(),
        generation_id: "gen-1".into(),
        transcript: "hi".into(),
        context: vec!["hi".into()],
        document_context: None,
    }
}

struct VecStream {
    items: std::collections::VecDeque<Value>,
}
impl SseValueStream for VecStream {
    fn next_value(&mut self) -> Result<Option<Value>, LlmError> {
        Ok(self.items.pop_front())
    }
}

fn collect(mut gen: Box<dyn ReplyGeneration>) -> Vec<ReplyEvent> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut out = Vec::new();
    while Instant::now() < deadline {
        match gen.poll() {
            ReplyPoll::Event(e) => out.push(e),
            ReplyPoll::Pending => std::thread::sleep(Duration::from_millis(2)),
            ReplyPoll::Done => return out,
        }
    }
    panic!("timed out");
}

#[test]
fn engine_emits_started_tokens_final_from_chunks() {
    let items = vec![json!({"t": "a"}), json!({"t": "b"}), json!({"done": true})];
    let gen = spawn_streaming_reply(
        request(),
        move || {
            Ok(Box::new(VecStream {
                items: items.into(),
            }) as Box<dyn SseValueStream>)
        },
        |v: &Value| {
            if v["done"].as_bool() == Some(true) {
                ReplyChunk::Complete
            } else if let Some(t) = v["t"].as_str() {
                ReplyChunk::Token(t.to_string())
            } else {
                ReplyChunk::Ignore
            }
        },
    );
    let events = collect(gen);
    assert!(matches!(events.first(), Some(ReplyEvent::Started { .. })));
    let tokens: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            ReplyEvent::Token { token, .. } => Some(token.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(tokens, ["a", "b"]);
    assert!(matches!(events.last(), Some(ReplyEvent::Final { text, .. }) if text == "ab"));
}

#[test]
fn engine_stops_streaming_when_generation_dropped() {
    // Infinite token stream that counts how many times it is pulled.
    struct Counting(Arc<AtomicUsize>);
    impl SseValueStream for Counting {
        fn next_value(&mut self) -> Result<Option<Value>, LlmError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(1));
            Ok(Some(json!({"t": "x"})))
        }
    }
    let pulls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&pulls);
    let mut gen = spawn_streaming_reply(
        request(),
        move || Ok(Box::new(Counting(counter)) as Box<dyn SseValueStream>),
        |v: &Value| match v["t"].as_str() {
            Some(t) => ReplyChunk::Token(t.to_string()),
            None => ReplyChunk::Ignore,
        },
    );
    // Pull a couple events then drop the generation.
    let _ = gen.poll();
    std::thread::sleep(Duration::from_millis(20));
    drop(gen);
    std::thread::sleep(Duration::from_millis(30));
    let after_drop = pulls.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(60));
    let later = pulls.load(Ordering::SeqCst);
    assert_eq!(
        after_drop, later,
        "worker must stop pulling after the generation is dropped"
    );
}
