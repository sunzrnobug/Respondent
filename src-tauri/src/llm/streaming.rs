use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde_json::Value;

use super::client::{LlmError, ReplyEvent, ReplyGeneration, ReplyPoll, ReplyRequest};

pub const GENERIC_FAILURE_TEXT: &str =
    "回复生成失败。请检查 API 密钥、模型或网络连接。";

/// A stream of parsed SSE JSON values; `[DONE]` or EOF yields Ok(None).
pub trait SseValueStream: Send {
    fn next_value(&mut self) -> Result<Option<Value>, LlmError>;
}

/// What a dialect makes of one SSE JSON value.
pub enum ReplyChunk {
    Token(String),
    Complete,
    Error,
    Ignore,
}

/// reqwest-blocking SSE reader shared by all dialects: strips `data:`, treats
/// `[DONE]`/EOF as end, skips comment/blank/non-data lines, parses JSON.
pub struct ReqwestSseStream {
    reader: std::io::BufReader<reqwest::blocking::Response>,
}

impl ReqwestSseStream {
    pub fn new(response: reqwest::blocking::Response) -> Self {
        Self {
            reader: std::io::BufReader::new(response),
        }
    }
}

impl SseValueStream for ReqwestSseStream {
    fn next_value(&mut self) -> Result<Option<Value>, LlmError> {
        use std::io::BufRead;
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = self
                .reader
                .read_line(&mut line)
                .map_err(|err| LlmError::Provider(format!("sse read: {err}")))?;
            if bytes == 0 {
                return Ok(None);
            }
            let trimmed = line.trim();
            let Some(data) = trimmed.strip_prefix("data:") else {
                continue; // comment / blank / event: lines
            };
            let data = data.trim();
            if data.is_empty() {
                continue;
            }
            if data == "[DONE]" {
                return Ok(None);
            }
            let value = serde_json::from_str(data)
                .map_err(|err| LlmError::Provider(format!("sse json: {err}")))?;
            return Ok(Some(value));
        }
    }
}

/// Shared worker: spawns a thread, emits Started, then maps each SSE value via
/// `map`, forwarding Token/Final and assembling the final text. Dropping the
/// returned generation disconnects the channel; the worker then stops pulling
/// the stream (aborting the upstream request).
pub fn spawn_streaming_reply<O, M>(
    request: ReplyRequest,
    open: O,
    map: M,
) -> Box<dyn ReplyGeneration>
where
    O: FnOnce() -> Result<Box<dyn SseValueStream>, LlmError> + Send + 'static,
    M: Fn(&Value) -> ReplyChunk + Send + 'static,
{
    let (sender, receiver) = unbounded();
    let _ = sender.send(ReplyPoll::Event(ReplyEvent::Started {
        session_id: request.session_id.clone(),
        generation_id: request.generation_id.clone(),
        based_on_transcript_event_id: format!("transcript-{}", request.generation_id),
        received_at_ms: now_ms(),
    }));

    thread::Builder::new()
        .name("llm-streaming-reply".into())
        .spawn(move || {
            let session_id = request.session_id.clone();
            let generation_id = request.generation_id.clone();
            let mut final_text = String::new();

            let mut stream = match open() {
                Ok(stream) => stream,
                Err(_) => {
                    finish_failure(&sender, &session_id, &generation_id);
                    return;
                }
            };

            loop {
                match stream.next_value() {
                    Ok(Some(value)) => match map(&value) {
                        ReplyChunk::Token(token) => {
                            final_text.push_str(&token);
                            if send_event(
                                &sender,
                                ReplyEvent::Token {
                                    session_id: session_id.clone(),
                                    generation_id: generation_id.clone(),
                                    token,
                                    received_at_ms: now_ms(),
                                },
                            )
                            .is_err()
                            {
                                return; // consumer dropped -> abort upstream
                            }
                        }
                        ReplyChunk::Complete => {
                            finish_text(&sender, &session_id, &generation_id, final_text);
                            return;
                        }
                        ReplyChunk::Error => {
                            finish_failure(&sender, &session_id, &generation_id);
                            return;
                        }
                        ReplyChunk::Ignore => {}
                    },
                    Ok(None) => {
                        if final_text.is_empty() {
                            finish_failure(&sender, &session_id, &generation_id);
                        } else {
                            finish_text(&sender, &session_id, &generation_id, final_text);
                        }
                        return;
                    }
                    Err(_) => {
                        finish_failure(&sender, &session_id, &generation_id);
                        return;
                    }
                }
            }
        })
        .expect("spawn llm streaming reply worker");

    Box::new(ChannelReplyGeneration {
        receiver,
        done: false,
    })
}

fn send_event(sender: &Sender<ReplyPoll>, event: ReplyEvent) -> Result<(), ()> {
    sender.send(ReplyPoll::Event(event)).map_err(|_| ())
}

fn finish_text(sender: &Sender<ReplyPoll>, session_id: &str, generation_id: &str, text: String) {
    let _ = sender.send(ReplyPoll::Event(ReplyEvent::Final {
        session_id: session_id.to_string(),
        generation_id: generation_id.to_string(),
        text,
        received_at_ms: now_ms(),
    }));
    let _ = sender.send(ReplyPoll::Done);
}

fn finish_failure(sender: &Sender<ReplyPoll>, session_id: &str, generation_id: &str) {
    finish_text(
        sender,
        session_id,
        generation_id,
        GENERIC_FAILURE_TEXT.to_string(),
    );
}

struct ChannelReplyGeneration {
    receiver: Receiver<ReplyPoll>,
    done: bool,
}

impl ReplyGeneration for ChannelReplyGeneration {
    fn poll(&mut self) -> ReplyPoll {
        if self.done {
            return ReplyPoll::Done;
        }
        match self.receiver.try_recv() {
            Ok(ReplyPoll::Done) => {
                self.done = true;
                ReplyPoll::Done
            }
            Ok(poll) => poll,
            Err(crossbeam_channel::TryRecvError::Empty) => ReplyPoll::Pending,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                self.done = true;
                ReplyPoll::Done
            }
        }
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn truncate_for_error(text: &str) -> String {
    const LIMIT: usize = 240;
    let trimmed = text.trim();
    if trimmed.len() <= LIMIT {
        return trimmed.to_string();
    }
    let boundary = trimmed
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= LIMIT)
        .last()
        .unwrap_or(0);
    format!("{}...", &trimmed[..boundary])
}
