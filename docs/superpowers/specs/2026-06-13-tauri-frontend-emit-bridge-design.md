# Tauri Frontend Emit Bridge Design

Date: 2026-06-13

## Background And Scope

The backend now has loopback capture, streaming ASR orchestration, streaming reply orchestration, and a real OpenAI Realtime ASR adapter. The frontend already has a `RealtimeEvent` contract and reducer, but the UI still uses `runMockRealtimeSession()` directly. This design adds the bridge that makes native Tauri sessions emit backend `AsrEvent` and `ReplyEvent` payloads into the frontend.

**In scope:**

- A backend session runtime managed by Tauri state.
- `start_session` starts loopback capture, ASR orchestration, reply orchestration, and bridge threads.
- `end_session` stops and removes the runtime for a session.
- Backend emits `AsrEvent`, `ReplyEvent`, and `system.status` payloads on a single event name: `realtime.event`.
- Frontend listens to `realtime.event`, validates payloads with `isRealtimeEvent`, and reduces them into `SessionState`.
- Browser/Vitest fallback remains the existing mock realtime session.

**Out of scope:**

- UI device picker. The frontend can choose the default output device from `list_audio_output_devices`.
- Real LLM cloud adapter. Replies still use `MockReplyClient`.
- Microphone capture. Runtime only uses WASAPI loopback output capture.
- Persistent transcript export wiring beyond existing domain/export code.

## Backend Runtime

`src-tauri/src/commands.rs` gains a `SessionManager`:

```rust
pub struct SessionManager {
    sessions: Mutex<HashMap<String, SessionRuntime>>,
}
```

Each `SessionRuntime` owns:

- `LoopbackCapture`
- `TranscriptionSession`
- `ReplySession`
- two bridge worker handles

`start_session(app, state, title, output_device_id)`:

1. Validate non-empty title and output device id.
2. Create `session-{timestamp}` id.
3. Start `LoopbackCapture::start(&output_device_id)`.
4. Choose ASR client:
   - If `OPENAI_API_KEY` is present and non-blank: `OpenAiRealtimeAsrClient::from_env(session_id.clone())`.
   - Otherwise: `MockAsrClient::new(session_id.clone())` and emit an info status saying mock ASR is active.
5. Start `TranscriptionSession` with `EnergyEndpointer::with_defaults()`.
6. Create a private ASR event channel for the reply session.
7. Spawn an ASR bridge thread:
   - Receives `AsrEvent` from `TranscriptionSession`.
   - Emits each event on `realtime.event`.
   - Forwards a clone to the reply ASR channel.
8. Start `ReplySession` with `MockReplyClient` and `ReplyTrigger`.
9. Spawn a reply bridge thread:
   - Receives `ReplyEvent`.
   - Emits each event on `realtime.event`.
10. Store runtime under the session id.
11. Emit `system.status` for session start and return the session id.

`end_session(state, session_id)`:

1. Validate non-empty id.
2. Remove runtime from the manager.
3. Stop capture/session/bridges.
4. Return `Ok(())` even if the id is already gone; the operation is idempotent after validation.

## Event Contract

The event name is:

```text
realtime.event
```

Payloads are already frontend-compatible:

- `AsrEvent` serializes to `transcript.partial`, `transcript.final`, and `endpoint.detected`.
- `ReplyEvent` serializes to `reply.started`, `reply.token`, and `reply.final`.
- New backend `SystemStatusEvent` serializes to:

```json
{
  "type": "system.status",
  "sessionId": "session-...",
  "level": "info",
  "message": "Native realtime session started",
  "receivedAtMs": 1760000000000
}
```

The API key is never emitted.

## Frontend Bridge

Create `src/services/realtimeBridge.ts`:

- `REALTIME_EVENT_NAME = "realtime.event"`.
- `isTauriRuntime()` checks for `window.__TAURI_INTERNALS__`.
- `listenNativeRealtimeEvents(emit)` calls Tauri `listen`, validates `event.payload` with `isRealtimeEvent`, and passes only valid events to `emit`.

Update `App.tsx`:

- In Tauri runtime:
  - Register native event listener.
  - Pick the default output device from `listAudioOutputDevices()`.
  - Call `startNativeSession("Meeting", device.id)`.
  - Set session state to the returned id.
  - On listener events, call `reduceSessionEvent`.
- Outside Tauri runtime:
  - Keep the existing mock realtime session.
- `end()` calls `endNativeSession(sessionId)` in Tauri runtime and always marks local state ended.

## Testing

Backend:

- Existing command validation tests move to pure helpers so they do not need `AppHandle`.
- Add a serialization test for `SystemStatusEvent`.
- `cargo test --test commands` remains deterministic and does not start audio.

Frontend:

- Mock `@tauri-apps/api/event` and test `listenNativeRealtimeEvents`:
  - valid payload is emitted,
  - invalid payload is ignored,
  - unlisten is returned.
- Existing App tests continue using mock fallback.

## Acceptance Criteria

- `start_session`/`end_session` are real Tauri bridge commands backed by session state.
- Frontend can consume native `realtime.event` payloads without changing `RealtimeEvent`.
- Browser/Vitest fallback remains green.
- `cargo test`, `cargo check`, and `npm test` pass.
- Privacy grep still finds no microphone/input-device capture path.
