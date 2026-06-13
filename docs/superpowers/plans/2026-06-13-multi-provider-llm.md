# Multi-Provider LLM Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add OpenAI-compatible Chat Completions support (DashScope / Zhipu / SiliconFlow / custom) for reply generation by factoring a shared streaming engine reused by the existing OpenAI Responses dialect and a new Chat Completions adapter, selected via `LLM_PROVIDER` + per-provider config.

**Architecture:** Extract the reqwest-blocking SSE reader + worker (thread + channel + `poll()`/Pending + cancellation + ReplyEvent assembly) into `llm/streaming.rs`, parameterized per dialect by (open-stream closure, `Fn(&Value) -> ReplyChunk`). Refactor `openai_responses` onto it without changing its public API or tests. Add `openai_compatible` (Chat Completions) with a `ProviderConfig { base_url, api_key, model }`. Resolve provider/config from env in `commands.rs` only.

**Tech Stack:** Rust, reqwest (blocking), crossbeam-channel, serde_json, thiserror. No new dependencies.

---

## File Structure

- `src-tauri/src/llm/streaming.rs` (create): `SseValueStream` trait, `ReplyChunk` enum, `ReqwestSseStream`, `spawn_streaming_reply`, plus moved helpers `now_ms` / `truncate_for_error` / `GENERIC_FAILURE_TEXT`.
- `src-tauri/src/llm/openai_responses.rs` (modify): reuse `spawn_streaming_reply`; keep public API + `tests/openai_responses_llm.rs` green.
- `src-tauri/src/llm/openai_compatible.rs` (create): `ProviderConfig`, `OpenAiCompatibleReplyClient`, `ChatTransport`, `ReqwestChatTransport`, `build_chat_body`, `chat_map`, `join_chat_url`.
- `src-tauri/src/llm/mod.rs` (modify): add `pub mod openai_compatible; pub mod streaming;`.
- `src-tauri/src/commands.rs` (modify): env resolver `build_reply_client_from_env`; update `reply_provider_name_for_test`.
- `src-tauri/tests/openai_compatible_llm.rs` (create): chat dialect + adapter tests.
- `src-tauri/tests/commands.rs` (modify): provider-selection tests for the new providers.
- `src-tauri/tests/e2e_real_network.rs` (modify): gated per-provider smoke.

Run cargo with PATH prepended (cargo is NOT on default PATH); never run `cargo update`:

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
```

---

## Task 1: Shared Streaming Engine + Responses Refactor

**Files:**
- Create: `src-tauri/src/llm/streaming.rs`
- Modify: `src-tauri/src/llm/openai_responses.rs`
- Modify: `src-tauri/src/llm/mod.rs`
- Create: `src-tauri/tests/streaming_engine.rs`

- [ ] **Step 1: Write a failing engine test (cancellation + mapping)**

Create `src-tauri/tests/streaming_engine.rs`:

```rust
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
    let items = vec![
        json!({"t": "a"}),
        json!({"t": "b"}),
        json!({"done": true}),
    ];
    let gen = spawn_streaming_reply(
        request(),
        move || Ok(Box::new(VecStream { items: items.into() }) as Box<dyn SseValueStream>),
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
    assert_eq!(after_drop, later, "worker must stop pulling after the generation is dropped");
}
```

- [ ] **Step 2: Run to verify RED**

```powershell
cd src-tauri
cargo test --test streaming_engine
cd ..
```

Expected: compile error `unresolved import respondent_lib::llm::streaming`.

- [ ] **Step 3: Create the shared engine**

Create `src-tauri/src/llm/streaming.rs`:

```rust
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde_json::Value;

use super::client::{LlmError, ReplyEvent, ReplyGeneration, ReplyPoll, ReplyRequest};

pub const GENERIC_FAILURE_TEXT: &str =
    "Reply generation failed. Check your API key, model, or network connection.";

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
pub fn spawn_streaming_reply<O, M>(request: ReplyRequest, open: O, map: M) -> Box<dyn ReplyGeneration>
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

// Note: `use std::sync::mpsc::TryRecvError` import above is unused; remove it.
```

(Remove the stray `use std::sync::mpsc::TryRecvError;` line — it is not used; `crossbeam_channel::TryRecvError` is referenced fully-qualified.)

Add to `src-tauri/src/llm/mod.rs`:

```rust
pub mod streaming;
```

- [ ] **Step 4: Refactor `openai_responses.rs` onto the shared engine**

Edit `src-tauri/src/llm/openai_responses.rs`, preserving all public items (`OpenAiReplyClient`, `OpenAiReplyConfig`, `with_transport`, `from_api_key`, `from_env`, `build_responses_body`, `ResponsesTransport`, `ResponsesEventStream`):

1. Replace the local `now_ms`, `truncate_for_error`, and `GENERIC_FAILURE_TEXT` definitions with imports:
   ```rust
   use super::streaming::{spawn_streaming_reply, ReplyChunk, SseValueStream};
   use super::streaming::{now_ms, truncate_for_error, GENERIC_FAILURE_TEXT};
   ```
   (Keep `truncate_for_error` used in the http-error path; keep its inline test by importing it.)
2. Add an adapter so a `ResponsesEventStream` is usable as an `SseValueStream`:
   ```rust
   struct ResponsesValueStream(Box<dyn ResponsesEventStream>);
   impl SseValueStream for ResponsesValueStream {
       fn next_value(&mut self) -> Result<Option<serde_json::Value>, LlmError> {
           self.0.next_event()
       }
   }
   ```
3. Replace the whole `OpenAiReplyGeneration` struct + its `start` + `impl ReplyGeneration` + `send_final`/`send_failure_final` with a call into the shared engine inside `StreamingReplyClient::start`:
   ```rust
   impl StreamingReplyClient for OpenAiReplyClient {
       fn name(&self) -> &'static str {
           "openai-responses-llm"
       }
       fn start(&self, request: ReplyRequest) -> Box<dyn ReplyGeneration> {
           let config = self.config.clone();
           let transport = std::sync::Arc::clone(&self.transport);
           let open = {
               let request = request.clone();
               move || -> Result<Box<dyn SseValueStream>, LlmError> {
                   let stream = transport.stream(&config, &request)?;
                   Ok(Box::new(ResponsesValueStream(stream)))
               }
           };
           spawn_streaming_reply(request, open, responses_map)
       }
   }

   fn responses_map(value: &serde_json::Value) -> ReplyChunk {
       match value["type"].as_str() {
           Some("response.output_text.delta") => match value["delta"].as_str() {
               Some(delta) => ReplyChunk::Token(delta.to_string()),
               None => ReplyChunk::Ignore,
           },
           Some("response.completed") => ReplyChunk::Complete,
           Some("response.error") | Some("error") => ReplyChunk::Error,
           _ => ReplyChunk::Ignore,
       }
   }
   ```
   Delete the now-unused `OpenAiReplyGeneration`, `send_final`, `send_failure_final`, and the local `now_ms`/`truncate_for_error`/`GENERIC_FAILURE_TEXT` (now imported). Keep `ReplyRequest` clonable (it is). Ensure `use std::sync::Arc;` remains (used for transport).

- [ ] **Step 5: Run engine + responses + orchestration tests (GREEN)**

```powershell
cd src-tauri
cargo test --test streaming_engine --test openai_responses_llm --test llm_orchestration
cd ..
```

Expected: `streaming_engine` 2 passed; `openai_responses_llm` 3 passed (unchanged); `llm_orchestration` 8 passed.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/llm/streaming.rs src-tauri/src/llm/openai_responses.rs src-tauri/src/llm/mod.rs src-tauri/tests/streaming_engine.rs
git commit -m "refactor: extract shared llm streaming engine"
```

---

## Task 2: OpenAI-Compatible Chat Completions Adapter

**Files:**
- Create: `src-tauri/src/llm/openai_compatible.rs`
- Modify: `src-tauri/src/llm/mod.rs`
- Create: `src-tauri/tests/openai_compatible_llm.rs`

- [ ] **Step 1: Write failing adapter + dialect tests**

Create `src-tauri/tests/openai_compatible_llm.rs`:

```rust
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use respondent_lib::llm::client::{
    ReplyEvent, ReplyGeneration, ReplyPoll, ReplyRequest, StreamingReplyClient,
};
use respondent_lib::llm::openai_compatible::{
    build_chat_body, chat_map, join_chat_url, ChatTransport, OpenAiCompatibleReplyClient,
    ProviderConfig,
};
use respondent_lib::llm::streaming::{ReplyChunk, SseValueStream};
use respondent_lib::llm::client::LlmError;
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
    assert_eq!(join_chat_url("https://x/v1"), "https://x/v1/chat/completions");
    assert_eq!(join_chat_url("https://x/v1/"), "https://x/v1/chat/completions");
}

#[test]
fn build_chat_body_has_stream_model_messages() {
    let body = build_chat_body(&config(), &request());
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["stream"], true);
    let messages = body["messages"].as_array().expect("messages");
    assert_eq!(messages[0]["role"], "system");
    assert!(messages[1]["content"].as_str().unwrap().contains("What next?"));
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
        ]),
    ))
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
```

- [ ] **Step 2: Run to verify RED**

```powershell
cd src-tauri
cargo test --test openai_compatible_llm
cd ..
```

Expected: compile error `unresolved import respondent_lib::llm::openai_compatible`.

- [ ] **Step 3: Implement the adapter**

Create `src-tauri/src/llm/openai_compatible.rs`:

```rust
use std::sync::Arc;

use serde_json::{json, Value};

use super::client::{LlmError, ReplyGeneration, ReplyRequest, StreamingReplyClient};
use super::openai_responses::format_context;
use super::streaming::{spawn_streaming_reply, ReplyChunk, ReqwestSseStream, SseValueStream};

const SYSTEM_PROMPT: &str = "You are a live meeting assistant. Suggest one concise, useful reply the user could say next. Keep it natural, specific, and short.";

#[derive(Clone)]
pub struct ProviderConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub fn join_chat_url(base_url: &str) -> String {
    format!("{}/chat/completions", base_url.trim_end_matches('/'))
}

pub fn build_chat_body(config: &ProviderConfig, request: &ReplyRequest) -> Value {
    json!({
        "model": config.model,
        "stream": true,
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": format!(
                "Conversation context:\n{}\n\nCurrent turn:\n{}\n\nWrite the suggested reply only.",
                format_context(&request.context),
                request.transcript
            )}
        ]
    })
}

/// Map one Chat Completions SSE value to an engine action, tolerating provider
/// quirks (missing/null content, reasoning_content, empty-choices usage chunk).
pub fn chat_map(value: &Value) -> ReplyChunk {
    if value.get("error").is_some() {
        return ReplyChunk::Error;
    }
    match value["choices"][0]["delta"]["content"].as_str() {
        Some(content) if !content.is_empty() => ReplyChunk::Token(content.to_string()),
        _ => ReplyChunk::Ignore,
    }
}

pub trait ChatTransport: Send + Sync {
    fn stream(
        &self,
        config: &ProviderConfig,
        request: &ReplyRequest,
    ) -> Result<Box<dyn SseValueStream>, LlmError>;
}

pub struct ReqwestChatTransport {
    client: reqwest::blocking::Client,
}

impl Default for ReqwestChatTransport {
    fn default() -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
        }
    }
}

impl ChatTransport for ReqwestChatTransport {
    fn stream(
        &self,
        config: &ProviderConfig,
        request: &ReplyRequest,
    ) -> Result<Box<dyn SseValueStream>, LlmError> {
        let response = self
            .client
            .post(join_chat_url(&config.base_url))
            .bearer_auth(&config.api_key)
            .json(&build_chat_body(config, request))
            .send()
            .map_err(|err| LlmError::Provider(format!("chat completions request: {err}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(LlmError::Provider(format!(
                "chat completions http {status}: {}",
                super::streaming::truncate_for_error(&body)
            )));
        }
        Ok(Box::new(ReqwestSseStream::new(response)))
    }
}

pub struct OpenAiCompatibleReplyClient {
    config: ProviderConfig,
    transport: Arc<dyn ChatTransport>,
}

impl OpenAiCompatibleReplyClient {
    pub fn connect(config: ProviderConfig) -> Result<Self, LlmError> {
        Self::with_transport(config, Arc::new(ReqwestChatTransport::default()))
    }

    pub fn with_transport(
        config: ProviderConfig,
        transport: Arc<dyn ChatTransport>,
    ) -> Result<Self, LlmError> {
        if config.api_key.trim().is_empty() {
            return Err(LlmError::Provider("missing API key".to_string()));
        }
        if config.base_url.trim().is_empty() {
            return Err(LlmError::Provider("missing base_url".to_string()));
        }
        Ok(Self { config, transport })
    }
}

impl StreamingReplyClient for OpenAiCompatibleReplyClient {
    fn name(&self) -> &'static str {
        "openai-compatible-llm"
    }

    fn start(&self, request: ReplyRequest) -> Box<dyn ReplyGeneration> {
        let config = self.config.clone();
        let transport = Arc::clone(&self.transport);
        let open = {
            let request = request.clone();
            move || -> Result<Box<dyn SseValueStream>, LlmError> {
                transport.stream(&config, &request)
            }
        };
        spawn_streaming_reply(request, open, chat_map)
    }
}
```

This requires `format_context` to be public in `openai_responses.rs`. In Step 4 of Task 1 it is already `fn format_context`; change it to `pub fn format_context` (it is referenced here).

Add to `src-tauri/src/llm/mod.rs`:

```rust
pub mod openai_compatible;
```

- [ ] **Step 4: Make `format_context` public**

In `src-tauri/src/llm/openai_responses.rs`, change `fn format_context(` to `pub fn format_context(`.

- [ ] **Step 5: Run to verify GREEN**

```powershell
cd src-tauri
cargo test --test openai_compatible_llm --test openai_responses_llm
cd ..
```

Expected: `openai_compatible_llm` 6 passed; `openai_responses_llm` 3 passed.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/llm/openai_compatible.rs src-tauri/src/llm/openai_responses.rs src-tauri/src/llm/mod.rs src-tauri/tests/openai_compatible_llm.rs
git commit -m "feat: add openai-compatible chat completions llm adapter"
```

---

## Task 3: Provider Selection Resolver In commands.rs

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/tests/commands.rs`

- [ ] **Step 1: Write failing resolver tests**

In `src-tauri/tests/commands.rs`, replace the two existing reply-provider tests (`reply_provider_selection_uses_mock_without_api_key`, `reply_provider_selection_uses_openai_with_api_key`) with calls to a new pure resolver `resolve_reply_provider_name`, and add provider cases. Use this block (keep the file's other tests):

```rust
use respondent_lib::commands::resolve_reply_provider_name;

fn env(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

#[test]
fn provider_defaults_to_mock_without_keys() {
    assert_eq!(resolve_reply_provider_name(&env(&[])), "mock-llm");
}

#[test]
fn provider_openai_with_key() {
    assert_eq!(
        resolve_reply_provider_name(&env(&[("OPENAI_API_KEY", "k")])),
        "openai-responses-llm"
    );
}

#[test]
fn provider_dashscope_with_key() {
    assert_eq!(
        resolve_reply_provider_name(&env(&[("LLM_PROVIDER", "dashscope"), ("DASHSCOPE_API_KEY", "k")])),
        "openai-compatible-llm"
    );
}

#[test]
fn provider_compatible_missing_config_falls_back_to_mock() {
    assert_eq!(
        resolve_reply_provider_name(&env(&[("LLM_PROVIDER", "siliconflow")])),
        "mock-llm"
    );
}
```

- [ ] **Step 2: Run to verify RED**

```powershell
cd src-tauri
cargo test --test commands
cd ..
```

Expected: compile error `unresolved import respondent_lib::commands::resolve_reply_provider_name`.

- [ ] **Step 3: Implement the resolver**

In `src-tauri/src/commands.rs`:

1. Add imports near the other llm imports:
   ```rust
   use crate::llm::openai_compatible::{OpenAiCompatibleReplyClient, ProviderConfig};
   use crate::llm::openai_responses::{OpenAiReplyClient, OpenAiReplyConfig};
   use std::collections::HashMap;
   ```
   (Some may already exist; do not duplicate.)

2. Add a pure provider-config builder + resolver operating on an env map:

   ```rust
   /// Build the reply client from an env-like map. Returns (client, using_mock).
   pub fn resolve_reply_client(
       env: &HashMap<String, String>,
   ) -> Result<(Box<dyn StreamingReplyClient>, bool), String> {
       let provider = env
           .get("LLM_PROVIDER")
           .map(|p| p.trim().to_lowercase())
           .filter(|p| !p.is_empty())
           .unwrap_or_else(|| "openai".to_string());

       let get = |key: &str| env.get(key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());

       let compatible = |base_default: &str, key: Option<String>, model_default: &str, base_key: &str, model_key: &str| -> Option<ProviderConfig> {
           let api_key = key?;
           let base_url = get(base_key).unwrap_or_else(|| base_default.to_string());
           let model = get(model_key).unwrap_or_else(|| model_default.to_string());
           Some(ProviderConfig { base_url, api_key, model })
       };

       let cfg: Option<ProviderConfig> = match provider.as_str() {
           "openai" => {
               return match get("OPENAI_API_KEY") {
                   Some(key) => {
                       let client = OpenAiReplyClient::connect(OpenAiReplyConfig::from_api_key(key))
                           .map_err(|e| e.to_string())?;
                       Ok((Box::new(client), false))
                   }
                   None => Ok((Box::new(MockReplyClient), true)),
               };
           }
           "dashscope" => compatible(
               "https://dashscope.aliyuncs.com/compatible-mode/v1",
               get("DASHSCOPE_API_KEY"),
               "qwen-plus",
               "DASHSCOPE_BASE_URL",
               "DASHSCOPE_LLM_MODEL",
           ),
           "zhipu" => compatible(
               "https://open.bigmodel.cn/api/paas/v4",
               get("ZHIPU_API_KEY").or_else(|| get("ZAI_API_KEY")),
               "glm-4-plus",
               "ZHIPU_BASE_URL",
               "ZHIPU_LLM_MODEL",
           ),
           "siliconflow" => compatible(
               "https://api.siliconflow.cn/v1",
               get("SILICONFLOW_API_KEY"),
               "Qwen/Qwen3-8B",
               "SILICONFLOW_BASE_URL",
               "SILICONFLOW_LLM_MODEL",
           ),
           "openai_compatible" => {
               match (get("OPENAI_COMPATIBLE_API_KEY"), get("OPENAI_COMPATIBLE_BASE_URL"), get("OPENAI_COMPATIBLE_MODEL")) {
                   (Some(api_key), Some(base_url), Some(model)) => Some(ProviderConfig { base_url, api_key, model }),
                   _ => None,
               }
           }
           _ => None,
       };

       match cfg {
           Some(config) => {
               let client = OpenAiCompatibleReplyClient::connect(config).map_err(|e| e.to_string())?;
               Ok((Box::new(client), false))
           }
           None => Ok((Box::new(MockReplyClient), true)),
       }
   }

   pub fn resolve_reply_provider_name(env: &HashMap<String, String>) -> &'static str {
       let (client, _) = resolve_reply_client(env).expect("resolve reply client");
       client.name()
   }

   fn current_env() -> HashMap<String, String> {
       std::env::vars().collect()
   }
   ```

3. Replace the body of the existing `build_reply_client()` to use the resolver:
   ```rust
   fn build_reply_client() -> Result<(Box<dyn StreamingReplyClient>, bool), String> {
       resolve_reply_client(&current_env())
   }
   ```
   Remove the old `build_reply_client_from_api_key` and `reply_provider_name_for_test` if present, OR keep `reply_provider_name_for_test` delegating to `resolve_reply_provider_name(&current_env())` — but since the old commands.rs tests are replaced in Step 1, delete `build_reply_client_from_api_key` and the old `reply_provider_name_for_test`. Ensure `MockReplyClient`, `StreamingReplyClient` are imported (they are).

4. The `using_mock_llm` status message text may stay; if it references "OPENAI_API_KEY not set", generalize it to "No LLM provider configured; using mock LLM provider".

- [ ] **Step 4: Run to verify GREEN**

```powershell
cd src-tauri
cargo test --test commands
cd ..
```

Expected: `commands` tests pass (the 4 new provider tests + existing ones).

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/commands.rs src-tauri/tests/commands.rs
git commit -m "feat: select llm provider from env (openai-compatible providers)"
```

---

## Task 4: Full Verification And Gated E2E

**Files:**
- Modify: `src-tauri/tests/e2e_real_network.rs`

- [ ] **Step 1: Add a gated compatible-provider smoke test**

Append to `src-tauri/tests/e2e_real_network.rs` an `#[ignore]` test that, when `SILICONFLOW_API_KEY` (or a chosen provider key) is set, runs a real `OpenAiCompatibleReplyClient::connect(...)` generation and asserts a non-empty Final arrives. Read the existing file first to match its style and helpers; gate with `#[ignore = "requires a real provider key"]` and read the key via `std::env::var`. If the file already has a generic helper, reuse it.

- [ ] **Step 2: Full Rust suite**

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
cd src-tauri
cargo test
cargo check
cd ..
```

Expected: all non-ignored tests pass; `cargo check` clean.

- [ ] **Step 3: Frontend unaffected**

```powershell
npm test
```

Expected: frontend tests pass (no frontend changes).

- [ ] **Step 4: Privacy grep**

```powershell
rg -n "eCapture|microphone|mic|input device|recording device" src-tauri/src
```

Expected: no microphone/input capture path.

- [ ] **Step 5: Commit any gated-test changes**

```powershell
git add src-tauri/tests/e2e_real_network.rs
git commit -m "test: gated e2e smoke for openai-compatible provider"
```

---

## Self-Review

Spec coverage:
- Shared streaming engine (SSE reader + worker + cancellation-aborts-upstream): Task 1 (`streaming.rs`, `send_event` returns Err on consumer drop → worker returns).
- Responses refactored onto the engine, public API + tests preserved: Task 1.
- Chat Completions adapter + `ProviderConfig` + mockable `ChatTransport` + SSE tolerance (`chat_map`): Task 2.
- base_url normalization (`join_chat_url`): Task 2.
- Provider selection from env, config struct, env resolution only in commands: Task 3 (`resolve_reply_client`).
- Default models conservative/overridable (zhipu uses `glm-4-plus`, not the unverified `glm-5.1`): Task 3.
- Tests deterministic (mock transports, no network); gated e2e: Tasks 1-4.
- Key never leaked (failure-final generic text): Tasks 1-2 (`GENERIC_FAILURE_TEXT`).

Type consistency:
- `SseValueStream::next_value() -> Result<Option<Value>, LlmError>`, `ReplyChunk`, `spawn_streaming_reply(request, open, map)` used identically across `streaming.rs`, `openai_responses.rs`, `openai_compatible.rs`, and `tests/streaming_engine.rs`.
- `ProviderConfig { base_url, api_key, model }`, `OpenAiCompatibleReplyClient::{connect,with_transport}`, `ChatTransport::stream`, `build_chat_body`, `chat_map`, `join_chat_url` consistent across Task 2 and its tests.
- `resolve_reply_client(&HashMap) -> (Box<dyn StreamingReplyClient>, bool)`, `resolve_reply_provider_name(&HashMap) -> &'static str` consistent across Task 3 and its tests.

Out of scope: desktop settings UI for provider/key entry; multi-provider ASR; secure key storage.
