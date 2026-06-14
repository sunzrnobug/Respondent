# Production Readiness Verification

Date: 2026-06-15

This checklist complements `low-latency-mvp.md`. MVP mock/demo flows can pass without these steps; release or enterprise acceptance should not.

## Automated Baseline

- `npm test`
- `npm run build`
- `cd src-tauri && cargo test`
- `cd src-tauri && cargo check`

## Security Baseline

- `src-tauri/tauri.conf.json` sets a restrictive CSP (not `null`). `connect-src` is limited to Tauri IPC (`'self' ipc: http://ipc.localhost`) because provider traffic runs in Rust, not the webview.
- Provider API keys are stored in the OS credential vault via `keyring` with platform features enabled (`windows-native`, `apple-native`, `sync-secret-service` in `Cargo.toml`). JSON profile files keep metadata only.
- Plaintext-key migration is blocked unless `verify_keyring_backend()` succeeds (probe round-trip against the OS store).
- Release builds reject implicit mock ASR/LLM fallback unless `RESPONDENT_ALLOW_MOCK=1` is explicitly set.
- Tauri capabilities drop `core:default` / `opener:default` in favour of explicit path/event/app/tray/window/webview/opener/global-shortcut permissions.

### OS credential store smoke (required before release sign-off)

```powershell
cd src-tauri
# Do NOT set RESPONDENT_SECRET_BACKEND=memory
cargo test --test secret_store_keyring -- --ignored --nocapture
```

Record: test passes on the target OS (Windows Credential Manager / macOS Keychain / Linux Secret Service).

## Provider Configuration

- Configure a real ASR and LLM profile in the desktop UI or via environment variables.
- Start a session and confirm `system.status` does **not** warn about demo/mock providers.
- With `RESPONDENT_ALLOW_MOCK=0`, starting a session without valid keys must fail with a clear configuration error.

## Real Network Acceptance

Run only when API keys and billable usage are approved:

```powershell
cd src-tauri
$env:DASHSCOPE_API_KEY = "<your-key>"
cargo test --test e2e_real_network real_dashscope_llm_smoke_when_api_key_is_present -- --ignored --nocapture
cargo test --test e2e_real_network real_bailian_asr_and_llm_smoke_when_api_key_is_present -- --ignored --nocapture
```

SiliconFlow / OpenAI variants (optional):

```powershell
$env:SILICONFLOW_API_KEY = "<your-key>"
cargo test --test e2e_real_network -- --ignored --nocapture
```

```powershell
$env:OPENAI_API_KEY = "<your-key>"
cargo test --test e2e_real_network real_openai_asr_and_llm_smoke_when_api_key_is_present -- --ignored --nocapture
```

Record for sign-off (paste into this file under **Acceptance Record**):

- Provider names resolved (not `mock-asr` / `mock-llm`)
- `[acceptance] bailian first partial transcript: …ms [PASS|SLOW]` (requires real speech audio; synthetic sine-wave fixtures may yield `NO_VALID_AUDIO`)
- `[acceptance] dashscope first reply token: …ms [PASS|SLOW]`
- No transport/auth errors in console output (provider-side audio validation errors are acceptable on synthetic fixtures)

## Acceptance Record

| Date | Operator | Provider (ASR/LLM) | Partial ms | Reply token ms | Audio smoke | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-06-15 | Cursor agent (JackieLoveUnique machine) | `bailian-realtime-asr` / DashScope `qwen-plus` (compatible-mode) | N/A (synthetic audio → `NO_VALID_AUDIO`; no partial) | 428 (LLM-only) / 447 (ASR→LLM fallback) | PASS (`loopback_capture_smoke`, non-silent 16 kHz frames) | `secret_store_keyring` PASS; auth/transport OK; no mock providers; ASR partial latency still needs real speech audio run |

## Real Audio Acceptance (Windows)

Requires audible system output (meeting/video playback):

```powershell
cd src-tauri
cargo test --test loopback_capture_smoke -- --ignored --nocapture
```

Record:

- Non-silent 16 kHz mono frames received within 5 s
- Selected output device matches the active playback device

## Data Governance

- **Windows build prerequisite**: SQLCipher requires [OpenSSL Dev](https://slproweb.com/) (`winget install ShiningLight.OpenSSL.Dev`). `src-tauri/.cargo/config.toml` points `OPENSSL_DIR` / `OPENSSL_LIB_DIR` at the default install path; `build.rs` adds the MSVC link search path.
- SQLite (`respondent.sqlite3`) is encrypted with SQLCipher (`bundled-sqlcipher`). The DB master key is stored in the OS credential vault (`respondent/db-master-key` via `secret_store`).
- Existing plaintext databases are migrated once via `ATTACH` + `sqlcipher_export`; failure is fail-closed and leaves the original plaintext file in place (no `.plaintext.bak` is written until export verification succeeds).
- On successful migration, any transient `.sqlite3.plaintext.bak` is zero-overwritten and deleted immediately after the encrypted database is verified. Upgrades from older builds that left a stale backup also purge it on the next successful encrypted open.
- Saved long-session history is stored in encrypted SQLite (`saved_sessions` table), not `localStorage`. Legacy `respondent.savedSessions` entries are imported on first launch and then removed from the browser store.
- Runtime session transcripts/suggestions remain in the same encrypted database (`sessions` / `events` tables).
- Both runtime and saved-session records apply a 90-day retention policy on startup.
- Exports include an explicit sensitivity notice in markdown output.
- `delete_session_record` / `delete_saved_session` perform physical `DELETE` (no secure wipe).

## Runtime Observability

- Bridge emit failures log to stderr: `[respondent] emit ... failed`
- Persist failures log to stderr and emit `system.status` warnings to the UI
- Document store lock failures return structured errors instead of panicking

## Sign-off Guidance

| Area | MVP demo | Production gate |
| --- | --- | --- |
| Mock providers | Allowed in debug / with `RESPONDENT_ALLOW_MOCK=1` | Disallowed in release |
| API key storage | OS credential vault + `secret_store_keyring` smoke | Required |
| CSP | Restrictive policy configured | Required |
| Real network tests | Manual, `--ignored` | Executed and recorded |
| Real audio capture | Manual, `--ignored` | Executed and recorded |
| Data retention | 90-day purge on startup/load | Documented and verified |
