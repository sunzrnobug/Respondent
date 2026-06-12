# Streaming LLM Orchestration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consume the ASR `AsrEvent` stream and emit `ReplyEvent`s (started/token/final) through a provider-agnostic streaming LLM interface, a deterministic mock, an endpoint-triggered `ReplyTrigger`, and a `ReplySession` that does latest-wins cancel-restart — all deterministically tested with no network.

**Architecture:** Mirror the ASR sub-project. Keep trigger policy (`ReplyTrigger`, pure) separate from generation (`StreamingReplyClient`). A worker thread drains all available `AsrEvent`s (latest trigger replaces any in-flight generation = cancel) then pumps the active generation one pull-step at a time, forwarding `ReplyEvent`s. Synchronous threads + crossbeam channels, no async runtime; the future real Claude adapter implements the same sync trait and bridges its async stream internally (returning `ReplyPoll::Pending` while awaiting tokens).

**Tech Stack:** Rust, crossbeam-channel, thiserror, serde. No new dependencies.

---

## File Structure

- `src-tauri/src/llm/client.rs` (modify): keep `ReplyRequest`/`ReplyEvent`; add `LlmError`, `ReplyPoll`, `ReplyGeneration`; refine `StreamingReplyClient` into a streaming trait.
- `src-tauri/src/llm/mock.rs` (modify): `MockReplyClient` (unit struct) + `MockReplyGeneration` (deterministic pull-based generation).
- `src-tauri/src/llm/reply_trigger.rs` (create): `ReplyTrigger` (pure endpoint-triggered policy with rolling context).
- `src-tauri/src/llm/session.rs` (create): `ReplySession` (worker-thread orchestration, latest-wins).
- `src-tauri/src/llm/mod.rs` (modify): export the new modules.
- `src-tauri/tests/llm_orchestration.rs` (create): deterministic tests for trigger, mock, and session.

`reply_trigger.rs` depends on `crate::asr::client::AsrEvent` (same crate; `asr` is already a module). `provider_contract.rs` needs NO change — `MockReplyClient` stays a unit struct and `name()` is unchanged.

Run all Rust commands with cargo on PATH (cargo is NOT on the default PATH):

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
```

Never run `cargo update` (the lockfile pins `time` to 0.3.47 on purpose).

---

## Task 1: Reply Trigger

**Files:**
- Create: `src-tauri/src/llm/reply_trigger.rs`
- Modify: `src-tauri/src/llm/mod.rs`
- Create: `src-tauri/tests/llm_orchestration.rs`

- [ ] **Step 1: Write failing trigger tests**

Create `src-tauri/tests/llm_orchestration.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify RED**

```powershell
cd src-tauri
cargo test --test llm_orchestration
cd ..
```

Expected: compile error `unresolved import respondent_lib::llm::reply_trigger`.

- [ ] **Step 3: Implement the trigger**

Create `src-tauri/src/llm/reply_trigger.rs`:

```rust
use crate::asr::client::AsrEvent;

use super::client::ReplyRequest;

const MAX_CONTEXT_TURNS: usize = 6;

/// Endpoint-triggered reply policy (ports the frontend replyEngine): a reply
/// is requested only on a `Final` that follows an `Endpoint`, carrying a
/// rolling window of recent final turns as context.
pub struct ReplyTrigger {
    session_id: String,
    endpoint_armed: bool,
    context: Vec<String>,
    generation_counter: u64,
}

impl ReplyTrigger {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            endpoint_armed: false,
            context: Vec::new(),
            generation_counter: 0,
        }
    }

    pub fn observe(&mut self, event: &AsrEvent) -> Option<ReplyRequest> {
        match event {
            AsrEvent::Endpoint { .. } => {
                self.endpoint_armed = true;
                None
            }
            AsrEvent::Final { text, .. } => {
                self.context.push(text.clone());
                while self.context.len() > MAX_CONTEXT_TURNS {
                    self.context.remove(0);
                }
                if self.endpoint_armed {
                    self.endpoint_armed = false;
                    self.generation_counter += 1;
                    Some(ReplyRequest {
                        session_id: self.session_id.clone(),
                        generation_id: format!("gen-{}", self.generation_counter),
                        transcript: text.clone(),
                        context: self.context.clone(),
                    })
                } else {
                    None
                }
            }
            AsrEvent::Partial { .. } => None,
        }
    }
}
```

Modify `src-tauri/src/llm/mod.rs` so it reads:

```rust
pub mod client;
pub mod mock;
pub mod reply_trigger;
```

- [ ] **Step 4: Run tests to verify GREEN**

```powershell
cd src-tauri
cargo test --test llm_orchestration
cd ..
```

Expected: `4 passed`.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/llm/reply_trigger.rs src-tauri/src/llm/mod.rs src-tauri/tests/llm_orchestration.rs
git commit -m "feat: add endpoint-triggered reply trigger"
```

---

## Task 2: Streaming Reply Trait, Error, And Deterministic Mock

**Files:**
- Modify: `src-tauri/src/llm/client.rs`
- Modify: `src-tauri/src/llm/mock.rs`
- Modify (append): `src-tauri/tests/llm_orchestration.rs`

- [ ] **Step 1: Write failing mock generation test**

APPEND to `src-tauri/tests/llm_orchestration.rs` (the helpers from Task 1 stay; only add the new `use` lines shown):

```rust
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
    assert!(events.iter().any(|event| matches!(event, ReplyEvent::Token { .. })));
    match events.last() {
        Some(ReplyEvent::Final { generation_id, text, .. }) => {
            assert_eq!(generation_id.as_str(), "gen-1");
            assert_eq!(text.as_str(), "Acknowledged: could you summarize");
        }
        other => panic!("expected final, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run tests to verify RED**

```powershell
cd src-tauri
cargo test --test llm_orchestration
cd ..
```

Expected: compile errors — `ReplyPoll` not found and `start`/`poll` not members.

- [ ] **Step 3: Refine the trait and add the error and poll types**

Replace the ENTIRE contents of `src-tauri/src/llm/client.rs` with exactly:

```rust
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ReplyRequest {
    pub session_id: String,
    pub generation_id: String,
    pub transcript: String,
    pub context: Vec<String>,
}

/// Streaming reply events. The wire shape mirrors the frontend RealtimeEvent
/// contract: an internally tagged "type" plus camelCase fields.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum ReplyEvent {
    #[serde(rename = "reply.started")]
    Started {
        session_id: String,
        generation_id: String,
        based_on_transcript_event_id: String,
        received_at_ms: i64,
    },
    #[serde(rename = "reply.token")]
    Token {
        session_id: String,
        generation_id: String,
        token: String,
        received_at_ms: i64,
    },
    #[serde(rename = "reply.final")]
    Final {
        session_id: String,
        generation_id: String,
        text: String,
        received_at_ms: i64,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("reply stream closed")]
    Closed,
    #[error("llm provider error: {0}")]
    Provider(String),
}

/// One pull from a `ReplyGeneration`.
pub enum ReplyPoll {
    Event(ReplyEvent),
    /// No event yet, but generation is still in progress (real adapters that
    /// await network tokens return this; the mock never does).
    Pending,
    Done,
}

/// A single in-progress reply generation. Pull events with `poll`; dropping
/// the value cancels the generation.
pub trait ReplyGeneration: Send {
    fn poll(&mut self) -> ReplyPoll;
}

pub trait StreamingReplyClient: Send {
    fn name(&self) -> &'static str;
    /// Begin generating a reply for `request`; returns the pull handle.
    fn start(&self, request: ReplyRequest) -> Box<dyn ReplyGeneration>;
}
```

- [ ] **Step 4: Implement the deterministic mock**

Replace the ENTIRE contents of `src-tauri/src/llm/mock.rs` with exactly:

```rust
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
            .split(' ')
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
```

- [ ] **Step 5: Run tests to verify GREEN**

```powershell
cd src-tauri
cargo test --test llm_orchestration --test provider_contract
cd ..
```

Expected: `llm_orchestration` `5 passed`; `provider_contract` `3 passed` (unchanged — `MockReplyClient` is still a unit struct with `name()`).

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/llm/client.rs src-tauri/src/llm/mock.rs src-tauri/tests/llm_orchestration.rs
git commit -m "feat: add streaming reply trait and deterministic mock"
```

---

## Task 3: Reply Session Orchestration

**Files:**
- Create: `src-tauri/src/llm/session.rs`
- Modify: `src-tauri/src/llm/mod.rs`
- Modify (append): `src-tauri/tests/llm_orchestration.rs`

- [ ] **Step 1: Write failing orchestration tests**

APPEND to `src-tauri/tests/llm_orchestration.rs` (reuse the `endpoint()`/`final_event()`/`partial()` helpers and existing imports; only add the new `use` lines shown):

```rust
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
```

- [ ] **Step 2: Run tests to verify RED**

```powershell
cd src-tauri
cargo test --test llm_orchestration
cd ..
```

Expected: compile error `unresolved import respondent_lib::llm::session`.

- [ ] **Step 3: Implement the session**

Create `src-tauri/src/llm/session.rs` with exactly:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TryRecvError};

use crate::asr::client::AsrEvent;

use super::client::{LlmError, ReplyEvent, ReplyGeneration, ReplyPoll, StreamingReplyClient};
use super::reply_trigger::ReplyTrigger;

const OUTPUT_CAPACITY: usize = 256;
/// Max time the worker blocks waiting for input while idle (also bounds how
/// soon it observes the stop flag).
const IDLE_WAIT: Duration = Duration::from_millis(100);
/// Max time the worker blocks sending one event before giving up (avoids a
/// deadlock if a stopped consumer never drains the output channel).
const SEND_TIMEOUT: Duration = Duration::from_millis(200);
/// Backoff when an in-flight generation has no token ready yet.
const PENDING_WAIT: Duration = Duration::from_millis(5);

pub struct ReplySession {
    events: Receiver<ReplyEvent>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<(), LlmError>>>,
}

impl ReplySession {
    pub fn start(
        asr_events: Receiver<AsrEvent>,
        client: Box<dyn StreamingReplyClient>,
        trigger: ReplyTrigger,
    ) -> ReplySession {
        let (out_tx, out_rx) = bounded::<ReplyEvent>(OUTPUT_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);

        let handle = thread::Builder::new()
            .name("llm-reply-session".into())
            .spawn(move || {
                run_session(&asr_events, client.as_ref(), trigger, &out_tx, &thread_stop)
            })
            .expect("spawn llm reply session thread");

        ReplySession {
            events: out_rx,
            stop,
            handle: Some(handle),
        }
    }

    pub fn events(&self) -> Receiver<ReplyEvent> {
        self.events.clone()
    }

    pub fn stop(mut self) -> Result<(), LlmError> {
        self.stop.store(true, Ordering::Release);
        self.join()
    }

    fn join(&mut self) -> Result<(), LlmError> {
        match self.handle.take() {
            Some(handle) => handle.join().unwrap_or(Err(LlmError::Closed)),
            None => Ok(()),
        }
    }
}

impl Drop for ReplySession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.join();
    }
}

fn run_session(
    asr_events: &Receiver<AsrEvent>,
    client: &dyn StreamingReplyClient,
    mut trigger: ReplyTrigger,
    out: &Sender<ReplyEvent>,
    stop: &AtomicBool,
) -> Result<(), LlmError> {
    let mut active: Option<Box<dyn ReplyGeneration>> = None;

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        // 1. Drain every currently-available ASR event; the latest trigger
        //    replaces (and thereby cancels) any in-flight generation.
        let mut disconnected = false;
        loop {
            match asr_events.try_recv() {
                Ok(event) => {
                    if let Some(request) = trigger.observe(&event) {
                        active = Some(client.start(request));
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        // 2. Pump the active generation one step.
        if let Some(generation) = active.as_mut() {
            match generation.poll() {
                ReplyPoll::Event(event) => {
                    out.send_timeout(event, SEND_TIMEOUT)
                        .map_err(|_| LlmError::Closed)?;
                }
                ReplyPoll::Pending => thread::sleep(PENDING_WAIT),
                ReplyPoll::Done => active = None,
            }
            continue;
        }

        // 3. Idle (nothing generating): exit if input is gone, else block for
        //    the next event.
        if disconnected {
            break;
        }
        match asr_events.recv_timeout(IDLE_WAIT) {
            Ok(event) => {
                if let Some(request) = trigger.observe(&event) {
                    active = Some(client.start(request));
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    Ok(())
}
```

Modify `src-tauri/src/llm/mod.rs` so it reads:

```rust
pub mod client;
pub mod mock;
pub mod reply_trigger;
pub mod session;
```

- [ ] **Step 4: Run tests to verify GREEN**

```powershell
cd src-tauri
cargo test --test llm_orchestration
cd ..
```

Expected: `7 passed` (4 trigger + 1 mock + 2 session).

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/llm/session.rs src-tauri/src/llm/mod.rs src-tauri/tests/llm_orchestration.rs
git commit -m "feat: add reply session orchestration with latest-wins"
```

---

## Task 4: Full Verification

**Files:**
- No new files expected.

- [ ] **Step 1: Run the full Rust suite**

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
cd src-tauri
cargo test
cargo check
cd ..
```

Expected: all non-ignored tests pass (including `llm_orchestration` 7, `asr_orchestration` 11, `provider_contract` 3); `cargo check` finishes clean.

- [ ] **Step 2: Confirm the frontend is unaffected**

```powershell
npm test
```

Expected: 55 tests pass (no frontend files changed).

- [ ] **Step 3: Confirm privacy grep**

```powershell
rg -n "eCapture|microphone|mic|input device|recording device" src-tauri/src
```

Expected: no implementation path that opens microphone/input capture.

- [ ] **Step 4: No-op commit guard**

If no files changed in this task, do not create an empty commit. This task is verification only.

---

## Self-Review

Spec coverage:

- `StreamingReplyClient` streaming interface + `ReplyGeneration`/`ReplyPoll` + `LlmError`: Task 2.
- `MockReplyClient` (unit struct) + deterministic `MockReplyGeneration`: Task 2.
- `ReplyTrigger` endpoint-triggered policy with 6-turn rolling context + generation counter: Task 1.
- `ReplySession` orchestration consuming `AsrEvent` -> `ReplyEvent` with latest-wins cancel-restart: Task 3.
- `ReplyEvent`/`ReplyRequest` unchanged, camelCase wire contract preserved; `provider_contract.rs` serialization test still runs: Task 2 keeps the enum and the unit-struct mock.
- Cancellation by dropping the generation (no explicit cancel event): Task 3 (`active = Some(...)` replaces the old box).
- Synchronous threads + crossbeam channels, no async runtime, no new deps: Tasks 1-3.
- Deterministic tests with synthetic `AsrEvent`s, no network: Tasks 1-3 (latest-wins is deterministic because both triggers are drained before the first pump).

Type consistency:

- `ReplyTrigger::new(session_id)`, `observe(&AsrEvent) -> Option<ReplyRequest>` used identically in Tasks 1 and 3.
- `MockReplyClient` (unit struct), `StreamingReplyClient::start(ReplyRequest) -> Box<dyn ReplyGeneration>`, `ReplyGeneration::poll() -> ReplyPoll` used identically in Tasks 2 and 3.
- `ReplySession::start(Receiver<AsrEvent>, Box<dyn StreamingReplyClient>, ReplyTrigger)` matches the trait and trigger signatures.

Out of scope (future sub-projects): real Claude/Anthropic streaming adapter implementing `StreamingReplyClient`; end-to-end capture→ASR→LLM wiring with a Tauri `emit` bridge to the frontend.
