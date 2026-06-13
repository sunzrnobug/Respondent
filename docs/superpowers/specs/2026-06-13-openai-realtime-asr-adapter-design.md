# OpenAI Realtime ASR Adapter Design

Date: 2026-06-13

## Background And Scope

The ASR orchestration layer already accepts 16 kHz / mono / i16 loopback frames and emits provider-agnostic `AsrEvent`s through `StreamingAsrClient`. This design adds the first real cloud ASR implementation for Windows-first live meeting use: an OpenAI Realtime transcription adapter that preserves the existing low-latency local endpointing path.

The adapter targets OpenAI Realtime transcription over WebSocket. The official Realtime transcription guide specifies `gpt-realtime-whisper` for live transcript deltas, `audio.input.format` at 24 kHz mono PCM for `audio/pcm`, `input_audio_buffer.append` for audio chunks, and `input_audio_buffer.commit` when turn detection is disabled. The guide also recommends omitting or nulling `audio.input.turn_detection` for `gpt-realtime-whisper` when audio is committed manually. This maps cleanly to our local `EnergyEndpointer`: we stream audio continuously, then commit at the locally detected end of speech.

**In scope:**

- `OpenAiRealtimeAsrClient` implementing `StreamingAsrClient`.
- A small transport boundary so protocol logic is unit-tested without network.
- Real WebSocket transport using an API key from process configuration.
- 16 kHz internal `AudioFrame` to 24 kHz little-endian PCM payload conversion.
- Parsing Realtime transcription delta/completed/error events into `AsrEvent::Partial`, `AsrEvent::Final`, and `AsrError`.
- Deterministic unit tests with a recording/mock transport; no test requires network or credentials.

**Out of scope for this sub-project:**

- Tauri command wiring and event emission to the frontend. That is the next sub-project.
- Provider choice UI or runtime provider switching.
- Microphone capture. The adapter only consumes the existing loopback `AudioFrame` stream.
- Server-side VAD. We keep local endpointing to preserve the ~300 ms static endpoint latency target.
- Prompt steering. The current GA Realtime transcription guide says prompt is not supported for `gpt-realtime-whisper`.

## Design Decision

Use OpenAI Realtime transcription as a concrete `StreamingAsrClient`, with a transport trait below it:

```rust
pub trait RealtimeTransport: Send {
    fn send_json(&mut self, value: serde_json::Value) -> Result<(), AsrError>;
    fn try_recv_json(&mut self) -> Result<Option<serde_json::Value>, AsrError>;
    fn close(&mut self) -> Result<(), AsrError>;
}
```

The ASR adapter owns protocol state and timestamps. The transport only moves JSON over a connection. This keeps the important behavior testable:

- Session update shape.
- Audio frame resampling and base64 payload.
- Manual commit on `finalize()`.
- Delta accumulation and final mapping.
- Provider errors becoming `AsrError::Provider`.

The real transport is a thin synchronous WebSocket implementation. The client constructor can accept a boxed transport for tests, and a convenience constructor builds the real WebSocket transport from config for runtime use.

## Files And Responsibilities

- `src-tauri/src/asr/openai_realtime.rs`:
  - `OpenAiRealtimeAsrClient`
  - `OpenAiRealtimeConfig`
  - `RealtimeTransport`
  - `WebSocketRealtimeTransport`
  - protocol event parsing helpers
  - 16 kHz to 24 kHz PCM conversion helpers
- `src-tauri/src/asr/mod.rs`: export `openai_realtime`.
- `src-tauri/Cargo.toml`: add small transport dependencies.
- `src-tauri/tests/openai_realtime_asr.rs`: deterministic tests for protocol behavior and sample conversion.
- `src-tauri/tests/provider_contract.rs`: verify provider name only; existing mock contract remains unchanged.

## Configuration

```rust
pub struct OpenAiRealtimeConfig {
    pub api_key: String,
    pub model: String,
    pub language: Option<String>,
    pub transcription_delay: TranscriptionDelay,
}
```

Defaults:

- `model`: `gpt-realtime-whisper`
- `language`: `None` unless a command supplies a hint later
- `transcription_delay`: `Minimal`

Runtime credential loading will use `OPENAI_API_KEY`. Missing or blank API key returns `AsrError::Provider("missing OPENAI_API_KEY")`. The key is only used in the `Authorization: Bearer ...` WebSocket header and must not be serialized into app events, test snapshots, or logs.

## Realtime Session Update

On construction/start, the adapter sends one `session.update`:

```json
{
  "type": "session.update",
  "session": {
    "type": "transcription",
    "audio": {
      "input": {
        "format": { "type": "audio/pcm", "rate": 24000 },
        "transcription": {
          "model": "gpt-realtime-whisper",
          "delay": "minimal"
        },
        "turn_detection": null
      }
    }
  }
}
```

If `language` is present, add `"language": "<value>"` to `transcription`.

The current docs expose a nested GA shape for transcription sessions. The adapter should use that shape rather than the older beta `input_audio_format`/`input_audio_transcription` top-level fields.

## Audio Path

Input frames are still the project-local contract:

- 16 kHz
- mono
- i16
- typically 320 samples per 20 ms frame

OpenAI Realtime `audio/pcm` expects 24 kHz mono PCM. `push_frame()` therefore:

1. Validates mono i16 input.
2. Converts `i16` samples to normalized `f32`.
3. Uses the existing `LinearResampler` from 16 kHz to 24 kHz.
4. Converts resampled `f32` back to `i16`.
5. Writes little-endian bytes.
6. Base64 encodes the bytes.
7. Sends:

```json
{ "type": "input_audio_buffer.append", "audio": "<base64 pcm16>" }
```

For a 20 ms frame, 320 samples become about 480 samples at 24 kHz, or 960 bytes before base64. The exact count is tested against the existing linear resampler behavior.

## Event Mapping

`try_recv_json()` is drained after every `push_frame()` and after every `finalize()`.

Server event mapping:

- `conversation.item.input_audio_transcription.delta`:
  - Append `delta` to the current item text buffer keyed by `item_id`.
  - Emit `AsrEvent::Partial` with accumulated text for that item.
- `conversation.item.input_audio_transcription.completed`:
  - Use `transcript` as the final text.
  - Clear the `item_id` buffer.
  - Emit `AsrEvent::Final`.
- `error`:
  - Return `AsrError::Provider` with a concise provider message.
- Unknown events:
  - Ignore. Realtime sends lifecycle acknowledgements that are not transcript content.

Timing:

- `started_at_ms`: first frame timestamp for the open utterance.
- `ended_at_ms`: latest pushed frame end timestamp.
- `received_at_ms`: local wall-clock milliseconds at parse time for real runtime. Tests may inject a clock so assertions stay deterministic.

The adapter cannot rely on OpenAI completion ordering across speech turns; the docs say ordering between different turns is not guaranteed. For the first implementation, the local `TranscriptionSession` drives one open utterance at a time and commits only at endpoint. The adapter uses `item_id` buffers to avoid mixing deltas when delayed completion events arrive.

## Finalize

`finalize()` sends:

```json
{ "type": "input_audio_buffer.commit" }
```

Then it drains available provider events once. It must not block indefinitely waiting for a final transcript. This preserves the current `TranscriptionSession::stop()` and `Drop` behavior: client methods return promptly. If the final arrives later, the next `push_frame()` or bridge pump can drain it; the Tauri bridge will also run a small event pump in the next sub-project.

## Error Handling

- Transport send/receive/close failures become `AsrError::Provider`.
- Invalid input format becomes `AsrError::Provider("openai realtime asr expects 16 kHz mono i16 frames")`.
- Provider `error` events become `AsrError::Provider("openai realtime error: ...")`.
- Event channel disconnect maps to `AsrError::Closed`, matching existing clients.
- The API key is never included in error strings.

## Latency Posture

This design follows the selected low-latency strategy:

- Local endpointing remains the fixed trigger, with ~300 ms silence confirmation.
- Realtime transcription delay defaults to `minimal`.
- Server VAD is disabled/null so it cannot add a second endpoint window.
- No speculative reply generation is added here.
- `finalize()` is non-blocking; the bridge can continue pumping provider events without stalling shutdown.

## Tests

All tests are deterministic and offline.

- `new_sends_transcription_session_update`:
  - Construct with a recording transport.
  - Assert the first message is `session.update` with transcription session, 24 kHz audio format, model, delay, language when configured, and `turn_detection: null`.
- `push_frame_appends_24khz_base64_pcm`:
  - Push a 16 kHz mono frame.
  - Decode the append payload.
  - Assert little-endian PCM bytes and approximately 24 kHz sample count for the frame duration.
- `finalize_commits_without_blocking`:
  - Call `finalize()`.
  - Assert a commit message was sent and no final is required for success.
- `delta_accumulates_into_partial`:
  - Queue two provider delta events for the same `item_id`.
  - Drain through the adapter.
  - Assert partial text is accumulated.
- `completed_emits_final_and_clears_buffer`:
  - Queue delta then completed for the same `item_id`.
  - Assert final uses `transcript`, not the local accumulated text.
- `provider_error_event_returns_provider_error`:
  - Queue an `error` event.
  - Assert `push_frame()` or `finalize()` returns `AsrError::Provider` without secrets.
- `wrong_frame_format_is_rejected`:
  - Push a 48 kHz or stereo frame.
  - Assert provider error and no audio append sent.

## Acceptance Criteria

- `OpenAiRealtimeAsrClient` implements `StreamingAsrClient` and reports `openai-realtime-asr`.
- No unit test requires network or `OPENAI_API_KEY`.
- `cargo test --test openai_realtime_asr` passes.
- Existing ASR/LLM/provider contract tests still pass.
- `cargo test` and `cargo check` pass.
- No microphone/input-device capture path is introduced.
