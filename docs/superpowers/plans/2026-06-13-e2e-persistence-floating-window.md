# E2E Persistence Floating Window Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the app closer to a usable MVP by adding a real-network E2E smoke harness, saving realtime transcripts/replies to SQLite, exposing export commands, and configuring the Tauri window as a compact always-on-top floating window.

**Architecture:** Keep realtime orchestration in `commands.rs`, but add a persistence bridge beside the existing ASR/reply emit bridges so UI events and stored events remain driven by the same backend streams. E2E validation is an ignored/gated Rust test that uses real OpenAI providers only when `OPENAI_API_KEY` is present.

**Tech Stack:** Rust, Tauri commands, SQLite via existing `SessionDb`, OpenAI Realtime ASR, OpenAI Responses LLM, Tauri v2 window config.

---

### Task 1: Real Network E2E Smoke Harness

**Files:**
- Create: `src-tauri/tests/e2e_real_network.rs`

- [ ] **Step 1: Write the gated smoke test**

Create a test named `real_openai_asr_and_llm_smoke_when_api_key_is_present`. The test must:

```rust
if std::env::var("OPENAI_API_KEY").ok().filter(|value| !value.trim().is_empty()).is_none() {
    eprintln!("skipping real OpenAI E2E smoke: OPENAI_API_KEY is not set");
    return;
}
```

Then it starts `OpenAiRealtimeAsrClient::connect`, starts `OpenAiReplyClient::from_env`, feeds deterministic non-silent 16 kHz frames, finalizes ASR, and waits for either a transcript final plus reply final or a provider error. Mark it `#[ignore]` so normal CI never spends real API/network cost.

- [ ] **Step 2: Run the smoke test without a key**

Run: `$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path; cargo test --test e2e_real_network -- --ignored --nocapture`

Expected: the test exits successfully with a skip message when `OPENAI_API_KEY` is absent.

- [ ] **Step 3: Commit**

Run:

```powershell
git add src-tauri/tests/e2e_real_network.rs
git commit -m "test: add real network e2e smoke harness"
```

### Task 2: Persist Realtime Sessions And Export

**Files:**
- Modify: `src-tauri/src/session/db.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/session_db.rs`
- Modify: `src-tauri/tests/commands.rs`

- [ ] **Step 1: Write failing persistence tests**

Add tests that prove:

```rust
let db = SessionDb::open_in_memory().expect("open db");
db.start_session_with_id("session-1", "Meeting", "default-output").expect("start");
db.insert_event(EventInsert { session_id: "session-1".into(), event_type: "transcript".into(), text: "hello".into(), is_final: true, started_at_ms: 0, ended_at_ms: 300 }).expect("insert");
let export = db.load_export("session-1").expect("load");
assert_eq!(export.id, "session-1");
assert_eq!(export.events[0].event_type, "transcript");
```

And command-level tests that call pure formatting helpers:

```rust
assert!(export_session_markdown_for_test(&export).contains("## Meeting"));
assert!(export_session_text_for_test(&export).contains("Transcript: hello"));
```

- [ ] **Step 2: Run tests to verify RED**

Run: `$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path; cargo test --test session_db start_session_with_supplied_id_persists_events`

Expected: compile failure until `start_session_with_id` exists.

- [ ] **Step 3: Implement database and command wiring**

Implement `SessionDb::start_session_with_id`. Add `PersistentSessionDb` as Tauri managed state using `app.path().app_data_dir()/respondent.sqlite3`. In `start_session`, insert the same native `session_id` that runtime uses. In ASR/reply bridges, write `transcript.final` as `event_type = "transcript"` and `reply.final` as `event_type = "suggestion"`. In `end_session`, update `ended_at`. Add Tauri commands `export_session_markdown(session_id)` and `export_session_text(session_id)`.

- [ ] **Step 4: Run persistence tests**

Run:

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
cargo test --test session_db
cargo test --test commands
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

Run:

```powershell
git add src-tauri/src/session/db.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/tests/session_db.rs src-tauri/tests/commands.rs
git commit -m "feat: persist realtime sessions and export transcripts"
```

### Task 3: Floating Window Configuration

**Files:**
- Modify: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Write config expectation**

Inspect `src-tauri/tauri.conf.json` and require the main window to use:

```json
"width": 420,
"height": 520,
"alwaysOnTop": true,
"decorations": false,
"resizable": false
```

- [ ] **Step 2: Apply configuration**

Edit only the main window config object.

- [ ] **Step 3: Verify config parses**

Run: `npm run build`

Expected: frontend production build passes. `cargo check` later validates Tauri config at compile time.

- [ ] **Step 4: Commit**

Run:

```powershell
git add src-tauri/tauri.conf.json
git commit -m "feat: configure compact floating window"
```

### Task 4: Final Verification And Push

**Files:**
- No source changes expected unless verification exposes a defect.

- [ ] **Step 1: Run full verification**

Run:

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
cargo test
cargo check
npm test
npm run build
```

Expected: all commands pass.

- [ ] **Step 2: Push branch**

Run: `git push`

Expected: PR #2 is updated on branch `feat/streaming-llm`.
