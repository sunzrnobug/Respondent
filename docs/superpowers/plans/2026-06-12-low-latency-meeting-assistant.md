# Low-Latency Meeting Assistant Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Windows-first floating meeting assistant that captures only system output audio, shows low-latency subtitles, streams suggested replies after endpoint/final transcript events, and saves the whole session text.

**Architecture:** Use a Tauri desktop app with a Rust backend for Windows WASAPI loopback, SQLite storage, and command/event bridges to a React TypeScript floating UI. Keep latency-sensitive work off the UI thread: audio capture, ASR streaming, LLM streaming, and SQLite writes each run behind focused interfaces and are first verified with deterministic mocks.

**Tech Stack:** Tauri, Rust, React, TypeScript, Vitest, SQLite, WASAPI loopback, streaming ASR over WebSocket, streaming LLM over HTTP/SSE or WebSocket.

---

## Scope Check

The spec covers one MVP with several subsystems. This plan keeps it as one implementation plan because each task produces a working, testable increment:

- scaffold and test harness
- typed event contracts
- session persistence
- transcript endpointing
- reply trigger policy
- mock real-time pipeline
- Windows audio capture
- provider adapters
- floating window UI
- end-to-end latency verification

Execution should not start with real ASR or WASAPI. The first usable vertical slice should run with mock audio/transcript/reply events so UI, persistence, and trigger semantics can be tested before hardware and network variables enter the loop.

## File Structure

Create this project layout:

```text
E:\Respondent\
  docs\
    superpowers\
      specs\
        2026-06-12-low-latency-meeting-assistant-design.md
      plans\
        2026-06-12-low-latency-meeting-assistant.md
  package.json
  vite.config.ts
  vitest.config.ts
  index.html
  src\
    main.tsx
    App.tsx
    styles.css
    domain\
      events.ts
      endpointing.ts
      transcriptEngine.ts
      replyEngine.ts
      exportTranscript.ts
    services\
      mockRealtime.ts
      tauriApi.ts
    state\
      sessionStore.ts
    test\
      setup.ts
  src-tauri\
    Cargo.toml
    tauri.conf.json
    src\
      main.rs
      lib.rs
      commands.rs
      audio\
        mod.rs
        devices.rs
        capture.rs
        frame.rs
      asr\
        mod.rs
        client.rs
        mock.rs
      llm\
        mod.rs
        client.rs
        mock.rs
      session\
        mod.rs
        db.rs
        export.rs
      telemetry\
        mod.rs
        latency.rs
    tests\
      session_db.rs
      audio_contract.rs
```

Responsibilities:

- `src/domain/*`: pure TypeScript logic; no Tauri imports.
- `src/services/*`: frontend adapters for mock mode and Tauri commands/events.
- `src/state/sessionStore.ts`: UI state reducer/store for active session display.
- `src-tauri/src/audio/*`: Windows output device enumeration and loopback capture.
- `src-tauri/src/asr/*`: streaming ASR interface and mock implementation.
- `src-tauri/src/llm/*`: streaming reply interface and mock implementation.
- `src-tauri/src/session/*`: SQLite schema, writes, and exports.
- `src-tauri/src/telemetry/*`: latency marks and metrics emitted to UI.

## Task 1: Repository And App Scaffold

**Files:**
- Create: `package.json`
- Create: `vite.config.ts`
- Create: `vitest.config.ts`
- Create: `index.html`
- Create: `src/main.tsx`
- Create: `src/App.tsx`
- Create: `src/styles.css`
- Create: `src/test/setup.ts`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`

- [ ] **Step 1: Initialize git and scaffold the app**

Run:

```powershell
git init
npm create tauri-app@latest . -- --template react-ts
npm install
npm install -D vitest @testing-library/react @testing-library/jest-dom @testing-library/user-event jsdom
```

Expected:

```text
Initialized empty Git repository
added ... packages
```

- [ ] **Step 2: Replace `package.json` scripts**

Use this scripts block:

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest",
    "tauri": "tauri",
    "tauri:dev": "tauri dev",
    "tauri:build": "tauri build"
  }
}
```

- [ ] **Step 3: Create Vitest config**

Create `vitest.config.ts`:

```ts
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["src/test/setup.ts"],
  },
});
```

- [ ] **Step 4: Create test setup**

Create `src/test/setup.ts`:

```ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 5: Verify frontend test runner**

Run:

```powershell
npm test
```

Expected:

```text
No test files found
```

The command may exit non-zero because no tests exist yet. After Task 2 it must pass.

- [ ] **Step 6: Verify Rust project compiles**

Run:

```powershell
cd src-tauri
cargo check
cd ..
```

Expected:

```text
Finished `dev` profile
```

- [ ] **Step 7: Commit scaffold**

Run:

```powershell
git add package.json package-lock.json vite.config.ts vitest.config.ts index.html src src-tauri docs
git commit -m "chore: scaffold low latency meeting assistant"
```

Expected:

```text
[main ...] chore: scaffold low latency meeting assistant
```

## Task 2: Shared Frontend Event Contracts

**Files:**
- Create: `src/domain/events.ts`
- Create: `src/domain/events.test.ts`

- [ ] **Step 1: Write failing tests for event guards**

Create `src/domain/events.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { isRealtimeEvent } from "./events";

describe("isRealtimeEvent", () => {
  it("accepts partial transcript events", () => {
    expect(
      isRealtimeEvent({
        type: "transcript.partial",
        sessionId: "s1",
        text: "hello",
        startedAtMs: 10,
        endedAtMs: 320,
        receivedAtMs: 350,
      }),
    ).toBe(true);
  });

  it("accepts reply token events", () => {
    expect(
      isRealtimeEvent({
        type: "reply.token",
        sessionId: "s1",
        generationId: "g1",
        token: "Yes",
        receivedAtMs: 500,
      }),
    ).toBe(true);
  });

  it("rejects microphone events because the MVP has no microphone path", () => {
    expect(
      isRealtimeEvent({
        type: "microphone.frame",
        sessionId: "s1",
      }),
    ).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```powershell
npm test -- src/domain/events.test.ts
```

Expected:

```text
Cannot find module './events'
```

- [ ] **Step 3: Implement event types and guard**

Create `src/domain/events.ts`:

```ts
export type TranscriptPartialEvent = {
  type: "transcript.partial";
  sessionId: string;
  text: string;
  startedAtMs: number;
  endedAtMs: number;
  receivedAtMs: number;
};

export type TranscriptFinalEvent = {
  type: "transcript.final";
  sessionId: string;
  text: string;
  startedAtMs: number;
  endedAtMs: number;
  receivedAtMs: number;
};

export type EndpointEvent = {
  type: "endpoint.detected";
  sessionId: string;
  silenceMs: number;
  detectedAtMs: number;
};

export type ReplyStartedEvent = {
  type: "reply.started";
  sessionId: string;
  generationId: string;
  basedOnTranscriptEventId: string;
  receivedAtMs: number;
};

export type ReplyTokenEvent = {
  type: "reply.token";
  sessionId: string;
  generationId: string;
  token: string;
  receivedAtMs: number;
};

export type ReplyFinalEvent = {
  type: "reply.final";
  sessionId: string;
  generationId: string;
  text: string;
  receivedAtMs: number;
};

export type SystemEvent = {
  type: "system.status";
  sessionId?: string;
  level: "info" | "warning" | "error";
  message: string;
  receivedAtMs: number;
};

export type RealtimeEvent =
  | TranscriptPartialEvent
  | TranscriptFinalEvent
  | EndpointEvent
  | ReplyStartedEvent
  | ReplyTokenEvent
  | ReplyFinalEvent
  | SystemEvent;

const allowedTypes = new Set<RealtimeEvent["type"]>([
  "transcript.partial",
  "transcript.final",
  "endpoint.detected",
  "reply.started",
  "reply.token",
  "reply.final",
  "system.status",
]);

export function isRealtimeEvent(value: unknown): value is RealtimeEvent {
  if (!value || typeof value !== "object") return false;
  const candidate = value as { type?: unknown };
  return typeof candidate.type === "string" && allowedTypes.has(candidate.type as RealtimeEvent["type"]);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:

```powershell
npm test -- src/domain/events.test.ts
```

Expected:

```text
PASS src/domain/events.test.ts
```

- [ ] **Step 5: Commit contracts**

Run:

```powershell
git add src/domain/events.ts src/domain/events.test.ts
git commit -m "feat: add realtime event contracts"
```

## Task 3: Transcript Engine And Endpoint Policy

**Files:**
- Create: `src/domain/endpointing.ts`
- Create: `src/domain/endpointing.test.ts`
- Create: `src/domain/transcriptEngine.ts`
- Create: `src/domain/transcriptEngine.test.ts`

- [ ] **Step 1: Write failing endpoint policy tests**

Create `src/domain/endpointing.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { chooseEndpointSilenceMs } from "./endpointing";

describe("chooseEndpointSilenceMs", () => {
  it("uses 300 ms for balanced clean speech", () => {
    expect(chooseEndpointSilenceMs({ noiseLevel: "low", recentFalseCuts: 0, utteranceMs: 1800 })).toBe(300);
  });

  it("uses 250 ms for very short clean utterances", () => {
    expect(chooseEndpointSilenceMs({ noiseLevel: "low", recentFalseCuts: 0, utteranceMs: 650 })).toBe(250);
  });

  it("widens to 500 ms after repeated false cuts", () => {
    expect(chooseEndpointSilenceMs({ noiseLevel: "medium", recentFalseCuts: 2, utteranceMs: 2400 })).toBe(500);
  });
});
```

- [ ] **Step 2: Run endpoint tests to verify failure**

Run:

```powershell
npm test -- src/domain/endpointing.test.ts
```

Expected:

```text
Cannot find module './endpointing'
```

- [ ] **Step 3: Implement endpoint policy**

Create `src/domain/endpointing.ts`:

```ts
export type NoiseLevel = "low" | "medium" | "high";

export type EndpointPolicyInput = {
  noiseLevel: NoiseLevel;
  recentFalseCuts: number;
  utteranceMs: number;
};

export function chooseEndpointSilenceMs(input: EndpointPolicyInput): number {
  if (input.noiseLevel === "high" || input.recentFalseCuts >= 2) return 500;
  if (input.noiseLevel === "medium" || input.recentFalseCuts === 1) return 400;
  if (input.utteranceMs <= 900) return 250;
  return 300;
}
```

- [ ] **Step 4: Write transcript engine tests**

Create `src/domain/transcriptEngine.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { createTranscriptEngine } from "./transcriptEngine";

describe("transcript engine", () => {
  it("shows partial text but only stores final text in history", () => {
    const engine = createTranscriptEngine("s1");
    engine.apply({ type: "transcript.partial", sessionId: "s1", text: "I can", startedAtMs: 0, endedAtMs: 300, receivedAtMs: 350 });
    engine.apply({ type: "transcript.partial", sessionId: "s1", text: "I can help", startedAtMs: 0, endedAtMs: 700, receivedAtMs: 760 });
    engine.apply({ type: "transcript.final", sessionId: "s1", text: "I can help.", startedAtMs: 0, endedAtMs: 900, receivedAtMs: 1100 });

    expect(engine.snapshot().livePartial).toBe("");
    expect(engine.snapshot().finalTurns).toEqual([
      { text: "I can help.", startedAtMs: 0, endedAtMs: 900 },
    ]);
  });

  it("ignores events from another session", () => {
    const engine = createTranscriptEngine("s1");
    engine.apply({ type: "transcript.final", sessionId: "s2", text: "wrong session", startedAtMs: 0, endedAtMs: 100, receivedAtMs: 150 });
    expect(engine.snapshot().finalTurns).toEqual([]);
  });
});
```

- [ ] **Step 5: Implement transcript engine**

Create `src/domain/transcriptEngine.ts`:

```ts
import type { RealtimeEvent } from "./events";

export type TranscriptTurn = {
  text: string;
  startedAtMs: number;
  endedAtMs: number;
};

export type TranscriptSnapshot = {
  sessionId: string;
  livePartial: string;
  finalTurns: TranscriptTurn[];
};

export function createTranscriptEngine(sessionId: string) {
  let livePartial = "";
  const finalTurns: TranscriptTurn[] = [];

  return {
    apply(event: RealtimeEvent) {
      if ("sessionId" in event && event.sessionId !== sessionId) return;

      if (event.type === "transcript.partial") {
        livePartial = event.text;
      }

      if (event.type === "transcript.final") {
        livePartial = "";
        finalTurns.push({
          text: event.text,
          startedAtMs: event.startedAtMs,
          endedAtMs: event.endedAtMs,
        });
      }
    },
    snapshot(): TranscriptSnapshot {
      return {
        sessionId,
        livePartial,
        finalTurns: [...finalTurns],
      };
    },
  };
}
```

- [ ] **Step 6: Run tests**

Run:

```powershell
npm test -- src/domain/endpointing.test.ts src/domain/transcriptEngine.test.ts
```

Expected:

```text
PASS src/domain/endpointing.test.ts
PASS src/domain/transcriptEngine.test.ts
```

- [ ] **Step 7: Commit transcript logic**

Run:

```powershell
git add src/domain/endpointing.ts src/domain/endpointing.test.ts src/domain/transcriptEngine.ts src/domain/transcriptEngine.test.ts
git commit -m "feat: add transcript endpoint policy"
```

## Task 4: Reply Trigger Engine

**Files:**
- Create: `src/domain/replyEngine.ts`
- Create: `src/domain/replyEngine.test.ts`

- [ ] **Step 1: Write failing trigger tests**

Create `src/domain/replyEngine.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { createReplyEngine } from "./replyEngine";

describe("reply engine", () => {
  it("does not trigger from partial transcript alone", () => {
    const engine = createReplyEngine("s1");
    const action = engine.apply({
      type: "transcript.partial",
      sessionId: "s1",
      text: "Can you explain",
      startedAtMs: 0,
      endedAtMs: 400,
      receivedAtMs: 450,
    });
    expect(action).toEqual({ type: "none" });
  });

  it("triggers after endpoint and final transcript", () => {
    const engine = createReplyEngine("s1");
    engine.apply({ type: "endpoint.detected", sessionId: "s1", silenceMs: 300, detectedAtMs: 1200 });
    const action = engine.apply({
      type: "transcript.final",
      sessionId: "s1",
      text: "Can you explain the timeline?",
      startedAtMs: 0,
      endedAtMs: 1100,
      receivedAtMs: 1300,
    });
    expect(action).toEqual({
      type: "start-reply",
      transcript: "Can you explain the timeline?",
      context: ["Can you explain the timeline?"],
    });
  });

  it("keeps only the latest six final turns in context", () => {
    const engine = createReplyEngine("s1");
    for (let index = 0; index < 7; index += 1) {
      engine.apply({ type: "endpoint.detected", sessionId: "s1", silenceMs: 300, detectedAtMs: index * 1000 + 500 });
      engine.apply({
        type: "transcript.final",
        sessionId: "s1",
        text: `turn ${index}`,
        startedAtMs: index * 1000,
        endedAtMs: index * 1000 + 400,
        receivedAtMs: index * 1000 + 600,
      });
    }
    expect(engine.context()).toEqual(["turn 1", "turn 2", "turn 3", "turn 4", "turn 5", "turn 6"]);
  });
});
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```powershell
npm test -- src/domain/replyEngine.test.ts
```

Expected:

```text
Cannot find module './replyEngine'
```

- [ ] **Step 3: Implement reply trigger engine**

Create `src/domain/replyEngine.ts`:

```ts
import type { RealtimeEvent } from "./events";

export type ReplyAction =
  | { type: "none" }
  | { type: "start-reply"; transcript: string; context: string[] };

export function createReplyEngine(sessionId: string) {
  let endpointArmed = false;
  const finalTurns: string[] = [];

  return {
    apply(event: RealtimeEvent): ReplyAction {
      if ("sessionId" in event && event.sessionId !== sessionId) return { type: "none" };

      if (event.type === "endpoint.detected") {
        endpointArmed = true;
        return { type: "none" };
      }

      if (event.type === "transcript.final") {
        finalTurns.push(event.text);
        while (finalTurns.length > 6) finalTurns.shift();

        if (endpointArmed) {
          endpointArmed = false;
          return {
            type: "start-reply",
            transcript: event.text,
            context: [...finalTurns],
          };
        }
      }

      return { type: "none" };
    },
    context() {
      return [...finalTurns];
    },
  };
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
npm test -- src/domain/replyEngine.test.ts
```

Expected:

```text
PASS src/domain/replyEngine.test.ts
```

- [ ] **Step 5: Commit reply trigger logic**

Run:

```powershell
git add src/domain/replyEngine.ts src/domain/replyEngine.test.ts
git commit -m "feat: add endpoint triggered reply engine"
```

## Task 5: Session Export Logic

**Files:**
- Create: `src/domain/exportTranscript.ts`
- Create: `src/domain/exportTranscript.test.ts`

- [ ] **Step 1: Write failing export tests**

Create `src/domain/exportTranscript.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { exportMarkdown, exportPlainText, type SessionExport } from "./exportTranscript";

const session: SessionExport = {
  title: "Customer call",
  startedAt: "2026-06-12T08:00:00.000Z",
  endedAt: "2026-06-12T08:05:00.000Z",
  events: [
    { type: "transcript", text: "What is the timeline?", atMs: 1200 },
    { type: "suggestion", text: "We can deliver the first draft by Friday.", atMs: 2100 },
  ],
};

describe("session export", () => {
  it("exports Markdown with timestamps and suggestions", () => {
    expect(exportMarkdown(session)).toContain("## Customer call");
    expect(exportMarkdown(session)).toContain("[00:01.200] Transcript: What is the timeline?");
    expect(exportMarkdown(session)).toContain("[00:02.100] Suggestion: We can deliver the first draft by Friday.");
  });

  it("exports plain text", () => {
    expect(exportPlainText(session)).toBe(
      "Customer call\nStarted: 2026-06-12T08:00:00.000Z\nEnded: 2026-06-12T08:05:00.000Z\n\n[00:01.200] Transcript: What is the timeline?\n[00:02.100] Suggestion: We can deliver the first draft by Friday.\n",
    );
  });
});
```

- [ ] **Step 2: Run test to verify failure**

Run:

```powershell
npm test -- src/domain/exportTranscript.test.ts
```

Expected:

```text
Cannot find module './exportTranscript'
```

- [ ] **Step 3: Implement export functions**

Create `src/domain/exportTranscript.ts`:

```ts
export type SessionExportEvent = {
  type: "transcript" | "suggestion" | "system";
  text: string;
  atMs: number;
};

export type SessionExport = {
  title: string;
  startedAt: string;
  endedAt: string;
  events: SessionExportEvent[];
};

function formatOffset(ms: number): string {
  const minutes = Math.floor(ms / 60000).toString().padStart(2, "0");
  const seconds = Math.floor((ms % 60000) / 1000).toString().padStart(2, "0");
  const millis = Math.floor(ms % 1000).toString().padStart(3, "0");
  return `${minutes}:${seconds}.${millis}`;
}

function label(type: SessionExportEvent["type"]): string {
  if (type === "transcript") return "Transcript";
  if (type === "suggestion") return "Suggestion";
  return "System";
}

export function exportPlainText(session: SessionExport): string {
  const header = `${session.title}\nStarted: ${session.startedAt}\nEnded: ${session.endedAt}\n\n`;
  const body = session.events
    .map((event) => `[${formatOffset(event.atMs)}] ${label(event.type)}: ${event.text}`)
    .join("\n");
  return `${header}${body}\n`;
}

export function exportMarkdown(session: SessionExport): string {
  const lines = [
    `## ${session.title}`,
    "",
    `- Started: ${session.startedAt}`,
    `- Ended: ${session.endedAt}`,
    "",
    "### Timeline",
    "",
    ...session.events.map((event) => `- [${formatOffset(event.atMs)}] ${label(event.type)}: ${event.text}`),
    "",
  ];
  return lines.join("\n");
}
```

- [ ] **Step 4: Run tests**

Run:

```powershell
npm test -- src/domain/exportTranscript.test.ts
```

Expected:

```text
PASS src/domain/exportTranscript.test.ts
```

- [ ] **Step 5: Commit export logic**

Run:

```powershell
git add src/domain/exportTranscript.ts src/domain/exportTranscript.test.ts
git commit -m "feat: add session text exports"
```

## Task 6: Mock Real-Time Pipeline

**Files:**
- Create: `src/services/mockRealtime.ts`
- Create: `src/services/mockRealtime.test.ts`
- Create: `src/state/sessionStore.ts`
- Create: `src/state/sessionStore.test.ts`

- [ ] **Step 1: Write store tests**

Create `src/state/sessionStore.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { createInitialSessionState, reduceSessionEvent } from "./sessionStore";

describe("session store", () => {
  it("updates subtitle and reply text from realtime events", () => {
    let state = createInitialSessionState("s1");
    state = reduceSessionEvent(state, { type: "transcript.partial", sessionId: "s1", text: "hello", startedAtMs: 0, endedAtMs: 300, receivedAtMs: 340 });
    state = reduceSessionEvent(state, { type: "transcript.final", sessionId: "s1", text: "hello there", startedAtMs: 0, endedAtMs: 600, receivedAtMs: 800 });
    state = reduceSessionEvent(state, { type: "reply.started", sessionId: "s1", generationId: "g1", basedOnTranscriptEventId: "t1", receivedAtMs: 850 });
    state = reduceSessionEvent(state, { type: "reply.token", sessionId: "s1", generationId: "g1", token: "Sure", receivedAtMs: 900 });
    state = reduceSessionEvent(state, { type: "reply.token", sessionId: "s1", generationId: "g1", token: ", I can help.", receivedAtMs: 960 });

    expect(state.liveSubtitle).toBe("");
    expect(state.transcript).toEqual(["hello there"]);
    expect(state.currentSuggestion).toBe("Sure, I can help.");
  });
});
```

- [ ] **Step 2: Implement session store**

Create `src/state/sessionStore.ts`:

```ts
import type { RealtimeEvent } from "../domain/events";

export type SessionState = {
  sessionId: string;
  status: "idle" | "listening" | "paused" | "ended";
  liveSubtitle: string;
  transcript: string[];
  currentGenerationId: string | null;
  currentSuggestion: string;
  suggestions: string[];
  systemMessages: string[];
};

export function createInitialSessionState(sessionId: string): SessionState {
  return {
    sessionId,
    status: "listening",
    liveSubtitle: "",
    transcript: [],
    currentGenerationId: null,
    currentSuggestion: "",
    suggestions: [],
    systemMessages: [],
  };
}

export function reduceSessionEvent(state: SessionState, event: RealtimeEvent): SessionState {
  if ("sessionId" in event && event.sessionId && event.sessionId !== state.sessionId) return state;

  if (event.type === "transcript.partial") {
    return { ...state, liveSubtitle: event.text };
  }

  if (event.type === "transcript.final") {
    return { ...state, liveSubtitle: "", transcript: [...state.transcript, event.text] };
  }

  if (event.type === "reply.started") {
    return { ...state, currentGenerationId: event.generationId, currentSuggestion: "" };
  }

  if (event.type === "reply.token" && event.generationId === state.currentGenerationId) {
    return { ...state, currentSuggestion: `${state.currentSuggestion}${event.token}` };
  }

  if (event.type === "reply.final" && event.generationId === state.currentGenerationId) {
    return {
      ...state,
      currentSuggestion: event.text,
      suggestions: [...state.suggestions, event.text],
    };
  }

  if (event.type === "system.status") {
    return { ...state, systemMessages: [...state.systemMessages, event.message] };
  }

  return state;
}
```

- [ ] **Step 3: Write mock pipeline test**

Create `src/services/mockRealtime.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { runMockRealtimeSession } from "./mockRealtime";

describe("mock realtime session", () => {
  it("emits partial, final, endpoint, and reply tokens in order", async () => {
    vi.useFakeTimers();
    const events: string[] = [];
    const stop = runMockRealtimeSession("s1", (event) => events.push(event.type));

    await vi.advanceTimersByTimeAsync(2500);
    stop();

    expect(events).toEqual([
      "system.status",
      "transcript.partial",
      "transcript.partial",
      "endpoint.detected",
      "transcript.final",
      "reply.started",
      "reply.token",
      "reply.token",
      "reply.final",
    ]);
    vi.useRealTimers();
  });
});
```

- [ ] **Step 4: Implement mock pipeline**

Create `src/services/mockRealtime.ts`:

```ts
import type { RealtimeEvent } from "../domain/events";

export function runMockRealtimeSession(sessionId: string, emit: (event: RealtimeEvent) => void): () => void {
  const timers: number[] = [];
  const schedule = (delayMs: number, event: RealtimeEvent) => {
    const timer = window.setTimeout(() => emit(event), delayMs);
    timers.push(timer);
  };

  schedule(0, { type: "system.status", sessionId, level: "info", message: "Mock session started", receivedAtMs: 0 });
  schedule(200, { type: "transcript.partial", sessionId, text: "Can you explain", startedAtMs: 0, endedAtMs: 500, receivedAtMs: 200 });
  schedule(650, { type: "transcript.partial", sessionId, text: "Can you explain the timeline?", startedAtMs: 0, endedAtMs: 1000, receivedAtMs: 650 });
  schedule(1300, { type: "endpoint.detected", sessionId, silenceMs: 300, detectedAtMs: 1300 });
  schedule(1450, { type: "transcript.final", sessionId, text: "Can you explain the timeline?", startedAtMs: 0, endedAtMs: 1100, receivedAtMs: 1450 });
  schedule(1700, { type: "reply.started", sessionId, generationId: "mock-g1", basedOnTranscriptEventId: "mock-t1", receivedAtMs: 1700 });
  schedule(1900, { type: "reply.token", sessionId, generationId: "mock-g1", token: "Yes. ", receivedAtMs: 1900 });
  schedule(2150, { type: "reply.token", sessionId, generationId: "mock-g1", token: "The first version can be ready by Friday.", receivedAtMs: 2150 });
  schedule(2300, { type: "reply.final", sessionId, generationId: "mock-g1", text: "Yes. The first version can be ready by Friday.", receivedAtMs: 2300 });

  return () => {
    for (const timer of timers) window.clearTimeout(timer);
  };
}
```

- [ ] **Step 5: Run tests**

Run:

```powershell
npm test -- src/state/sessionStore.test.ts src/services/mockRealtime.test.ts
```

Expected:

```text
PASS src/state/sessionStore.test.ts
PASS src/services/mockRealtime.test.ts
```

- [ ] **Step 6: Commit mock pipeline**

Run:

```powershell
git add src/services/mockRealtime.ts src/services/mockRealtime.test.ts src/state/sessionStore.ts src/state/sessionStore.test.ts
git commit -m "feat: add mock realtime session pipeline"
```

## Task 7: Floating Window UI With Mock Mode

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/main.tsx`
- Modify: `src/styles.css`

- [ ] **Step 1: Replace `src/App.tsx` with mock-powered UI**

Use:

```tsx
import { useMemo, useRef, useState } from "react";
import { Copy, Pause, Play, Square, ChevronDown, ChevronUp } from "lucide-react";
import { runMockRealtimeSession } from "./services/mockRealtime";
import { createInitialSessionState, reduceSessionEvent, type SessionState } from "./state/sessionStore";
import "./styles.css";

function createSessionId() {
  return `session-${Date.now()}`;
}

export default function App() {
  const [session, setSession] = useState<SessionState>(() => createInitialSessionState("idle"));
  const [historyOpen, setHistoryOpen] = useState(false);
  const stopRef = useRef<null | (() => void)>(null);

  const isListening = session.status === "listening";
  const statusText = useMemo(() => {
    if (session.status === "listening") return "Listening";
    if (session.status === "paused") return "Paused";
    if (session.status === "ended") return "Saved";
    return "Ready";
  }, [session.status]);

  function start() {
    const sessionId = createSessionId();
    stopRef.current?.();
    setSession(createInitialSessionState(sessionId));
    stopRef.current = runMockRealtimeSession(sessionId, (event) => {
      setSession((current) => reduceSessionEvent(current, event));
    });
  }

  function pause() {
    stopRef.current?.();
    stopRef.current = null;
    setSession((current) => ({ ...current, status: "paused" }));
  }

  function end() {
    stopRef.current?.();
    stopRef.current = null;
    setSession((current) => ({ ...current, status: "ended", liveSubtitle: "" }));
  }

  async function copySuggestion() {
    await navigator.clipboard.writeText(session.currentSuggestion);
  }

  return (
    <main className="shell">
      <header className="topbar">
        <div className="status">
          <span className={isListening ? "dot dotLive" : "dot"} />
          <span>{statusText}</span>
        </div>
        <div className="actions">
          <button type="button" onClick={start} title="Start"><Play size={16} /></button>
          <button type="button" onClick={pause} title="Pause"><Pause size={16} /></button>
          <button type="button" onClick={end} title="End"><Square size={16} /></button>
        </div>
      </header>

      <section className="panel">
        <div className="label">Subtitle</div>
        <p className={session.liveSubtitle ? "subtitle partial" : "subtitle"}>
          {session.liveSubtitle || session.transcript.at(-1) || "Start a session to see live subtitles."}
        </p>
      </section>

      <section className="panel replyPanel">
        <div className="row">
          <div className="label">Suggested reply</div>
          <button type="button" onClick={copySuggestion} disabled={!session.currentSuggestion} title="Copy suggestion">
            <Copy size={16} />
          </button>
        </div>
        <p className="reply">{session.currentSuggestion || "The reply will stream here after an endpoint and final transcript."}</p>
      </section>

      <section className="history">
        <button className="historyToggle" type="button" onClick={() => setHistoryOpen((value) => !value)}>
          <span>Session history</span>
          {historyOpen ? <ChevronUp size={16} /> : <ChevronDown size={16} />}
        </button>
        {historyOpen ? (
          <div className="historyBody">
            {session.transcript.map((text, index) => <p key={`${text}-${index}`}>{text}</p>)}
            {session.suggestions.map((text, index) => <p className="suggestionItem" key={`${text}-${index}`}>{text}</p>)}
          </div>
        ) : null}
      </section>
    </main>
  );
}
```

- [ ] **Step 2: Install icon dependency**

Run:

```powershell
npm install lucide-react
```

Expected:

```text
added ... packages
```

- [ ] **Step 3: Replace `src/styles.css`**

Use:

```css
:root {
  color: #f7f7f5;
  background: transparent;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
}

body {
  margin: 0;
  min-width: 360px;
  background: transparent;
}

button {
  border: 1px solid rgba(255, 255, 255, 0.16);
  background: rgba(255, 255, 255, 0.08);
  color: inherit;
  width: 32px;
  height: 32px;
  display: inline-grid;
  place-items: center;
  border-radius: 6px;
  cursor: pointer;
}

button:disabled {
  opacity: 0.45;
  cursor: default;
}

.shell {
  width: min(560px, 100vw);
  box-sizing: border-box;
  padding: 10px;
  background: rgba(22, 25, 28, 0.92);
  border: 1px solid rgba(255, 255, 255, 0.16);
  border-radius: 8px;
  box-shadow: 0 18px 48px rgba(0, 0, 0, 0.28);
}

.topbar,
.row,
.historyToggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.topbar {
  height: 36px;
  -webkit-app-region: drag;
}

.actions,
.actions button,
.replyPanel button,
.historyToggle {
  -webkit-app-region: no-drag;
}

.status {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #d9e2df;
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #8a9490;
}

.dotLive {
  background: #38d996;
  box-shadow: 0 0 0 4px rgba(56, 217, 150, 0.14);
}

.actions {
  display: flex;
}

.panel {
  padding: 10px 0;
  border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.label {
  font-size: 11px;
  line-height: 16px;
  color: #aab5b0;
  text-transform: uppercase;
}

.subtitle,
.reply {
  min-height: 44px;
  margin: 6px 0 0;
  font-size: 16px;
  line-height: 1.45;
}

.partial {
  color: #cad6d0;
}

.reply {
  color: #fff3c4;
}

.history {
  border-top: 1px solid rgba(255, 255, 255, 0.1);
}

.historyToggle {
  width: 100%;
  padding: 8px 0 0;
  border: 0;
  background: transparent;
  height: auto;
  font-size: 13px;
}

.historyBody {
  max-height: 180px;
  overflow: auto;
  padding-top: 8px;
  font-size: 13px;
  color: #d8dfdc;
}

.historyBody p {
  margin: 0 0 8px;
}

.suggestionItem {
  color: #fff3c4;
}
```

- [ ] **Step 4: Run frontend checks**

Run:

```powershell
npm test
npm run build
```

Expected:

```text
Test Files ... passed
built in ...
```

- [ ] **Step 5: Run the mock UI**

Run:

```powershell
npm run dev
```

Expected:

```text
Local: http://localhost:5173/
```

Open the local URL and verify:

- Start shows partial subtitle.
- Final subtitle replaces partial without duplicate visible live text.
- Suggested reply streams after endpoint/final.
- Copy button copies generated text.
- End changes status to Saved.

- [ ] **Step 6: Commit mock UI**

Run:

```powershell
git add package.json package-lock.json src/App.tsx src/styles.css
git commit -m "feat: add floating assistant mock UI"
```

## Task 8: Rust Session Database And Export Commands

**Files:**
- Create: `src-tauri/src/session/mod.rs`
- Create: `src-tauri/src/session/db.rs`
- Create: `src-tauri/src/session/export.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/tests/session_db.rs`

- [ ] **Step 1: Add Rust dependencies**

Modify `src-tauri/Cargo.toml`:

```toml
[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.32", features = ["bundled"] }
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
thiserror = "1"
```

- [ ] **Step 2: Write database integration test**

Create `src-tauri/tests/session_db.rs`:

```rust
use respondent_lib::session::db::{EventInsert, SessionDb};

#[test]
fn creates_session_and_exports_events() {
    let db = SessionDb::open_in_memory().expect("open db");
    let session_id = db.start_session("Customer call", "default-output").expect("start session");

    db.insert_event(EventInsert {
        session_id: session_id.clone(),
        event_type: "transcript".into(),
        text: "What is the timeline?".into(),
        is_final: true,
        started_at_ms: 0,
        ended_at_ms: 1200,
    }).expect("insert transcript");

    db.insert_event(EventInsert {
        session_id: session_id.clone(),
        event_type: "suggestion".into(),
        text: "We can deliver the first draft by Friday.".into(),
        is_final: true,
        started_at_ms: 1500,
        ended_at_ms: 2400,
    }).expect("insert suggestion");

    db.end_session(&session_id).expect("end session");
    let export = db.load_export(&session_id).expect("load export");

    assert_eq!(export.title, "Customer call");
    assert_eq!(export.events.len(), 2);
    assert_eq!(export.events[0].text, "What is the timeline?");
}
```

- [ ] **Step 3: Run test to verify failure**

Run:

```powershell
cd src-tauri
cargo test --test session_db
cd ..
```

Expected:

```text
unresolved import `respondent_lib::session`
```

- [ ] **Step 4: Implement session module**

Create `src-tauri/src/session/mod.rs`:

```rust
pub mod db;
pub mod export;
```

Create `src-tauri/src/session/export.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct SessionExport {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub events: Vec<SessionExportEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionExportEvent {
    pub event_type: String,
    pub text: String,
    pub is_final: bool,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
}
```

Create `src-tauri/src/session/db.rs`:

```rust
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use super::export::{SessionExport, SessionExportEvent};

#[derive(Debug, Clone)]
pub struct EventInsert {
    pub session_id: String,
    pub event_type: String,
    pub text: String,
    pub is_final: bool,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
}

pub struct SessionDb {
    conn: Connection,
}

impl SessionDb {
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let db = Self { conn: Connection::open_in_memory()? };
        db.migrate()?;
        Ok(db)
    }

    pub fn open(path: &std::path::Path) -> rusqlite::Result<Self> {
        let db = Self { conn: Connection::open(path)? };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                output_device_id TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                text TEXT NOT NULL,
                is_final INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                ended_at_ms INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            );
            ",
        )
    }

    pub fn start_session(&self, title: &str, output_device_id: &str) -> rusqlite::Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (id, title, output_device_id, started_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, title, output_device_id, now, now],
        )?;
        Ok(id)
    }

    pub fn end_session(&self, session_id: &str) -> rusqlite::Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute("UPDATE sessions SET ended_at = ?1 WHERE id = ?2", params![now, session_id])?;
        Ok(())
    }

    pub fn insert_event(&self, event: EventInsert) -> rusqlite::Result<()> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO events (id, session_id, event_type, text, is_final, started_at_ms, ended_at_ms, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                event.session_id,
                event.event_type,
                event.text,
                if event.is_final { 1 } else { 0 },
                event.started_at_ms,
                event.ended_at_ms,
                now
            ],
        )?;
        Ok(())
    }

    pub fn load_export(&self, session_id: &str) -> rusqlite::Result<SessionExport> {
        let mut session_stmt = self.conn.prepare("SELECT id, title, started_at, ended_at FROM sessions WHERE id = ?1")?;
        let (id, title, started_at, ended_at): (String, String, String, Option<String>) = session_stmt
            .query_row(params![session_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?;

        let mut event_stmt = self.conn.prepare(
            "SELECT event_type, text, is_final, started_at_ms, ended_at_ms FROM events WHERE session_id = ?1 ORDER BY created_at ASC",
        )?;
        let events = event_stmt
            .query_map(params![session_id], |row| {
                Ok(SessionExportEvent {
                    event_type: row.get(0)?,
                    text: row.get(1)?,
                    is_final: row.get::<_, i64>(2)? == 1,
                    started_at_ms: row.get(3)?,
                    ended_at_ms: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(SessionExport { id, title, started_at, ended_at, events })
    }
}
```

- [ ] **Step 5: Export module from `lib.rs`**

Ensure `src-tauri/src/lib.rs` includes:

```rust
pub mod session;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 6: Run Rust test**

Run:

```powershell
cd src-tauri
cargo test --test session_db
cd ..
```

Expected:

```text
test creates_session_and_exports_events ... ok
```

- [ ] **Step 7: Commit session database**

Run:

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/session src-tauri/src/lib.rs src-tauri/tests/session_db.rs
git commit -m "feat: add local session database"
```

## Task 9: Rust Audio Device Contract And WASAPI Capture Skeleton

**Files:**
- Create: `src-tauri/src/audio/mod.rs`
- Create: `src-tauri/src/audio/frame.rs`
- Create: `src-tauri/src/audio/devices.rs`
- Create: `src-tauri/src/audio/capture.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/tests/audio_contract.rs`

- [ ] **Step 1: Add Windows audio dependencies**

Modify `src-tauri/Cargo.toml` dependencies:

```toml
windows = { version = "0.58", features = [
  "Win32_Media_Audio",
  "Win32_System_Com",
  "Win32_System_Threading",
  "Win32_UI_Shell_PropertiesSystem",
  "Win32_Devices_FunctionDiscovery",
] }
crossbeam-channel = "0.5"
```

- [ ] **Step 2: Write audio contract tests**

Create `src-tauri/tests/audio_contract.rs`:

```rust
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};

#[test]
fn computes_frame_duration_for_16khz_mono_pcm() {
    let frame = AudioFrame {
        format: PcmFormat { sample_rate: 16_000, channels: 1, bits_per_sample: 16 },
        samples: vec![0; 320],
        captured_at_ms: 100,
    };

    assert_eq!(frame.duration_ms(), 10);
}
```

- [ ] **Step 3: Run test to verify failure**

Run:

```powershell
cd src-tauri
cargo test --test audio_contract
cd ..
```

Expected:

```text
unresolved import `respondent_lib::audio`
```

- [ ] **Step 4: Implement audio frame contract**

Create `src-tauri/src/audio/mod.rs`:

```rust
pub mod capture;
pub mod devices;
pub mod frame;
```

Create `src-tauri/src/audio/frame.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct PcmFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
}

#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub format: PcmFormat,
    pub samples: Vec<i16>,
    pub captured_at_ms: u64,
}

impl AudioFrame {
    pub fn duration_ms(&self) -> u32 {
        if self.format.sample_rate == 0 || self.format.channels == 0 {
            return 0;
        }
        let sample_frames = self.samples.len() as u32 / self.format.channels as u32;
        sample_frames * 1000 / self.format.sample_rate
    }
}
```

- [ ] **Step 5: Implement device and capture interfaces**

Create `src-tauri/src/audio/devices.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_output_devices() -> Vec<OutputDevice> {
    vec![OutputDevice {
        id: "default-output".into(),
        name: "Default output device".into(),
        is_default: true,
    }]
}
```

Create `src-tauri/src/audio/capture.rs`:

```rust
use crossbeam_channel::{bounded, Receiver, Sender};

use super::frame::{AudioFrame, PcmFormat};

pub struct LoopbackCapture {
    sender: Sender<AudioFrame>,
    receiver: Receiver<AudioFrame>,
}

impl LoopbackCapture {
    pub fn new_for_device(_device_id: &str) -> Self {
        let (sender, receiver) = bounded(128);
        Self { sender, receiver }
    }

    pub fn receiver(&self) -> Receiver<AudioFrame> {
        self.receiver.clone()
    }

    pub fn push_test_frame(&self, captured_at_ms: u64) {
        let _ = self.sender.send(AudioFrame {
            format: PcmFormat { sample_rate: 16_000, channels: 1, bits_per_sample: 16 },
            samples: vec![0; 320],
            captured_at_ms,
        });
    }
}
```

This skeleton preserves the no-microphone contract while leaving the platform-specific WASAPI loopback internals behind `LoopbackCapture`.

- [ ] **Step 6: Export audio module from `lib.rs`**

Add:

```rust
pub mod audio;
```

- [ ] **Step 7: Run audio tests**

Run:

```powershell
cd src-tauri
cargo test --test audio_contract
cd ..
```

Expected:

```text
test computes_frame_duration_for_16khz_mono_pcm ... ok
```

- [ ] **Step 8: Commit audio contract**

Run:

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/audio src-tauri/src/lib.rs src-tauri/tests/audio_contract.rs
git commit -m "feat: add output audio capture contract"
```

## Task 10: ASR And LLM Provider Interfaces

**Files:**
- Create: `src-tauri/src/asr/mod.rs`
- Create: `src-tauri/src/asr/client.rs`
- Create: `src-tauri/src/asr/mock.rs`
- Create: `src-tauri/src/llm/mod.rs`
- Create: `src-tauri/src/llm/client.rs`
- Create: `src-tauri/src/llm/mock.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Implement ASR interface**

Create `src-tauri/src/asr/mod.rs`:

```rust
pub mod client;
pub mod mock;
```

Create `src-tauri/src/asr/client.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum AsrEvent {
    #[serde(rename = "transcript.partial")]
    Partial { session_id: String, text: String, started_at_ms: i64, ended_at_ms: i64, received_at_ms: i64 },
    #[serde(rename = "transcript.final")]
    Final { session_id: String, text: String, started_at_ms: i64, ended_at_ms: i64, received_at_ms: i64 },
    #[serde(rename = "endpoint.detected")]
    Endpoint { session_id: String, silence_ms: i64, detected_at_ms: i64 },
}

pub trait StreamingAsrClient: Send + Sync {
    fn name(&self) -> &'static str;
}
```

Create `src-tauri/src/asr/mock.rs`:

```rust
use super::client::StreamingAsrClient;

pub struct MockAsrClient;

impl StreamingAsrClient for MockAsrClient {
    fn name(&self) -> &'static str {
        "mock-asr"
    }
}
```

- [ ] **Step 2: Implement LLM interface**

Create `src-tauri/src/llm/mod.rs`:

```rust
pub mod client;
pub mod mock;
```

Create `src-tauri/src/llm/client.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ReplyRequest {
    pub session_id: String,
    pub generation_id: String,
    pub transcript: String,
    pub context: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ReplyEvent {
    #[serde(rename = "reply.started")]
    Started { session_id: String, generation_id: String, based_on_transcript_event_id: String, received_at_ms: i64 },
    #[serde(rename = "reply.token")]
    Token { session_id: String, generation_id: String, token: String, received_at_ms: i64 },
    #[serde(rename = "reply.final")]
    Final { session_id: String, generation_id: String, text: String, received_at_ms: i64 },
}

pub trait StreamingReplyClient: Send + Sync {
    fn name(&self) -> &'static str;
}
```

Create `src-tauri/src/llm/mock.rs`:

```rust
use super::client::StreamingReplyClient;

pub struct MockReplyClient;

impl StreamingReplyClient for MockReplyClient {
    fn name(&self) -> &'static str {
        "mock-llm"
    }
}
```

- [ ] **Step 3: Export provider modules**

Add to `src-tauri/src/lib.rs`:

```rust
pub mod asr;
pub mod llm;
```

- [ ] **Step 4: Run Rust checks**

Run:

```powershell
cd src-tauri
cargo check
cd ..
```

Expected:

```text
Finished `dev` profile
```

- [ ] **Step 5: Commit provider interfaces**

Run:

```powershell
git add src-tauri/src/asr src-tauri/src/llm src-tauri/src/lib.rs
git commit -m "feat: add streaming provider interfaces"
```

## Task 11: Tauri Commands And Frontend Adapter

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/services/tauriApi.ts`

- [ ] **Step 1: Add Tauri command module**

Create `src-tauri/src/commands.rs`:

```rust
use crate::audio::devices::{list_output_devices, OutputDevice};

#[tauri::command]
pub fn list_audio_output_devices() -> Vec<OutputDevice> {
    list_output_devices()
}

#[tauri::command]
pub fn start_session(title: String, output_device_id: String) -> Result<String, String> {
    if title.trim().is_empty() {
        return Err("Session title cannot be empty".into());
    }
    if output_device_id.trim().is_empty() {
        return Err("Output device id cannot be empty".into());
    }
    Ok(format!("session-{}", chrono::Utc::now().timestamp_millis()))
}

#[tauri::command]
pub fn end_session(session_id: String) -> Result<(), String> {
    if session_id.trim().is_empty() {
        return Err("Session id cannot be empty".into());
    }
    Ok(())
}
```

- [ ] **Step 2: Register commands**

Modify `src-tauri/src/lib.rs`:

```rust
pub mod asr;
pub mod audio;
pub mod commands;
pub mod llm;
pub mod session;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_audio_output_devices,
            commands::start_session,
            commands::end_session
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3: Add frontend Tauri API adapter**

Create `src/services/tauriApi.ts`:

```ts
import { invoke } from "@tauri-apps/api/core";

export type OutputDevice = {
  id: string;
  name: string;
  is_default: boolean;
};

export async function listAudioOutputDevices(): Promise<OutputDevice[]> {
  return invoke<OutputDevice[]>("list_audio_output_devices");
}

export async function startNativeSession(title: string, outputDeviceId: string): Promise<string> {
  return invoke<string>("start_session", { title, outputDeviceId });
}

export async function endNativeSession(sessionId: string): Promise<void> {
  await invoke("end_session", { sessionId });
}
```

- [ ] **Step 4: Run checks**

Run:

```powershell
npm run build
cd src-tauri
cargo check
cd ..
```

Expected:

```text
built in ...
Finished `dev` profile
```

- [ ] **Step 5: Commit command bridge**

Run:

```powershell
git add src/services/tauriApi.ts src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: add tauri command bridge"
```

## Task 12: Latency Telemetry

**Files:**
- Create: `src-tauri/src/telemetry/mod.rs`
- Create: `src-tauri/src/telemetry/latency.rs`
- Create: `src/domain/latency.ts`
- Create: `src/domain/latency.test.ts`

- [ ] **Step 1: Write frontend latency classification test**

Create `src/domain/latency.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { classifyLatency } from "./latency";

describe("classifyLatency", () => {
  it("marks reply TTFT under 1500 ms as target", () => {
    expect(classifyLatency("reply_ttft", 1200)).toBe("target");
  });

  it("marks reply TTFT above 1500 ms as slow", () => {
    expect(classifyLatency("reply_ttft", 1700)).toBe("slow");
  });
});
```

- [ ] **Step 2: Implement frontend latency classification**

Create `src/domain/latency.ts`:

```ts
export type LatencyMetric = "asr_partial" | "asr_final" | "endpoint" | "reply_ttft" | "reply_complete";
export type LatencyClass = "target" | "slow";

const thresholds: Record<LatencyMetric, number> = {
  asr_partial: 800,
  asr_final: 1800,
  endpoint: 500,
  reply_ttft: 1500,
  reply_complete: 3000,
};

export function classifyLatency(metric: LatencyMetric, valueMs: number): LatencyClass {
  return valueMs <= thresholds[metric] ? "target" : "slow";
}
```

- [ ] **Step 3: Implement Rust latency event type**

Create `src-tauri/src/telemetry/mod.rs`:

```rust
pub mod latency;
```

Create `src-tauri/src/telemetry/latency.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LatencyMark {
    pub session_id: String,
    pub metric: String,
    pub value_ms: i64,
    pub recorded_at_ms: i64,
}
```

Add to `src-tauri/src/lib.rs`:

```rust
pub mod telemetry;
```

- [ ] **Step 4: Run checks**

Run:

```powershell
npm test -- src/domain/latency.test.ts
cd src-tauri
cargo check
cd ..
```

Expected:

```text
PASS src/domain/latency.test.ts
Finished `dev` profile
```

- [ ] **Step 5: Commit latency telemetry**

Run:

```powershell
git add src/domain/latency.ts src/domain/latency.test.ts src-tauri/src/telemetry src-tauri/src/lib.rs
git commit -m "feat: add latency telemetry contracts"
```

## Task 13: Replace Mock Device Listing With Windows Output Enumeration

**Files:**
- Modify: `src-tauri/src/audio/devices.rs`
- Modify: `src-tauri/tests/audio_contract.rs`

- [ ] **Step 1: Add a non-hardware unit test for default device shape**

Extend `src-tauri/tests/audio_contract.rs`:

```rust
use respondent_lib::audio::devices::OutputDevice;

#[test]
fn output_device_serializes_expected_fields() {
    let device = OutputDevice {
        id: "device-1".into(),
        name: "Headphones".into(),
        is_default: true,
    };

    let json = serde_json::to_value(device).expect("serialize device");
    assert_eq!(json["id"], "device-1");
    assert_eq!(json["name"], "Headphones");
    assert_eq!(json["is_default"], true);
}
```

- [ ] **Step 2: Implement Windows-only enumeration behind the existing function**

Modify `src-tauri/src/audio/devices.rs`:

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[cfg(target_os = "windows")]
pub fn list_output_devices() -> Vec<OutputDevice> {
    list_output_devices_windows().unwrap_or_else(|_| {
        vec![OutputDevice {
            id: "default-output".into(),
            name: "Default output device".into(),
            is_default: true,
        }]
    })
}

#[cfg(not(target_os = "windows"))]
pub fn list_output_devices() -> Vec<OutputDevice> {
    vec![OutputDevice {
        id: "default-output".into(),
        name: "Default output device".into(),
        is_default: true,
    }]
}

#[cfg(target_os = "windows")]
fn list_output_devices_windows() -> windows::core::Result<Vec<OutputDevice>> {
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let default_device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let default_id = default_device.GetId()?.to_string()?;

        Ok(vec![OutputDevice {
            id: default_id,
            name: "Default output device".into(),
            is_default: true,
        }])
    }
}
```

This step intentionally returns the default endpoint first. Full friendly-name enumeration can be added after capture works, because MVP needs a reliable default output path before a richer settings screen.

- [ ] **Step 3: Run Rust tests**

Run:

```powershell
cd src-tauri
cargo test --test audio_contract
cargo check
cd ..
```

Expected:

```text
test output_device_serializes_expected_fields ... ok
Finished `dev` profile
```

- [ ] **Step 4: Commit Windows output enumeration**

Run:

```powershell
git add src-tauri/src/audio/devices.rs src-tauri/tests/audio_contract.rs
git commit -m "feat: enumerate windows output device"
```

## Task 14: End-To-End Manual Verification

**Files:**
- Create: `docs/verification/low-latency-mvp.md`

- [ ] **Step 1: Create verification checklist**

Create `docs/verification/low-latency-mvp.md`:

```markdown
# Low-Latency MVP Verification

Date: 2026-06-12

## Automated Checks

- `npm test`
- `npm run build`
- `cd src-tauri && cargo test`
- `cd src-tauri && cargo check`

## Mock UI Flow

- Start creates a new visible listening session.
- Partial subtitle appears before final subtitle.
- Suggested reply starts only after endpoint/final sequence.
- Copy button copies the current suggestion.
- End changes state to Saved.

## Native App Flow

- `npm run tauri:dev` opens a Windows desktop window.
- Window stays above normal app windows.
- Top bar can drag the window.
- Start does not ask for microphone permission.
- Output device command returns at least one output device.

## Latency Checks

- Mock ASR partial target: under 800 ms.
- Mock endpoint target: 300 ms silence window.
- Mock reply first token target: under 1500 ms after endpoint.

## Privacy Checks

- No microphone API is requested in frontend code.
- No Rust microphone capture module exists.
- Session export contains text events only.
- Audio files are not written by default.
```

- [ ] **Step 2: Run full automated checks**

Run:

```powershell
npm test
npm run build
cd src-tauri
cargo test
cargo check
cd ..
```

Expected:

```text
Test Files ... passed
built in ...
test result: ok
Finished `dev` profile
```

- [ ] **Step 3: Run desktop app**

Run:

```powershell
npm run tauri:dev
```

Expected:

```text
Finished dev
```

Manual verification:

- The app window opens.
- Start begins a mock session.
- UI remains responsive while mock events stream.
- No microphone permission prompt appears.

- [ ] **Step 4: Commit verification checklist**

Run:

```powershell
git add docs/verification/low-latency-mvp.md
git commit -m "test: add low latency mvp verification checklist"
```

## Execution Notes

After Task 14, the app has a tested mock real-time product loop and Rust contracts for persistence, provider adapters, telemetry, and output audio. The next implementation plan should replace the `LoopbackCapture` skeleton with real event-driven WASAPI loopback frames, then connect a real streaming ASR service and a real low-TTFT LLM service.

Do not add microphone capture while executing this plan. Any code path named microphone, input device capture, or recording device capture violates the MVP contract.

## Self-Review

Spec coverage:

- Windows-only: covered by Tauri/Rust scaffold and Windows output enumeration.
- No microphone capture: covered by event guard, verification checklist, and audio module scope.
- WASAPI loopback path: covered by audio contract and Windows output enumeration; full loopback frame capture is isolated for the next plan after skeleton verification.
- Streaming partial subtitles: covered by event contracts, transcript engine, mock pipeline, and UI.
- Endpoint-triggered replies: covered by endpoint policy and reply engine.
- Low TTFT strategy: covered by reply trigger policy and latency telemetry.
- Session save and export: covered by frontend export functions and Rust SQLite session database.
- Floating window: covered by Tauri UI and app-region drag controls.

Red-flag scan:

- No empty implementation markers or undefined task bodies remain.

Type consistency:

- Frontend event names match `RealtimeEvent`.
- Reply generation ids use `generationId` in TypeScript and `generation_id` in Rust serialization.
- Session event types use `transcript`, `suggestion`, and `system` in export/storage paths.
