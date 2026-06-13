# Tauri Frontend Emit Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect backend realtime ASR/reply sessions to the frontend by emitting validated `RealtimeEvent` payloads over Tauri's event bus.

**Architecture:** Backend `start_session` owns a session runtime in managed Tauri state and emits `AsrEvent`, `ReplyEvent`, and `system.status` on `realtime.event`. Frontend code listens to that event only in Tauri runtime; browser/Vitest fallback stays on the existing mock realtime session.

**Tech Stack:** Rust/Tauri 2, crossbeam-channel, serde, React, TypeScript, Vitest.

---

## File Structure

- `src-tauri/src/commands.rs` (modify): validation helpers, `SystemStatusEvent`, `SessionManager`, runtime start/stop, emit bridge threads, real Tauri command signatures.
- `src-tauri/src/lib.rs` (modify): manage `SessionManager`.
- `src-tauri/tests/commands.rs` (modify): test pure validation/helpers and system status serialization.
- `src/services/realtimeBridge.ts` (create): frontend Tauri event listener wrapper.
- `src/services/realtimeBridge.test.ts` (create): listener behavior tests.
- `src/App.tsx` (modify): native bridge path with mock fallback.
- `src/App.test.tsx` (modify only if async start requires wait handling).

Run Rust commands with cargo on PATH:

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
```

---

## Task 1: Backend Command Runtime And Emit Bridge

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/commands.rs`

- [ ] **Step 1: Write failing backend tests**

Update `src-tauri/tests/commands.rs` to import pure helpers instead of the Tauri command:

```rust
use respondent_lib::commands::{
    end_session_for_test, start_session_for_test, SystemStatusEvent,
};

#[test]
fn start_session_rejects_empty_title() {
    assert!(start_session_for_test(String::new(), "default-output".into()).is_err());
}

#[test]
fn start_session_rejects_empty_output_device() {
    assert!(start_session_for_test("Customer call".into(), String::new()).is_err());
}

#[test]
fn start_session_accepts_valid_input() {
    let id = start_session_for_test("Customer call".into(), "default-output".into())
        .expect("valid session start");
    assert!(id.starts_with("session-"));
}

#[test]
fn end_session_rejects_empty_id() {
    assert!(end_session_for_test(String::new()).is_err());
}

#[test]
fn end_session_accepts_non_empty_id() {
    assert!(end_session_for_test("session-123".into()).is_ok());
}

#[test]
fn system_status_serializes_to_frontend_contract() {
    let event = SystemStatusEvent::info(Some("s1".to_string()), "ready");
    let value = serde_json::to_value(&event).expect("serialize");
    assert_eq!(value["type"], "system.status");
    assert_eq!(value["sessionId"], "s1");
    assert_eq!(value["level"], "info");
    assert_eq!(value["message"], "ready");
    assert!(value["receivedAtMs"].as_i64().unwrap() > 0);
}
```

- [ ] **Step 2: Run tests to verify RED**

```powershell
cd src-tauri
cargo test --test commands
cd ..
```

Expected: compile failure for missing helpers/types.

- [ ] **Step 3: Implement backend runtime**

In `src-tauri/src/commands.rs`, replace the simple command-only module with:

- `pub const REALTIME_EVENT_NAME: &str = "realtime.event";`
- `SystemStatusEvent` serializing to frontend contract.
- `start_session_for_test` and `end_session_for_test` pure helper functions.
- `SessionManager` with `Mutex<HashMap<String, SessionRuntime>>`.
- Tauri `start_session(app: tauri::AppHandle, state: tauri::State<'_, SessionManager>, title, output_device_id)`.
- Tauri `end_session(state: tauri::State<'_, SessionManager>, session_id)`.
- `SessionRuntime::start` that starts:
  - `LoopbackCapture::start(&output_device_id)`,
  - ASR client (`OpenAiRealtimeAsrClient` if `OPENAI_API_KEY` exists, otherwise `MockAsrClient`),
  - `TranscriptionSession`,
  - ASR bridge thread,
  - `ReplySession` with `MockReplyClient`,
  - reply bridge thread.
- Bridge threads should use `recv_timeout(100ms)`, a stop flag, `app.emit(REALTIME_EVENT_NAME, event)`, and clean shutdown.
- `SessionRuntime::stop(self)` stops capture, transcription, reply, bridge handles.

Keep errors as `String` in command boundaries. Do not include API keys in error/status messages.

Modify `src-tauri/src/lib.rs`:

```rust
tauri::Builder::default()
    .manage(commands::SessionManager::default())
```

- [ ] **Step 4: Run backend tests**

```powershell
cd src-tauri
cargo test --test commands
cargo check
cd ..
```

Expected: commands tests pass, check passes.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/tests/commands.rs
git commit -m "feat: bridge backend realtime sessions to tauri events"
```

---

## Task 2: Frontend Native Realtime Listener

**Files:**
- Create: `src/services/realtimeBridge.ts`
- Create: `src/services/realtimeBridge.test.ts`

- [ ] **Step 1: Write failing frontend tests**

Create `src/services/realtimeBridge.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RealtimeEvent } from "../domain/events";

const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

import {
  listenNativeRealtimeEvents,
  REALTIME_EVENT_NAME,
} from "./realtimeBridge";

describe("realtimeBridge", () => {
  beforeEach(() => {
    listenMock.mockReset();
  });

  it("listens on the realtime event name and forwards valid payloads", async () => {
    const unlisten = vi.fn();
    listenMock.mockImplementation(async (_name, handler) => {
      handler({
        payload: {
          type: "transcript.partial",
          sessionId: "s1",
          text: "hello",
          startedAtMs: 0,
          endedAtMs: 20,
          receivedAtMs: 25,
        } satisfies RealtimeEvent,
      });
      return unlisten;
    });
    const emit = vi.fn();

    const stop = await listenNativeRealtimeEvents(emit);

    expect(listenMock).toHaveBeenCalledWith(
      REALTIME_EVENT_NAME,
      expect.any(Function),
    );
    expect(emit).toHaveBeenCalledWith(
      expect.objectContaining({ type: "transcript.partial", text: "hello" }),
    );
    stop();
    expect(unlisten).toHaveBeenCalled();
  });

  it("ignores invalid payloads", async () => {
    listenMock.mockImplementation(async (_name, handler) => {
      handler({ payload: { type: "nope" } });
      return vi.fn();
    });
    const emit = vi.fn();

    await listenNativeRealtimeEvents(emit);

    expect(emit).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run tests to verify RED**

```powershell
npm test -- realtimeBridge
```

Expected: module not found.

- [ ] **Step 3: Implement bridge service**

Create `src/services/realtimeBridge.ts`:

```ts
import { listen } from "@tauri-apps/api/event";
import { isRealtimeEvent, type RealtimeEvent } from "../domain/events";
import type { StopRealtimeSession } from "./mockRealtime";

export const REALTIME_EVENT_NAME = "realtime.event";

export function isTauriRuntime(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in (window as unknown as Record<string, unknown>)
  );
}

export async function listenNativeRealtimeEvents(
  emit: (event: RealtimeEvent) => void,
): Promise<StopRealtimeSession> {
  const unlisten = await listen<unknown>(REALTIME_EVENT_NAME, (event) => {
    if (isRealtimeEvent(event.payload)) {
      emit(event.payload);
    }
  });
  return unlisten;
}
```

- [ ] **Step 4: Run tests**

```powershell
npm test -- realtimeBridge
```

Expected: bridge tests pass.

- [ ] **Step 5: Commit**

```powershell
git add src/services/realtimeBridge.ts src/services/realtimeBridge.test.ts
git commit -m "feat: add frontend native realtime event listener"
```

---

## Task 3: App Uses Native Bridge With Mock Fallback

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/App.test.tsx` if needed

- [ ] **Step 1: Update tests for async start if needed**

Run current App test first:

```powershell
npm test -- App
```

If it fails after implementation because start is async, wrap Start clicks in `await act(async () => fireEvent.click(...))` or use `findByText`.

- [ ] **Step 2: Implement App native branch**

Modify `src/App.tsx`:

- Import `listAudioOutputDevices`, `startNativeSession`, `endNativeSession`.
- Import `isTauriRuntime`, `listenNativeRealtimeEvents`.
- In `start()`:
  - Stop previous session.
  - If not Tauri: keep current mock path.
  - If Tauri:
    - register listener,
    - list devices,
    - choose default or first,
    - call `startNativeSession("Meeting", device.id)`,
    - set initial state with returned id,
    - store a stop function that unlistens and calls `endNativeSession(sessionId)`.
    - On failure, set `systemMessages` with a readable error and status `idle`.
- In `end()`:
  - call stop function and mark ended.

- [ ] **Step 3: Run frontend tests**

```powershell
npm test -- App realtimeBridge
```

Expected: tests pass.

- [ ] **Step 4: Commit**

```powershell
git add src/App.tsx src/App.test.tsx
git commit -m "feat: use native realtime bridge in tauri runtime"
```

---

## Task 4: Full Bridge Verification

**Files:**
- No production changes expected unless verification finds defects.

- [ ] **Step 1: Rust verification**

```powershell
cd src-tauri
cargo test
cargo check
cd ..
```

Expected: all non-ignored Rust tests pass.

- [ ] **Step 2: Frontend verification**

```powershell
npm test
```

Expected: all Vitest suites pass.

- [ ] **Step 3: Privacy grep**

```powershell
rg -n "eCapture|microphone|\bmic\b|input device|recording device" src-tauri/src
```

Expected: no matches; exit code 1 is expected for no matches.

- [ ] **Step 4: Commit verification fixes if any**

Only commit if verification required code changes:

```powershell
git add <changed-files>
git commit -m "fix: stabilize tauri realtime bridge"
```

---

## Handoff Notes

- Keep frontend mock fallback for tests/browser preview.
- Do not add microphone capture.
- Do not emit secrets.
- Use `MockReplyClient` until a real LLM cloud adapter is planned.
