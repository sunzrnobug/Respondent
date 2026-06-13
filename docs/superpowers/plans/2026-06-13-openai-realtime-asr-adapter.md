# OpenAI Realtime ASR Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a real OpenAI Realtime transcription ASR adapter that implements the existing `StreamingAsrClient` contract while keeping all tests deterministic and offline.

**Architecture:** Put OpenAI-specific protocol logic in `src-tauri/src/asr/openai_realtime.rs` behind a tiny `RealtimeTransport` trait. Unit tests use a recording transport; runtime uses a synchronous WebSocket transport. The adapter keeps local endpointing semantics: `push_frame()` appends 24 kHz PCM audio and drains transcript events; `finalize()` sends a manual commit and returns promptly.

**Tech Stack:** Rust, serde_json, crossbeam-channel, base64, tungstenite with native TLS. Existing audio conversion uses `LinearResampler`; no async runtime.

---

## File Structure

- `src-tauri/src/asr/openai_realtime.rs` (create): config, delay enum, transport trait, OpenAI client, real WebSocket transport, event parsing, audio conversion.
- `src-tauri/src/asr/mod.rs` (modify): export `openai_realtime`.
- `src-tauri/Cargo.toml` (modify): add `base64` and `tungstenite`.
- `src-tauri/tests/openai_realtime_asr.rs` (create): deterministic offline tests.
- `src-tauri/tests/provider_contract.rs` (modify): add provider name assertion for the OpenAI adapter using a recording transport.

Run Rust commands with cargo on PATH:

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
```

---

## Task 1: Module, Config, Transport Boundary, Session Update

**Files:**
- Create: `src-tauri/src/asr/openai_realtime.rs`
- Modify: `src-tauri/src/asr/mod.rs`
- Create: `src-tauri/tests/openai_realtime_asr.rs`

- [ ] **Step 1: Write failing tests**

Create `src-tauri/tests/openai_realtime_asr.rs`:

```rust
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use respondent_lib::asr::client::{AsrError, StreamingAsrClient};
use respondent_lib::asr::openai_realtime::{
    OpenAiRealtimeAsrClient, OpenAiRealtimeConfig, RealtimeTransport, TranscriptionDelay,
};
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct RecordingTransport {
    sent: Arc<Mutex<Vec<Value>>>,
    incoming: Arc<Mutex<VecDeque<Value>>>,
}

impl RecordingTransport {
    fn sent(&self) -> Vec<Value> {
        self.sent.lock().unwrap().clone()
    }

    fn queue(&self, value: Value) {
        self.incoming.lock().unwrap().push_back(value);
    }
}

impl RealtimeTransport for RecordingTransport {
    fn send_json(&mut self, value: Value) -> Result<(), AsrError> {
        self.sent.lock().unwrap().push(value);
        Ok(())
    }

    fn try_recv_json(&mut self) -> Result<Option<Value>, AsrError> {
        Ok(self.incoming.lock().unwrap().pop_front())
    }

    fn close(&mut self) -> Result<(), AsrError> {
        Ok(())
    }
}

fn config() -> OpenAiRealtimeConfig {
    OpenAiRealtimeConfig {
        api_key: "test-key".to_string(),
        model: "gpt-realtime-whisper".to_string(),
        language: Some("en".to_string()),
        transcription_delay: TranscriptionDelay::Minimal,
    }
}

#[test]
fn new_sends_transcription_session_update() {
    let transport = RecordingTransport::default();
    let sent = transport.clone();

    let client = OpenAiRealtimeAsrClient::with_transport(
        "s1".to_string(),
        config(),
        Box::new(transport),
    )
    .expect("client");

    assert_eq!(client.name(), "openai-realtime-asr");
    let messages = sent.sent();
    assert_eq!(messages.len(), 1);
    let update = &messages[0];
    assert_eq!(update["type"], "session.update");
    assert_eq!(update["session"]["type"], "transcription");
    assert_eq!(update["session"]["audio"]["input"]["format"], json!({
        "type": "audio/pcm",
        "rate": 24000
    }));
    assert_eq!(
        update["session"]["audio"]["input"]["transcription"]["model"],
        "gpt-realtime-whisper"
    );
    assert_eq!(
        update["session"]["audio"]["input"]["transcription"]["language"],
        "en"
    );
    assert_eq!(
        update["session"]["audio"]["input"]["transcription"]["delay"],
        "minimal"
    );
    assert!(update["session"]["audio"]["input"]["turn_detection"].is_null());
}

#[test]
fn default_config_uses_low_latency_model_and_delay() {
    let cfg = OpenAiRealtimeConfig::from_api_key("k");
    assert_eq!(cfg.model, "gpt-realtime-whisper");
    assert_eq!(cfg.language, None);
    assert_eq!(cfg.transcription_delay, TranscriptionDelay::Minimal);
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cd src-tauri
cargo test --test openai_realtime_asr
cd ..
```

Expected: compile failure mentioning `respondent_lib::asr::openai_realtime` is unresolved.

- [ ] **Step 3: Implement minimal module/config/session update**

Create `src-tauri/src/asr/openai_realtime.rs`:

```rust
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde_json::{json, Value};

use crate::audio::frame::AudioFrame;

use super::client::{AsrError, AsrEvent, StreamingAsrClient};

const OPENAI_REALTIME_SAMPLE_RATE: u32 = 24_000;

pub trait RealtimeTransport: Send {
    fn send_json(&mut self, value: Value) -> Result<(), AsrError>;
    fn try_recv_json(&mut self) -> Result<Option<Value>, AsrError>;
    fn close(&mut self) -> Result<(), AsrError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptionDelay {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

impl TranscriptionDelay {
    fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiRealtimeConfig {
    pub api_key: String,
    pub model: String,
    pub language: Option<String>,
    pub transcription_delay: TranscriptionDelay,
}

impl OpenAiRealtimeConfig {
    pub fn from_api_key(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "gpt-realtime-whisper".to_string(),
            language: None,
            transcription_delay: TranscriptionDelay::Minimal,
        }
    }
}

pub struct OpenAiRealtimeAsrClient {
    session_id: String,
    config: OpenAiRealtimeConfig,
    transport: Box<dyn RealtimeTransport>,
    sender: Sender<AsrEvent>,
    receiver: Receiver<AsrEvent>,
    item_text: HashMap<String, String>,
    utterance_started_at_ms: Option<i64>,
    last_frame_ended_at_ms: i64,
}

impl OpenAiRealtimeAsrClient {
    pub fn with_transport(
        session_id: String,
        config: OpenAiRealtimeConfig,
        transport: Box<dyn RealtimeTransport>,
    ) -> Result<Self, AsrError> {
        if config.api_key.trim().is_empty() {
            return Err(AsrError::Provider("missing OPENAI_API_KEY".to_string()));
        }

        let (sender, receiver) = unbounded();
        let mut client = Self {
            session_id,
            config,
            transport,
            sender,
            receiver,
            item_text: HashMap::new(),
            utterance_started_at_ms: None,
            last_frame_ended_at_ms: 0,
        };
        client.send_session_update()?;
        Ok(client)
    }

    fn send_session_update(&mut self) -> Result<(), AsrError> {
        let mut transcription = json!({
            "model": self.config.model,
            "delay": self.config.transcription_delay.as_str(),
        });
        if let Some(language) = &self.config.language {
            transcription["language"] = json!(language);
        }

        self.transport.send_json(json!({
            "type": "session.update",
            "session": {
                "type": "transcription",
                "audio": {
                    "input": {
                        "format": {
                            "type": "audio/pcm",
                            "rate": OPENAI_REALTIME_SAMPLE_RATE,
                        },
                        "transcription": transcription,
                        "turn_detection": Value::Null,
                    }
                }
            }
        }))
    }

    fn drain_provider_events(&mut self) -> Result<(), AsrError> {
        while let Some(value) = self.transport.try_recv_json()? {
            self.handle_provider_event(value)?;
        }
        Ok(())
    }

    fn handle_provider_event(&mut self, value: Value) -> Result<(), AsrError> {
        if value["type"] == "error" {
            return Err(AsrError::Provider("openai realtime error".to_string()));
        }
        Ok(())
    }
}

impl StreamingAsrClient for OpenAiRealtimeAsrClient {
    fn name(&self) -> &'static str {
        "openai-realtime-asr"
    }

    fn push_frame(&mut self, _frame: &AudioFrame) -> Result<(), AsrError> {
        self.drain_provider_events()
    }

    fn events(&self) -> Receiver<AsrEvent> {
        self.receiver.clone()
    }

    fn finalize(&mut self) -> Result<(), AsrError> {
        self.transport.send_json(json!({ "type": "input_audio_buffer.commit" }))?;
        self.drain_provider_events()
    }
}

impl Drop for OpenAiRealtimeAsrClient {
    fn drop(&mut self) {
        let _ = self.transport.close();
    }
}
```

Modify `src-tauri/src/asr/mod.rs`:

```rust
pub mod client;
pub mod endpointer;
pub mod mock;
pub mod openai_realtime;
pub mod session;
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```powershell
cd src-tauri
cargo test --test openai_realtime_asr
cd ..
```

Expected: 2 passed. Warnings are acceptable only if fixed before commit.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/asr/openai_realtime.rs src-tauri/src/asr/mod.rs src-tauri/tests/openai_realtime_asr.rs
git commit -m "feat: add openai realtime asr session setup"
```

---

## Task 2: Frame Validation, Resampling, Base64 Append

**Files:**
- Modify: `src-tauri/src/asr/openai_realtime.rs`
- Modify: `src-tauri/tests/openai_realtime_asr.rs`

- [ ] **Step 1: Add failing audio append tests**

Append to `src-tauri/tests/openai_realtime_asr.rs`:

```rust
use base64::{engine::general_purpose::STANDARD, Engine as _};
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};

fn mono_16k_frame(samples: Vec<i16>, captured_at_ms: u64) -> AudioFrame {
    AudioFrame {
        format: PcmFormat {
            sample_rate: 16_000,
            channels: 1,
            bits_per_sample: 16,
        },
        samples,
        captured_at_ms,
    }
}

#[test]
fn push_frame_appends_24khz_base64_pcm() {
    let transport = RecordingTransport::default();
    let sent = transport.clone();
    let mut client = OpenAiRealtimeAsrClient::with_transport(
        "s1".to_string(),
        config(),
        Box::new(transport),
    )
    .expect("client");

    client
        .push_frame(&mono_16k_frame(vec![1000; 320], 100))
        .expect("push");

    let messages = sent.sent();
    let append = messages
        .iter()
        .find(|message| message["type"] == "input_audio_buffer.append")
        .expect("append message");
    let audio = append["audio"].as_str().expect("base64 audio");
    let bytes = STANDARD.decode(audio).expect("decode pcm");
    assert_eq!(bytes.len(), 960);
    assert_eq!(bytes[0], 232);
    assert_eq!(bytes[1], 3);
}

#[test]
fn wrong_frame_format_is_rejected_without_append() {
    let transport = RecordingTransport::default();
    let sent = transport.clone();
    let mut client = OpenAiRealtimeAsrClient::with_transport(
        "s1".to_string(),
        config(),
        Box::new(transport),
    )
    .expect("client");

    let err = client
        .push_frame(&AudioFrame {
            format: PcmFormat {
                sample_rate: 48_000,
                channels: 2,
                bits_per_sample: 16,
            },
            samples: vec![0; 960],
            captured_at_ms: 0,
        })
        .expect_err("reject wrong format");

    assert!(err.to_string().contains("expects 16 kHz mono i16 frames"));
    assert!(
        sent.sent()
            .iter()
            .all(|message| message["type"] != "input_audio_buffer.append")
    );
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cd src-tauri
cargo test --test openai_realtime_asr push_frame_appends_24khz_base64_pcm
cargo test --test openai_realtime_asr wrong_frame_format_is_rejected_without_append
cd ..
```

Expected: append test fails because no append is sent; format test fails because the current stub accepts the frame.

- [ ] **Step 3: Add dependency and implementation**

Modify `src-tauri/Cargo.toml` dependencies:

```toml
base64 = "0.22"
tungstenite = { version = "0.24", features = ["native-tls"] }
```

In `src-tauri/src/asr/openai_realtime.rs`, add:

```rust
use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::audio::convert::{to_pcm16, LinearResampler};
```

Add a `resampler: LinearResampler` field to `OpenAiRealtimeAsrClient`, initialized as `LinearResampler::new(16_000, OPENAI_REALTIME_SAMPLE_RATE)`.

Replace `push_frame()` with:

```rust
fn push_frame(&mut self, frame: &AudioFrame) -> Result<(), AsrError> {
    self.validate_frame(frame)?;
    if self.utterance_started_at_ms.is_none() {
        self.utterance_started_at_ms = Some(frame.captured_at_ms as i64);
    }
    self.last_frame_ended_at_ms = frame.captured_at_ms as i64 + frame.duration_ms() as i64;

    let payload = self.encode_frame(frame);
    self.transport.send_json(json!({
        "type": "input_audio_buffer.append",
        "audio": payload,
    }))?;
    self.drain_provider_events()
}
```

Add helper methods:

```rust
fn validate_frame(&self, frame: &AudioFrame) -> Result<(), AsrError> {
    if frame.format.sample_rate != 16_000
        || frame.format.channels != 1
        || frame.format.bits_per_sample != 16
    {
        return Err(AsrError::Provider(
            "openai realtime asr expects 16 kHz mono i16 frames".to_string(),
        ));
    }
    Ok(())
}

fn encode_frame(&mut self, frame: &AudioFrame) -> String {
    let normalized: Vec<f32> = frame
        .samples
        .iter()
        .map(|sample| *sample as f32 / i16::MAX as f32)
        .collect();
    let resampled = self.resampler.process(&normalized);
    let pcm = to_pcm16(&resampled);
    let mut bytes = Vec::with_capacity(pcm.len() * 2);
    for sample in pcm {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    STANDARD.encode(bytes)
}
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```powershell
cd src-tauri
cargo test --test openai_realtime_asr
cd ..
```

Expected: 4 passed.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/asr/openai_realtime.rs src-tauri/tests/openai_realtime_asr.rs
git commit -m "feat: append openai realtime asr audio frames"
```

---

## Task 3: Provider Event Parsing, Partial/Final Events, Commit Behavior

**Files:**
- Modify: `src-tauri/src/asr/openai_realtime.rs`
- Modify: `src-tauri/tests/openai_realtime_asr.rs`

- [ ] **Step 1: Add failing event mapping tests**

Append to `src-tauri/tests/openai_realtime_asr.rs`:

```rust
use respondent_lib::asr::client::AsrEvent;

#[test]
fn finalize_commits_without_requiring_a_final() {
    let transport = RecordingTransport::default();
    let sent = transport.clone();
    let mut client = OpenAiRealtimeAsrClient::with_transport(
        "s1".to_string(),
        config(),
        Box::new(transport),
    )
    .expect("client");

    client.finalize().expect("commit");

    assert!(
        sent.sent()
            .iter()
            .any(|message| message["type"] == "input_audio_buffer.commit")
    );
}

#[test]
fn delta_accumulates_into_partial() {
    let transport = RecordingTransport::default();
    transport.queue(json!({
        "type": "conversation.item.input_audio_transcription.delta",
        "item_id": "item_1",
        "delta": "Hello"
    }));
    transport.queue(json!({
        "type": "conversation.item.input_audio_transcription.delta",
        "item_id": "item_1",
        "delta": ", world"
    }));
    let mut client = OpenAiRealtimeAsrClient::with_transport(
        "s1".to_string(),
        config(),
        Box::new(transport),
    )
    .expect("client");
    let events = client.events();

    client.push_frame(&mono_16k_frame(vec![0; 320], 100)).expect("push");

    let first = events.try_recv().expect("first partial");
    let second = events.try_recv().expect("second partial");
    assert!(matches!(first, AsrEvent::Partial { ref text, .. } if text == "Hello"));
    assert!(matches!(second, AsrEvent::Partial { ref text, .. } if text == "Hello, world"));
}

#[test]
fn completed_emits_final_and_clears_buffer() {
    let transport = RecordingTransport::default();
    transport.queue(json!({
        "type": "conversation.item.input_audio_transcription.delta",
        "item_id": "item_1",
        "delta": "draft"
    }));
    transport.queue(json!({
        "type": "conversation.item.input_audio_transcription.completed",
        "item_id": "item_1",
        "transcript": "final text"
    }));
    let mut client = OpenAiRealtimeAsrClient::with_transport(
        "s1".to_string(),
        config(),
        Box::new(transport),
    )
    .expect("client");
    let events = client.events();

    client.push_frame(&mono_16k_frame(vec![0; 320], 100)).expect("push");

    let all: Vec<AsrEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(
        all.iter()
            .any(|event| matches!(event, AsrEvent::Final { text, .. } if text == "final text")),
        "expected final in {all:?}"
    );
}

#[test]
fn provider_error_event_returns_provider_error_without_secret() {
    let transport = RecordingTransport::default();
    transport.queue(json!({
        "type": "error",
        "error": {
            "message": "bad audio"
        }
    }));
    let mut client = OpenAiRealtimeAsrClient::with_transport(
        "s1".to_string(),
        config(),
        Box::new(transport),
    )
    .expect("client");

    let err = client
        .push_frame(&mono_16k_frame(vec![0; 320], 100))
        .expect_err("provider error");
    let message = err.to_string();
    assert!(message.contains("bad audio"));
    assert!(!message.contains("test-key"));
}
```

- [ ] **Step 2: Run tests to verify RED**

Run:

```powershell
cd src-tauri
cargo test --test openai_realtime_asr delta_accumulates_into_partial
cargo test --test openai_realtime_asr completed_emits_final_and_clears_buffer
cargo test --test openai_realtime_asr provider_error_event_returns_provider_error_without_secret
cd ..
```

Expected: mapping tests fail because `handle_provider_event()` ignores transcript events and error details.

- [ ] **Step 3: Implement event mapping**

Replace `handle_provider_event()` in `src-tauri/src/asr/openai_realtime.rs`:

```rust
fn handle_provider_event(&mut self, value: Value) -> Result<(), AsrError> {
    match value["type"].as_str() {
        Some("conversation.item.input_audio_transcription.delta") => {
            let item_id = value["item_id"].as_str().unwrap_or("unknown").to_string();
            let delta = value["delta"].as_str().unwrap_or_default();
            let text = self.item_text.entry(item_id).or_default();
            text.push_str(delta);
            self.sender
                .send(AsrEvent::Partial {
                    session_id: self.session_id.clone(),
                    text: text.clone(),
                    started_at_ms: self.utterance_started_at_ms.unwrap_or(0),
                    ended_at_ms: self.last_frame_ended_at_ms,
                    received_at_ms: now_ms(),
                })
                .map_err(|_| AsrError::Closed)?;
            Ok(())
        }
        Some("conversation.item.input_audio_transcription.completed") => {
            let item_id = value["item_id"].as_str().unwrap_or("unknown");
            let transcript = value["transcript"].as_str().unwrap_or_default().to_string();
            self.item_text.remove(item_id);
            self.sender
                .send(AsrEvent::Final {
                    session_id: self.session_id.clone(),
                    text: transcript,
                    started_at_ms: self.utterance_started_at_ms.unwrap_or(0),
                    ended_at_ms: self.last_frame_ended_at_ms,
                    received_at_ms: now_ms(),
                })
                .map_err(|_| AsrError::Closed)?;
            Ok(())
        }
        Some("error") => {
            let detail = value["error"]["message"]
                .as_str()
                .or_else(|| value["message"].as_str())
                .unwrap_or("provider error");
            Err(AsrError::Provider(format!("openai realtime error: {detail}")))
        }
        _ => Ok(()),
    }
}
```

Add:

```rust
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
```

- [ ] **Step 4: Run tests to verify GREEN**

Run:

```powershell
cd src-tauri
cargo test --test openai_realtime_asr
cd ..
```

Expected: all OpenAI adapter tests pass.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/asr/openai_realtime.rs src-tauri/tests/openai_realtime_asr.rs
git commit -m "feat: map openai realtime asr transcript events"
```

---

## Task 4: Real WebSocket Transport And Provider Contract

**Files:**
- Modify: `src-tauri/src/asr/openai_realtime.rs`
- Modify: `src-tauri/tests/provider_contract.rs`

- [ ] **Step 1: Add failing provider contract test**

Modify `src-tauri/tests/provider_contract.rs` imports:

```rust
use respondent_lib::asr::openai_realtime::{
    OpenAiRealtimeAsrClient, OpenAiRealtimeConfig, RealtimeTransport,
};
use serde_json::Value;
```

Add a tiny local transport to the test file:

```rust
struct ContractTransport;

impl RealtimeTransport for ContractTransport {
    fn send_json(&mut self, _value: Value) -> Result<(), respondent_lib::asr::client::AsrError> {
        Ok(())
    }

    fn try_recv_json(&mut self) -> Result<Option<Value>, respondent_lib::asr::client::AsrError> {
        Ok(None)
    }

    fn close(&mut self) -> Result<(), respondent_lib::asr::client::AsrError> {
        Ok(())
    }
}
```

Extend `mock_clients_report_their_names()`:

```rust
let openai = OpenAiRealtimeAsrClient::with_transport(
    "s1".to_string(),
    OpenAiRealtimeConfig::from_api_key("test-key"),
    Box::new(ContractTransport),
)
.expect("openai client");
assert_eq!(openai.name(), "openai-realtime-asr");
```

- [ ] **Step 2: Run tests to verify current contract still passes/fails usefully**

Run:

```powershell
cd src-tauri
cargo test --test provider_contract
cd ..
```

Expected: passes after Task 1; this step is still required to protect imports before adding the real transport.

- [ ] **Step 3: Implement WebSocket transport and runtime constructor**

In `src-tauri/src/asr/openai_realtime.rs`, add imports:

```rust
use std::net::TcpStream;

use tungstenite::client::IntoClientRequest;
use tungstenite::http::HeaderValue;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{connect, Message, WebSocket};
```

Add:

```rust
pub struct WebSocketRealtimeTransport {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl WebSocketRealtimeTransport {
    pub fn connect(config: &OpenAiRealtimeConfig) -> Result<Self, AsrError> {
        if config.api_key.trim().is_empty() {
            return Err(AsrError::Provider("missing OPENAI_API_KEY".to_string()));
        }
        let url = format!(
            "wss://api.openai.com/v1/realtime?model={}",
            config.model
        );
        let mut request = url
            .into_client_request()
            .map_err(|err| AsrError::Provider(format!("openai realtime request: {err}")))?;
        let auth = format!("Bearer {}", config.api_key);
        let auth = HeaderValue::from_str(&auth)
            .map_err(|err| AsrError::Provider(format!("openai realtime auth header: {err}")))?;
        request.headers_mut().insert("Authorization", auth);
        let (socket, _) = connect(request)
            .map_err(|err| AsrError::Provider(format!("openai realtime connect: {err}")))?;
        Ok(Self { socket })
    }
}

impl RealtimeTransport for WebSocketRealtimeTransport {
    fn send_json(&mut self, value: Value) -> Result<(), AsrError> {
        self.socket
            .send(Message::Text(value.to_string()))
            .map_err(|err| AsrError::Provider(format!("openai realtime send: {err}")))
    }

    fn try_recv_json(&mut self) -> Result<Option<Value>, AsrError> {
        match self.socket.read() {
            Ok(Message::Text(text)) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|err| AsrError::Provider(format!("openai realtime json: {err}"))),
            Ok(Message::Binary(_)) | Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => Ok(None),
            Ok(Message::Close(_)) => Err(AsrError::Closed),
            Err(tungstenite::Error::Io(err))
                if err.kind() == std::io::ErrorKind::WouldBlock =>
            {
                Ok(None)
            }
            Err(err) => Err(AsrError::Provider(format!("openai realtime receive: {err}"))),
        }
    }

    fn close(&mut self) -> Result<(), AsrError> {
        self.socket
            .close(None)
            .map_err(|err| AsrError::Provider(format!("openai realtime close: {err}")))
    }
}
```

Add runtime constructors:

```rust
impl OpenAiRealtimeAsrClient {
    pub fn connect(session_id: String, config: OpenAiRealtimeConfig) -> Result<Self, AsrError> {
        let transport = WebSocketRealtimeTransport::connect(&config)?;
        Self::with_transport(session_id, config, Box::new(transport))
    }

    pub fn from_env(session_id: String) -> Result<Self, AsrError> {
        let api_key = std::env::var("OPENAI_API_KEY")
            .map_err(|_| AsrError::Provider("missing OPENAI_API_KEY".to_string()))?;
        Self::connect(session_id, OpenAiRealtimeConfig::from_api_key(api_key))
    }
}
```

If `socket.read()` blocks in practice, set the underlying stream to nonblocking immediately after `connect` when the concrete `MaybeTlsStream` exposes a `get_ref()` TcpStream; otherwise keep this transport constructor unused by tests and note the residual runtime risk in the final review. Do not make `finalize()` wait for a final transcript.

- [ ] **Step 4: Run focused tests and check**

Run:

```powershell
cd src-tauri
cargo test --test provider_contract
cargo test --test openai_realtime_asr
cargo check
cd ..
```

Expected: all pass. If the `tungstenite` API differs, adapt imports/types to the installed version while preserving behavior.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/asr/openai_realtime.rs src-tauri/tests/provider_contract.rs
git commit -m "feat: add openai realtime asr websocket transport"
```

---

## Task 5: ASR Adapter Full Verification

**Files:**
- No production files expected unless verification finds a defect.

- [ ] **Step 1: Run ASR/OpenAI focused tests**

Run:

```powershell
cd src-tauri
cargo test --test openai_realtime_asr
cargo test --test asr_orchestration
cargo test --test provider_contract
cd ..
```

Expected: all pass.

- [ ] **Step 2: Run full Rust verification**

Run:

```powershell
cd src-tauri
cargo test
cargo check
cd ..
```

Expected: all non-ignored Rust tests pass; `loopback_capture_smoke` remains ignored unless explicitly enabled.

- [ ] **Step 3: Run frontend tests**

Run:

```powershell
npm test
```

Expected: all Vitest suites pass.

- [ ] **Step 4: Verify privacy boundary**

Run:

```powershell
rg -n "eCapture|microphone|\bmic\b|input device|recording device" src-tauri/src
```

Expected: no matches in implementation paths; `rg` exits 1 when there are no matches.

- [ ] **Step 5: Commit any verification-only fixes**

If verification required code changes, commit them:

```powershell
git add <changed-files>
git commit -m "fix: stabilize openai realtime asr adapter"
```

If no changes are needed, do not create an empty commit.

---

## Handoff Notes

- Keep all adapter tests offline.
- Never put `OPENAI_API_KEY` into committed docs beyond the variable name.
- Do not add a microphone/input-device path.
- Keep `finalize()` non-blocking with respect to provider final text.
- The next plan will wire `TranscriptionSession` and `ReplySession` to Tauri `emit` events and frontend state.
