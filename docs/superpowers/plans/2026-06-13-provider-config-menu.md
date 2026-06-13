# Provider Config Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an in-app provider configuration menu for both LLM and ASR, with Tauri-backed persistence and resolver precedence over environment variables.

**Architecture:** Add a focused Rust `provider_config` module that owns disk persistence, redacted summaries, defaults, and settings structs. Keep existing env resolvers intact, add settings-aware resolver entry points in `commands.rs`, and add a compact React settings panel wired through `tauriApi.ts`.

**Tech Stack:** Tauri 2, Rust, serde/serde_json, React 19, TypeScript, Vitest, Testing Library.

---

### Task 1: Backend Provider Config Storage

**Files:**
- Create: `src-tauri/src/provider_config.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/provider_config.rs`

- [ ] **Step 1: Write failing Rust tests**

Create `src-tauri/tests/provider_config.rs`:

```rust
use respondent_lib::provider_config::{
    load_provider_settings, save_provider_settings, AsrProviderSettings, LlmProviderSettings,
    ProviderSettings,
};

fn settings_path() -> std::path::PathBuf {
    let unique = format!(
        "respondent-provider-config-test-{}.json",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}

#[test]
fn summary_redacts_api_keys() {
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "siliconflow".into(),
            api_key: Some("secret-llm".into()),
            base_url: Some("https://api.siliconflow.cn/v1".into()),
            model: Some("Qwen/Qwen3-8B".into()),
        }),
        asr: Some(AsrProviderSettings {
            provider: "bailian_realtime".into(),
            api_key: Some("secret-asr".into()),
            base_url: None,
            model: Some("fun-asr-realtime".into()),
            language_hint: Some("zh".into()),
            max_sentence_silence_ms: Some(800),
            heartbeat: Some(true),
        }),
    };

    let summary = settings.summary();
    let serialized = serde_json::to_string(&summary).unwrap();

    assert!(summary.llm.unwrap().has_api_key);
    assert!(summary.asr.unwrap().has_api_key);
    assert!(!serialized.contains("secret-llm"));
    assert!(!serialized.contains("secret-asr"));
}

#[test]
fn saves_and_loads_provider_settings() {
    let path = settings_path();
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "openai".into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            model: Some("gpt-5.4-mini".into()),
        }),
        asr: None,
    };

    save_provider_settings(&path, &settings).unwrap();
    let loaded = load_provider_settings(&path).unwrap();

    assert_eq!(loaded.llm.unwrap().api_key.as_deref(), Some("sk-test"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn missing_settings_file_loads_empty_settings() {
    let path = settings_path();
    let loaded = load_provider_settings(&path).unwrap();

    assert!(loaded.llm.is_none());
    assert!(loaded.asr.is_none());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test provider_config`

Expected: compile failure because `respondent_lib::provider_config` does not exist.

- [ ] **Step 3: Implement provider config module**

Create `src-tauri/src/provider_config.rs` with serializable settings structs, redacted summary structs, `load_provider_settings`, `save_provider_settings`, and `settings_file_path(app)`.

- [ ] **Step 4: Export module**

Add `pub mod provider_config;` to `src-tauri/src/lib.rs`.

- [ ] **Step 5: Run tests to verify pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test provider_config`

Expected: all provider config tests pass.

### Task 2: Settings-Aware Resolvers

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Test: `src-tauri/tests/commands.rs`

- [ ] **Step 1: Write failing resolver tests**

Append tests in `src-tauri/tests/commands.rs` that import provider settings and assert:

```rust
#[test]
fn llm_manual_settings_override_env() {
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "siliconflow".into(),
            api_key: Some("manual-key".into()),
            base_url: None,
            model: None,
        }),
        asr: None,
    };

    assert_eq!(
        resolve_reply_provider_name_with_settings(
            &env(&[("LLM_PROVIDER", "openai"), ("OPENAI_API_KEY", "env-key")]),
            &settings,
        ),
        "openai-compatible-llm"
    );
}

#[test]
fn llm_incomplete_manual_settings_fall_back_to_env() {
    let settings = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "siliconflow".into(),
            api_key: None,
            base_url: None,
            model: None,
        }),
        asr: None,
    };

    assert_eq!(
        resolve_reply_provider_name_with_settings(
            &env(&[("LLM_PROVIDER", "openai"), ("OPENAI_API_KEY", "env-key")]),
            &settings,
        ),
        "openai-responses-llm"
    );
}

#[test]
fn asr_manual_settings_override_env() {
    let settings = ProviderSettings {
        llm: None,
        asr: Some(AsrProviderSettings {
            provider: "bailian_realtime".into(),
            api_key: Some("manual-key".into()),
            base_url: None,
            model: None,
            language_hint: None,
            max_sentence_silence_ms: None,
            heartbeat: None,
        }),
    };

    assert_eq!(
        resolve_asr_provider_name_with_settings(
            "s1",
            &env(&[("ASR_PROVIDER", "openai_realtime"), ("OPENAI_API_KEY", "env-key")]),
            &settings,
        ),
        "bailian-realtime-asr"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test commands provider`

Expected: compile failure for missing `*_with_settings` resolver helpers.

- [ ] **Step 3: Implement resolver helpers**

Add helper functions that convert complete `LlmProviderSettings` and `AsrProviderSettings` into env-like maps using existing provider-specific environment keys, then call the existing resolvers. Incomplete manual settings return the existing env resolver result.

- [ ] **Step 4: Run targeted tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test commands provider`

Expected: provider selection tests pass.

### Task 3: Tauri Commands

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/tests/commands.rs`

- [ ] **Step 1: Write failing command helper tests**

Add tests for pure helper functions that merge updates:

```rust
#[test]
fn provider_config_update_without_api_key_preserves_existing_key() {
    let existing = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "openai".into(),
            api_key: Some("old-key".into()),
            base_url: None,
            model: Some("old-model".into()),
        }),
        asr: None,
    };
    let update = ProviderSettings {
        llm: Some(LlmProviderSettings {
            provider: "openai".into(),
            api_key: None,
            base_url: None,
            model: Some("new-model".into()),
        }),
        asr: None,
    };

    let merged = merge_provider_settings(existing, update);

    assert_eq!(
        merged.llm.unwrap().api_key.as_deref(),
        Some("old-key")
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test commands provider_config_update`

Expected: compile failure for missing merge helper.

- [ ] **Step 3: Implement commands and merge helper**

Add `get_provider_config`, `save_provider_config`, `clear_provider_config`, `merge_provider_settings`, and a `ProviderConfigStore` managed state that stores the JSON path. Register commands in `lib.rs`.

- [ ] **Step 4: Run targeted tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test commands provider_config`

Expected: command helper tests pass.

### Task 4: Frontend API and UI Tests

**Files:**
- Modify: `src/services/tauriApi.ts`
- Modify: `src/App.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

- [ ] **Step 1: Write failing frontend tests**

Mock `@tauri-apps/api/core` in `src/App.test.tsx`, render `App`, click `Configure providers`, change LLM provider, save, and assert `save_provider_config` was invoked with both `llm` and `asr` payloads.

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm test -- App.test.tsx`

Expected: failure because no configuration button exists.

- [ ] **Step 3: Implement Tauri API wrappers**

Add types and wrappers in `src/services/tauriApi.ts`: `getProviderConfig`, `saveProviderConfig`, `clearProviderConfig`.

- [ ] **Step 4: Implement compact settings panel**

Add a settings button to `App.tsx`, keep fields in local state, load config only in Tauri runtime, and support mock/local fallback in browser tests. Use password inputs for API keys and never hydrate key values from summaries.

- [ ] **Step 5: Style panel**

Add CSS for a compact overlay/panel that fits within the existing small desktop shell without nested cards.

- [ ] **Step 6: Run frontend tests**

Run: `npm test -- App.test.tsx`

Expected: App tests pass.

### Task 5: Session Runtime Wiring and Full Verification

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Test: existing Rust and frontend suites

- [ ] **Step 1: Wire SessionRuntime to provider config**

Read saved provider settings from app data before building ASR and LLM clients. Pass settings to the settings-aware resolver helpers.

- [ ] **Step 2: Run full verification**

Run:

```bash
npm test
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: both commands exit 0.

- [ ] **Step 3: Review git diff**

Run: `git diff --stat` and inspect touched files for accidental API Key logging, unrelated changes, or UI text overflow risks.

- [ ] **Step 4: Commit implementation**

Run:

```bash
git add src src-tauri docs/superpowers/plans/2026-06-13-provider-config-menu.md
git commit -m "feat: add provider configuration menu"
```
