use std::collections::VecDeque;

use super::client::{
    ReplyEvent, ReplyGeneration, ReplyPoll, ReplyRequest, StreamingReplyClient,
};

pub struct MockReplyClient;

impl StreamingReplyClient for MockReplyClient {
    fn name(&self) -> &'static str {
        "mock-llm"
    }

    fn start(&self, request: ReplyRequest) -> Box<dyn ReplyGeneration> {
        Box::new(MockReplyGeneration::new(request))
    }
}

/// Deterministic pull-based generation: a fixed acknowledgement of the
/// transcript, streamed as Started -> Token(s) -> Final, then Done.
pub struct MockReplyGeneration {
    queue: VecDeque<ReplyEvent>,
}

impl MockReplyGeneration {
    fn new(request: ReplyRequest) -> Self {
        let ReplyRequest {
            session_id,
            generation_id,
            transcript,
            ..
        } = request;

        let summary = transcript
            .split_ascii_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");
        let tokens = vec!["Acknowledged: ".to_string(), summary];
        let full_text = tokens.concat();

        let mut queue = VecDeque::new();
        let mut clock: i64 = 0;
        queue.push_back(ReplyEvent::Started {
            session_id: session_id.clone(),
            generation_id: generation_id.clone(),
            based_on_transcript_event_id: format!("transcript-{generation_id}"),
            received_at_ms: clock,
        });
        for token in tokens {
            clock += 10;
            queue.push_back(ReplyEvent::Token {
                session_id: session_id.clone(),
                generation_id: generation_id.clone(),
                token,
                received_at_ms: clock,
            });
        }
        clock += 10;
        queue.push_back(ReplyEvent::Final {
            session_id,
            generation_id,
            text: full_text,
            received_at_ms: clock,
        });

        Self { queue }
    }
}

impl ReplyGeneration for MockReplyGeneration {
    fn poll(&mut self) -> ReplyPoll {
        match self.queue.pop_front() {
            Some(event) => ReplyPoll::Event(event),
            None => ReplyPoll::Done,
        }
    }
}
