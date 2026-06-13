# OpenAI Responses LLM Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace mock reply suggestions with a real low-latency OpenAI Responses streaming LLM adapter when `OPENAI_API_KEY` is configured.

**Architecture:** Add a pull-based `OpenAiReplyClient` that preserves the existing `StreamingReplyClient` contract by running the network SSE stream on a worker thread and exposing non-blocking `poll()` events. Keep `MockReplyClient` as the fallback when no API key is available.

**Tech Stack:** Rust, Tauri, OpenAI Responses API streaming, `reqwest` blocking client, `crossbeam-channel`, `serde_json`.

---

### Task 1: OpenAI Responses Adapter

**Files:**
- Create: `src-tauri/src/llm/openai_responses.rs`
- Modify: `src-tauri/src/llm/mod.rs`
- Modify: `src-tauri/Cargo.toml`
- Test: `src-tauri/tests/openai_responses_llm.rs`

- [ ] **Step 1: Write failing adapter tests**

Create tests that prove:

```rust
use respondent_lib::llm::client::{ReplyEvent, ReplyPoll, ReplyRequest, StreamingReplyClient};
use respondent_lib::llm::openai_responses::{
    build_responses_body, OpenAiReplyClient, OpenAiReplyConfig, ResponsesEventStream,
    ResponsesTransport,
};
use respondent_lib::llm::client::LlmError;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn responses_body_includes_stream_model_context_and_current_turn() {
    let body = build_responses_body(
        &OpenAiReplyConfig::from_api_key("test-key"),
        &ReplyRequest {
            session_id: "s1".into(),
            generation_id: "gen-1".into(),
            transcript: "What should we do next?".into(),
            context: vec!["Earlier context".into(), "What should we do next?".into()],
        },
    );

    assert_eq!(body["model"], "gpt-5.4-mini");
    assert_eq!(body["stream"], true);
    let input = body["input"].as_array().expect("input messages");
    assert!(input[0]["content"].as_str().unwrap().contains("live meeting"));
    assert!(input[1]["content"].as_str().unwrap().contains("Earlier context"));
    assert!(input[1]["content"].as_str().unwrap().contains("What should we do next?"));
}

#[test]
fn openai_reply_streams_started_tokens_and_final_from_response_events() {
    let client = OpenAiReplyClient::with_transport(
        OpenAiReplyConfig::from_api_key("test-key"),
        Arc::new(FakeTransport::new(vec![
            json!({"type": "response.output_text.delta", "delta": "I would "}),
            json!({"type": "response.output_text.delta", "delta": "ask for timing."}),
            json!({"type": "response.completed"}),
        ])),
    )
    .expect("client");

    let events = collect_events(client.start(request()));

    assert!(matches!(events.first(), Some(ReplyEvent::Started { generation_id, .. }) if generation_id == "gen-1"));
    let tokens: Vec<&str> = events.iter().filter_map(token_text).collect();
    assert_eq!(tokens, ["I would ", "ask for timing."]);
    assert!(matches!(events.last(), Some(ReplyEvent::Final { text, .. }) if text == "I would ask for timing."));
}

#[test]
fn openai_reply_reports_provider_error_without_leaking_api_key() {
    let client = OpenAiReplyClient::with_transport(
        OpenAiReplyConfig::from_api_key("secret-key"),
        Arc::new(FakeTransport::new(vec![
            json!({"type": "response.error", "error": {"message": "secret-key is invalid"}}),
        ])),
    )
    .expect("client");

    let events = collect_events(client.start(request()));
    let final_text = events.iter().find_map(|event| match event {
        ReplyEvent::Final { text, .. } => Some(text.as_str()),
        _ => None,
    }).expect("final error event");

    assert!(final_text.contains("Reply generation failed"));
    assert!(!final_text.contains("secret-key"));
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path; cargo test --test openai_responses_llm`

Expected: compile failure because `llm::openai_responses` does not exist.

- [ ] **Step 3: Implement minimal adapter**

Create `OpenAiReplyConfig`, `ResponsesTransport`, `ResponsesEventStream`, `OpenAiReplyClient`, and `ReqwestResponsesTransport`. The worker sends `reply.started`, maps `response.output_text.delta` to `reply.token`, accumulates tokens, and sends `reply.final` on `response.completed` or a generic non-secret failure final on provider errors.

- [ ] **Step 4: Run adapter tests to verify GREEN**

Run: `$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path; cargo test --test openai_responses_llm`

Expected: all adapter tests pass.

- [ ] **Step 5: Commit**

Run:

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/llm/mod.rs src-tauri/src/llm/openai_responses.rs src-tauri/tests/openai_responses_llm.rs
git commit -m "feat: add openai responses reply adapter"
```

### Task 2: Runtime Wiring

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/tests/provider_contract.rs`

- [ ] **Step 1: Write failing provider contract test**

Extend `mock_clients_report_their_names` to construct `OpenAiReplyClient::with_transport(...)` and assert:

```rust
assert_eq!(openai_reply.name(), "openai-responses-llm");
```

- [ ] **Step 2: Run test to verify RED**

Run: `$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path; cargo test --test provider_contract mock_clients_report_their_names`

Expected: compile failure until the OpenAI reply client is exported and imported.

- [ ] **Step 3: Wire runtime provider selection**

Add `build_reply_client() -> Result<(Box<dyn StreamingReplyClient>, bool), String>` in `commands.rs`. Use OpenAI when `OPENAI_API_KEY` is present and non-empty; otherwise use `MockReplyClient`. Emit a status event when the reply provider falls back to mock.

- [ ] **Step 4: Run contract tests**

Run: `$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path; cargo test --test provider_contract`

Expected: all provider contract tests pass.

- [ ] **Step 5: Commit**

Run:

```powershell
git add src-tauri/src/commands.rs src-tauri/tests/provider_contract.rs
git commit -m "feat: use real llm provider in sessions"
```

### Task 3: Full Verification And PR Update

**Files:**
- No source changes expected unless verification exposes defects.

- [ ] **Step 1: Run Rust verification**

Run:

```powershell
$env:Path = 'C:\Users\Administrator\.cargo\bin;' + $env:Path
cargo test
cargo check
```

Expected: Rust tests and check pass.

- [ ] **Step 2: Run frontend verification**

Run: `npm test`

Expected: frontend tests pass.

- [ ] **Step 3: Push branch**

Run:

```powershell
git status --short
git push
```

Expected: branch `feat/streaming-llm` updates existing draft PR #2.
