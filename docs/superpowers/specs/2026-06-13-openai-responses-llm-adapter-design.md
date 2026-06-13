# OpenAI Responses LLM Adapter Design

Date: 2026-06-13

## Scope

Add the first real reply provider by implementing `StreamingReplyClient` with OpenAI Responses streaming. This replaces `MockReplyClient` in native runtime whenever `OPENAI_API_KEY` is present.

## Design

- Create `src-tauri/src/llm/openai_responses.rs`.
- Use OpenAI Responses API with `stream: true`.
- Default model is `gpt-5.4-mini` for lower latency; override with `OPENAI_LLM_MODEL`.
- Parse SSE `data:` JSON events:
  - `response.output_text.delta` -> `ReplyEvent::Token`
  - `response.completed` -> `ReplyEvent::Final`
  - `response.error` / `error` -> final error text and stop
- Preserve existing `ReplySession` and `ReplyPoll` contract. The adapter runs its network stream on a worker thread and `poll()` stays non-blocking.
- Unit tests use a fake streaming transport; no network or API key required.

## Prompt

The prompt asks for a concise reply suggestion suitable for a live meeting. It includes:

- current transcript turn
- rolling context provided by `ReplyRequest.context`

No raw API key is logged or emitted.

## Acceptance

- `OpenAiReplyClient` implements `StreamingReplyClient`.
- Commands runtime uses OpenAI LLM when `OPENAI_API_KEY` is set; otherwise mock.
- `cargo test`, `cargo check`, and `npm test` pass.
