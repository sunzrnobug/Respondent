use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::audio::frame::AudioFrame;

use super::client::{AsrError, AsrEvent, StreamingAsrClient};

const PARTIAL_EVERY_FRAMES: u32 = 25; // ~0.5 s at 20 ms frames

pub struct MockAsrClient {
    session_id: String,
    sender: Sender<AsrEvent>,
    receiver: Receiver<AsrEvent>,
    phrases: Vec<&'static str>,
    phrase_index: usize,
    frames_in_utterance: u32,
    partials_emitted: usize,
    utterance_started_at_ms: Option<i64>,
    last_frame_ended_at_ms: i64,
}

impl MockAsrClient {
    pub fn new(session_id: impl Into<String>) -> Self {
        let (sender, receiver) = unbounded();
        Self {
            session_id: session_id.into(),
            sender,
            receiver,
            phrases: vec![
                "could you summarize the timeline",
                "what are the main risks",
                "lets confirm the next steps",
            ],
            phrase_index: 0,
            frames_in_utterance: 0,
            partials_emitted: 0,
            utterance_started_at_ms: None,
            last_frame_ended_at_ms: 0,
        }
    }

    fn current_phrase(&self) -> &'static str {
        self.phrases[self.phrase_index % self.phrases.len()]
    }

    /// Reveals one more word per partial, saturating at the full phrase once
    /// every word has been shown (acceptable for a deterministic test double).
    fn partial_prefix(&self) -> String {
        let words: Vec<&str> = self.current_phrase().split(' ').collect();
        let take = (self.partials_emitted + 1).clamp(1, words.len());
        words[..take].join(" ")
    }
}

impl StreamingAsrClient for MockAsrClient {
    fn name(&self) -> &'static str {
        "mock-asr"
    }

    fn push_frame(&mut self, frame: &AudioFrame) -> Result<(), AsrError> {
        if self.utterance_started_at_ms.is_none() {
            self.utterance_started_at_ms = Some(frame.captured_at_ms as i64);
        }
        self.last_frame_ended_at_ms = frame.captured_at_ms as i64 + frame.duration_ms() as i64;
        self.frames_in_utterance += 1;

        if self.frames_in_utterance % PARTIAL_EVERY_FRAMES == 0 {
            let text = self.partial_prefix();
            self.partials_emitted += 1;
            self.sender
                .send(AsrEvent::Partial {
                    session_id: self.session_id.clone(),
                    text,
                    started_at_ms: self.utterance_started_at_ms.unwrap_or(0),
                    ended_at_ms: self.last_frame_ended_at_ms,
                    received_at_ms: self.last_frame_ended_at_ms,
                })
                .map_err(|_| AsrError::Closed)?;
        }
        Ok(())
    }

    fn events(&self) -> Receiver<AsrEvent> {
        self.receiver.clone()
    }

    fn finalize(&mut self) -> Result<(), AsrError> {
        if self.frames_in_utterance == 0 {
            return Ok(());
        }
        self.sender
            .send(AsrEvent::Final {
                session_id: self.session_id.clone(),
                text: self.current_phrase().to_string(),
                started_at_ms: self.utterance_started_at_ms.unwrap_or(0),
                ended_at_ms: self.last_frame_ended_at_ms,
                received_at_ms: self.last_frame_ended_at_ms,
            })
            .map_err(|_| AsrError::Closed)?;

        self.phrase_index += 1;
        self.frames_in_utterance = 0;
        self.partials_emitted = 0;
        self.utterance_started_at_ms = None;
        Ok(())
    }
}
