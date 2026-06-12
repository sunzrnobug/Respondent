# Streaming ASR Orchestration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the WASAPI capture frame stream to provider-agnostic `AsrEvent`s (partial/final/endpoint) through a streaming `StreamingAsrClient` interface, a deterministic mock, a local energy endpointer, and a `TranscriptionSession` orchestration — all deterministically tested with no network.

**Architecture:** Keep text recognition (ASR client) and turn detection (energy endpointer) as separate, independently tested units. A worker thread reads `Receiver<AudioFrame>`, runs the endpointer and the ASR client per frame, and emits a unified `AsrEvent` stream with a guaranteed `partial… → endpoint → final` ordering. Synchronous threads + crossbeam channels (no async runtime); the future real cloud adapter implements the same sync trait and bridges its async WebSocket internally.

**Tech Stack:** Rust, crossbeam-channel, thiserror, serde. No new dependencies.

---

## File Structure

- `src-tauri/src/asr/client.rs` (modify): keep `AsrEvent`; add `AsrError`; refine `StreamingAsrClient` into a streaming trait.
- `src-tauri/src/asr/mock.rs` (modify): flesh out `MockAsrClient` into a deterministic streaming implementation.
- `src-tauri/src/asr/endpointer.rs` (create): `EnergyEndpointer` (pure energy/silence state machine).
- `src-tauri/src/asr/session.rs` (create): `TranscriptionSession` (worker-thread orchestration).
- `src-tauri/src/asr/mod.rs` (modify): export the new modules.
- `src-tauri/tests/asr_orchestration.rs` (create): deterministic tests for endpointer, mock, and session.
- `src-tauri/tests/provider_contract.rs` (modify): update the `MockAsrClient` name assertion to the new constructor.

Run all Rust commands with cargo on PATH:

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
```

---

## Task 1: Energy Endpointer

**Files:**
- Create: `src-tauri/src/asr/endpointer.rs`
- Modify: `src-tauri/src/asr/mod.rs`
- Create: `src-tauri/tests/asr_orchestration.rs`

- [ ] **Step 1: Write failing endpointer tests**

Create `src-tauri/tests/asr_orchestration.rs`:

```rust
use respondent_lib::asr::endpointer::{EndpointSignal, EnergyEndpointer};
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};

fn frame(amplitude: i16, captured_at_ms: u64) -> AudioFrame {
    AudioFrame {
        format: PcmFormat {
            sample_rate: 16_000,
            channels: 1,
            bits_per_sample: 16,
        },
        samples: vec![amplitude; 320],
        captured_at_ms,
    }
}

#[test]
fn endpointer_ignores_pure_silence() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    assert_eq!(endpointer.observe(&frame(0, 0)), None);
    assert_eq!(endpointer.observe(&frame(0, 20)), None);
}

#[test]
fn endpointer_emits_start_of_speech_once() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    assert_eq!(endpointer.observe(&frame(8000, 0)), Some(EndpointSignal::StartOfSpeech));
    assert_eq!(endpointer.observe(&frame(8000, 20)), None);
}

#[test]
fn endpointer_emits_end_of_speech_after_silence_window() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    endpointer.observe(&frame(8000, 0)); // start of speech
    assert_eq!(endpointer.observe(&frame(0, 20)), None); // 20ms silence
    assert_eq!(endpointer.observe(&frame(0, 40)), None); // 40ms silence
    assert_eq!(endpointer.observe(&frame(0, 60)), Some(EndpointSignal::EndOfSpeech)); // 60ms
    assert_eq!(endpointer.observe(&frame(0, 80)), None); // already idle
}

#[test]
fn endpointer_rearms_for_a_new_utterance() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    endpointer.observe(&frame(8000, 0));
    endpointer.observe(&frame(0, 20));
    endpointer.observe(&frame(0, 40));
    endpointer.observe(&frame(0, 60)); // end of speech
    assert_eq!(endpointer.observe(&frame(8000, 80)), Some(EndpointSignal::StartOfSpeech));
}

#[test]
fn endpointer_exposes_its_silence_window() {
    let endpointer = EnergyEndpointer::new(0.01, 300);
    assert_eq!(endpointer.silence_window_ms(), 300);
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cd src-tauri
cargo test --test asr_orchestration
cd ..
```

Expected: compile error `unresolved import respondent_lib::asr::endpointer`.

- [ ] **Step 3: Implement the endpointer**

Create `src-tauri/src/asr/endpointer.rs`:

```rust
use crate::audio::frame::AudioFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointSignal {
    StartOfSpeech,
    EndOfSpeech,
}

/// Provider-independent turn detection from audio energy. A frame whose RMS
/// energy is at or above `speech_threshold` counts as speech; once speech has
/// started, `silence_window_ms` of continuous sub-threshold audio ends the turn.
pub struct EnergyEndpointer {
    speech_threshold: f32,
    silence_window_ms: u32,
    in_speech: bool,
    silence_accum_ms: u32,
}

impl EnergyEndpointer {
    pub fn new(speech_threshold: f32, silence_window_ms: u32) -> Self {
        Self {
            speech_threshold,
            silence_window_ms,
            in_speech: false,
            silence_accum_ms: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(0.01, 300)
    }

    pub fn silence_window_ms(&self) -> u32 {
        self.silence_window_ms
    }

    pub fn observe(&mut self, frame: &AudioFrame) -> Option<EndpointSignal> {
        let rms = frame_rms(&frame.samples);
        if rms >= self.speech_threshold {
            self.silence_accum_ms = 0;
            if !self.in_speech {
                self.in_speech = true;
                return Some(EndpointSignal::StartOfSpeech);
            }
            return None;
        }

        if self.in_speech {
            self.silence_accum_ms = self.silence_accum_ms.saturating_add(frame.duration_ms());
            if self.silence_accum_ms >= self.silence_window_ms {
                self.in_speech = false;
                self.silence_accum_ms = 0;
                return Some(EndpointSignal::EndOfSpeech);
            }
        }
        None
    }
}

fn frame_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&sample| {
            let normalized = sample as f64 / i16::MAX as f64;
            normalized * normalized
        })
        .sum();
    ((sum_sq / samples.len() as f64).sqrt()) as f32
}
```

Modify `src-tauri/src/asr/mod.rs` to:

```rust
pub mod client;
pub mod endpointer;
pub mod mock;
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```powershell
cd src-tauri
cargo test --test asr_orchestration
cd ..
```

Expected: `5 passed`.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/asr/endpointer.rs src-tauri/src/asr/mod.rs src-tauri/tests/asr_orchestration.rs
git commit -m "feat: add energy-based asr endpointer"
```

---

## Task 2: Streaming Trait, Error, And Deterministic Mock

**Files:**
- Modify: `src-tauri/src/asr/client.rs`
- Modify: `src-tauri/src/asr/mock.rs`
- Modify: `src-tauri/tests/provider_contract.rs`
- Modify: `src-tauri/tests/asr_orchestration.rs`

- [ ] **Step 1: Write failing mock streaming tests**

Append to `src-tauri/tests/asr_orchestration.rs`:

```rust
use respondent_lib::asr::client::{AsrEvent, StreamingAsrClient};
use respondent_lib::asr::mock::MockAsrClient;

#[test]
fn mock_emits_partials_while_frames_arrive() {
    let mut client = MockAsrClient::new("s1");
    let events = client.events();
    for index in 0..25 {
        client.push_frame(&frame(8000, index * 20)).unwrap();
    }

    match events.try_recv().expect("a partial after 25 frames") {
        AsrEvent::Partial { session_id, text, .. } => {
            assert_eq!(session_id, "s1");
            assert!(!text.is_empty());
        }
        other => panic!("expected partial, got {other:?}"),
    }
}

#[test]
fn mock_emits_full_phrase_on_finalize() {
    let mut client = MockAsrClient::new("s1");
    let events = client.events();
    for index in 0..10 {
        client.push_frame(&frame(8000, index * 20)).unwrap();
    }
    client.finalize().unwrap();

    let mut last_final = None;
    while let Ok(event) = events.try_recv() {
        if let AsrEvent::Final { text, .. } = event {
            last_final = Some(text);
        }
    }
    assert_eq!(last_final.as_deref(), Some("could you summarize the timeline"));
}

#[test]
fn mock_advances_to_next_phrase_after_finalize() {
    let mut client = MockAsrClient::new("s1");
    let events = client.events();

    client.push_frame(&frame(8000, 0)).unwrap();
    client.finalize().unwrap();
    client.push_frame(&frame(8000, 20)).unwrap();
    client.finalize().unwrap();

    let finals: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            AsrEvent::Final { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(finals, vec![
        "could you summarize the timeline".to_string(),
        "what are the main risks".to_string(),
    ]);
}

#[test]
fn mock_finalize_without_frames_is_a_noop() {
    let mut client = MockAsrClient::new("s1");
    let events = client.events();
    client.finalize().unwrap();
    assert!(events.try_recv().is_err());
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cd src-tauri
cargo test --test asr_orchestration
cd ..
```

Expected: compile errors — `MockAsrClient::new` not found and `push_frame`/`finalize` not members of `StreamingAsrClient`.

- [ ] **Step 3: Refine the trait and add the error type**

Replace the trait section of `src-tauri/src/asr/client.rs` (keep the existing `AsrEvent` enum unchanged) so the file reads:

```rust
use crossbeam_channel::Receiver;
use serde::Serialize;

use crate::audio::frame::AudioFrame;

/// Streaming ASR events. The wire shape mirrors the frontend RealtimeEvent
/// contract: an internally tagged "type" plus camelCase fields.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum AsrEvent {
    #[serde(rename = "transcript.partial")]
    Partial {
        session_id: String,
        text: String,
        started_at_ms: i64,
        ended_at_ms: i64,
        received_at_ms: i64,
    },
    #[serde(rename = "transcript.final")]
    Final {
        session_id: String,
        text: String,
        started_at_ms: i64,
        ended_at_ms: i64,
        received_at_ms: i64,
    },
    #[serde(rename = "endpoint.detected")]
    Endpoint {
        session_id: String,
        silence_ms: i64,
        detected_at_ms: i64,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("asr stream closed")]
    Closed,
    #[error("asr provider error: {0}")]
    Provider(String),
}

/// A streaming ASR session. One instance serves one transcription session.
/// `events()` carries only `Partial`/`Final`; `Endpoint` is produced by the
/// orchestration's local endpointer, not the ASR client.
pub trait StreamingAsrClient: Send {
    fn name(&self) -> &'static str;
    /// Feed one 16 kHz/mono/i16 audio frame; may produce partials via events().
    fn push_frame(&mut self, frame: &AudioFrame) -> Result<(), AsrError>;
    /// The event stream for this session (clonable).
    fn events(&self) -> Receiver<AsrEvent>;
    /// Close the current utterance, producing a `Final` and arming the next.
    fn finalize(&mut self) -> Result<(), AsrError>;
}
```

- [ ] **Step 4: Implement the deterministic mock**

Replace `src-tauri/src/asr/mock.rs` with:

```rust
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
```

- [ ] **Step 5: Update the provider-contract name test**

In `src-tauri/tests/provider_contract.rs`, replace the line:

```rust
    assert_eq!(MockAsrClient.name(), "mock-asr");
```

with:

```rust
    assert_eq!(MockAsrClient::new("s1").name(), "mock-asr");
```

- [ ] **Step 6: Run tests to verify GREEN**

Run:

```powershell
cd src-tauri
cargo test --test asr_orchestration --test provider_contract
cd ..
```

Expected: `asr_orchestration` `9 passed`; `provider_contract` `3 passed`.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/asr/client.rs src-tauri/src/asr/mock.rs src-tauri/tests/provider_contract.rs src-tauri/tests/asr_orchestration.rs
git commit -m "feat: add streaming asr trait and deterministic mock"
```

---

## Task 3: Transcription Session Orchestration

**Files:**
- Create: `src-tauri/src/asr/session.rs`
- Modify: `src-tauri/src/asr/mod.rs`
- Modify: `src-tauri/tests/asr_orchestration.rs`

- [ ] **Step 1: Write failing orchestration test**

Append to `src-tauri/tests/asr_orchestration.rs`:

```rust
use std::time::Duration;

use crossbeam_channel::unbounded as unbounded_frames;
use respondent_lib::asr::session::TranscriptionSession;

#[test]
fn session_emits_partial_then_endpoint_then_final_for_one_utterance() {
    let (frame_tx, frame_rx) = unbounded_frames();
    let session = TranscriptionSession::start(
        "s1".to_string(),
        frame_rx,
        Box::new(MockAsrClient::new("s1")),
        EnergyEndpointer::new(0.01, 60),
    );
    let events = session.events();

    let mut at_ms = 0u64;
    for _ in 0..30 {
        frame_tx.send(frame(8000, at_ms)).unwrap();
        at_ms += 20;
    }
    for _ in 0..5 {
        frame_tx.send(frame(0, at_ms)).unwrap();
        at_ms += 20;
    }
    drop(frame_tx); // closing the capture stream ends the session

    let mut collected = Vec::new();
    while let Ok(event) = events.recv_timeout(Duration::from_secs(2)) {
        collected.push(event);
    }
    session.stop().unwrap();

    assert!(
        matches!(collected.first(), Some(AsrEvent::Partial { .. })),
        "first event should be a partial, got {collected:?}"
    );
    let endpoint_pos = collected
        .iter()
        .position(|event| matches!(event, AsrEvent::Endpoint { .. }))
        .expect("an endpoint event");
    let final_pos = collected
        .iter()
        .position(|event| matches!(event, AsrEvent::Final { .. }))
        .expect("a final event");
    assert!(endpoint_pos < final_pos, "endpoint must precede final: {collected:?}");
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cd src-tauri
cargo test --test asr_orchestration
cd ..
```

Expected: compile error `unresolved import respondent_lib::asr::session`.

- [ ] **Step 3: Implement the session**

Create `src-tauri/src/asr/session.rs`:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender};

use crate::audio::frame::AudioFrame;

use super::client::{AsrError, AsrEvent, StreamingAsrClient};
use super::endpointer::{EndpointSignal, EnergyEndpointer};

const OUTPUT_CAPACITY: usize = 256;
const FRAME_WAIT: Duration = Duration::from_millis(100);

pub struct TranscriptionSession {
    events: Receiver<AsrEvent>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<Result<(), AsrError>>>,
}

impl TranscriptionSession {
    pub fn start(
        session_id: String,
        frames: Receiver<AudioFrame>,
        mut client: Box<dyn StreamingAsrClient>,
        mut endpointer: EnergyEndpointer,
    ) -> TranscriptionSession {
        let (out_tx, out_rx) = bounded::<AsrEvent>(OUTPUT_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);

        let handle = thread::Builder::new()
            .name("asr-transcription".into())
            .spawn(move || {
                run_session(
                    &session_id,
                    &frames,
                    client.as_mut(),
                    &mut endpointer,
                    &out_tx,
                    &thread_stop,
                )
            })
            .expect("spawn asr transcription thread");

        TranscriptionSession {
            events: out_rx,
            stop,
            handle: Some(handle),
        }
    }

    pub fn events(&self) -> Receiver<AsrEvent> {
        self.events.clone()
    }

    pub fn stop(mut self) -> Result<(), AsrError> {
        self.stop.store(true, Ordering::Release);
        self.join()
    }

    fn join(&mut self) -> Result<(), AsrError> {
        match self.handle.take() {
            Some(handle) => handle.join().unwrap_or(Err(AsrError::Closed)),
            None => Ok(()),
        }
    }
}

impl Drop for TranscriptionSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.join();
    }
}

fn run_session(
    session_id: &str,
    frames: &Receiver<AudioFrame>,
    client: &mut dyn StreamingAsrClient,
    endpointer: &mut EnergyEndpointer,
    out: &Sender<AsrEvent>,
    stop: &Arc<AtomicBool>,
) -> Result<(), AsrError> {
    let client_events = client.events();
    let mut saw_speech = false;

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        let frame = match frames.recv_timeout(FRAME_WAIT) {
            Ok(frame) => frame,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };

        let signal = endpointer.observe(&frame);
        if matches!(signal, Some(EndpointSignal::StartOfSpeech)) {
            saw_speech = true;
        }

        client.push_frame(&frame)?;
        forward_available(&client_events, out)?;

        if matches!(signal, Some(EndpointSignal::EndOfSpeech)) {
            out.send(AsrEvent::Endpoint {
                session_id: session_id.to_string(),
                silence_ms: endpointer.silence_window_ms() as i64,
                detected_at_ms: frame.captured_at_ms as i64,
            })
            .map_err(|_| AsrError::Closed)?;
            client.finalize()?;
            forward_available(&client_events, out)?;
            saw_speech = false;
        }
    }

    if saw_speech {
        client.finalize()?;
        forward_available(&client_events, out)?;
    }
    Ok(())
}

fn forward_available(
    client_events: &Receiver<AsrEvent>,
    out: &Sender<AsrEvent>,
) -> Result<(), AsrError> {
    while let Ok(event) = client_events.try_recv() {
        out.send(event).map_err(|_| AsrError::Closed)?;
    }
    Ok(())
}
```

Modify `src-tauri/src/asr/mod.rs` to:

```rust
pub mod client;
pub mod endpointer;
pub mod mock;
pub mod session;
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```powershell
cd src-tauri
cargo test --test asr_orchestration
cd ..
```

Expected: `10 passed`.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/asr/session.rs src-tauri/src/asr/mod.rs src-tauri/tests/asr_orchestration.rs
git commit -m "feat: add transcription session orchestration"
```

---

## Task 4: Full Verification

**Files:**
- No new files expected.

- [ ] **Step 1: Run the full Rust suite**

Run:

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
cd src-tauri
cargo test
cargo check
cd ..
```

Expected: all non-ignored tests pass (including `asr_orchestration` 10, `provider_contract` 3); `cargo check` finishes clean.

- [ ] **Step 2: Confirm the frontend is unaffected**

Run:

```powershell
npm test
```

Expected: 55 tests pass (no frontend files changed).

- [ ] **Step 3: Confirm privacy grep**

Run:

```powershell
rg -n "eCapture|microphone|mic|input device|recording device" src-tauri/src
```

Expected: no implementation path that opens microphone/input capture.

- [ ] **Step 4: No-op commit guard**

If no files changed in this task, do not create an empty commit. This task is verification only.

---

## Self-Review

Spec coverage:

- `StreamingAsrClient` streaming interface + `AsrError`: Task 2.
- `MockAsrClient` deterministic implementation: Task 2.
- `EnergyEndpointer` local energy/silence detection: Task 1.
- `TranscriptionSession` orchestration with `partial → endpoint → final` ordering: Task 3.
- `AsrEvent` unchanged, camelCase wire contract preserved: Task 2 keeps the enum; `provider_contract.rs` serialization tests still run.
- Endpoint produced by orchestration from the endpointer (not the ASR client): Task 3 emits `Endpoint`; the mock never emits it.
- Synchronous threads + crossbeam channels, no async runtime, no new deps: Tasks 1-3.
- Deterministic tests with synthetic frames, no network: Tasks 1-3.

Type consistency:

- `EnergyEndpointer::new(threshold, window)`, `observe(&frame) -> Option<EndpointSignal>`, `silence_window_ms()` used identically in Tasks 1 and 3.
- `MockAsrClient::new(session_id)`, `push_frame`, `events`, `finalize` used identically in Tasks 2 and 3.
- `TranscriptionSession::start(session_id, frames, Box<dyn StreamingAsrClient>, EnergyEndpointer)` matches the trait and endpointer signatures.

Out of scope (future sub-projects): real cloud ASR WebSocket adapter implementing `StreamingAsrClient`; Tauri command + `emit` bridge forwarding `AsrEvent`s to the frontend.
