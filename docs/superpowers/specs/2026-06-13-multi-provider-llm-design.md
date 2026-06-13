# 多 Provider LLM(OpenAI 兼容 Chat Completions)设计

日期：2026-06-13

## 背景与范围

当前 LLM 回复建议只有 OpenAI Responses 适配器(`src-tauri/src/llm/openai_responses.rs`):reqwest blocking + SSE,后台线程 + crossbeam channel + `poll()/Pending` 拉模型,通过可 mock 的 `ResponsesTransport`/`ResponsesEventStream`(产出 `serde_json::Value`)做确定性测试。`commands.rs` 用 `build_reply_client_from_api_key(OPENAI_API_KEY)` 选真实/mock。

本轮新增对**阿里百炼、智谱、硅基流动及任意 OpenAI 兼容 `/chat/completions`** 服务的支持,通过一个通用 Chat Completions 适配器实现;并通过 `LLM_PROVIDER` + per-provider 配置选择。**ASR 维持 OpenAI Realtime 不变**(各家实时音频 ASR 形态不统一,另拆一轮)。

**范围内**:
- 抽出共享流式引擎(SSE 行读取 + worker 线程/channel/poll/取消 + ReplyEvent 拼装),供两种 dialect 复用。
- `openai_responses` 重构为复用共享引擎,**公开 API 与现有测试不变**。
- 新增 `OpenAiCompatibleReplyClient`(Chat Completions dialect),`ProviderConfig { base_url, api_key, model }`,可 mock 测试。
- `commands.rs` 新增 env resolver:`LLM_PROVIDER` + per-provider key/base_url/model → 选 client(保留 mock 回退)。
- 单测:Chat Completions SSE 容错矩阵;provider 选择。

**范围外**:
- 多家 ASR(维持 OpenAI Realtime)。
- 桌面端"设置面板填 key/base_url/model"的 UI(本轮仍 env 驱动,但 config 设计为结构体以便后续 UI 接入)。
- LLM key 的安全持久化存储。

## 设计原则

**DRY 共享引擎,不复制易错的线程/poll/取消逻辑。** Responses 与 Chat Completions 真正不同的只有四点:URL、请求体、SSE 增量字段、结束信号。把其余(reqwest blocking、Bearer、`data:` 行读取、`[DONE]`/EOF、worker 线程 + unbounded channel + `poll()`/Pending、错误→failure-final、final 拼装)抽成共享引擎,按 dialect 只参数化这四点。

**config 是结构体,env 解析是薄层。** 适配器吃显式 `ProviderConfig`;`LLM_PROVIDER`/env 矩阵解析集中在 commands.rs 的一个 resolver,不进适配器——便于将来设置面板供给 config 而不依赖 env。

## 共享流式引擎(新增 `src-tauri/src/llm/streaming.rs`)

```rust
/// 一次 SSE 事件流,产出已解析的 JSON 值(每个 `data:` 行一个)。
pub trait SseValueStream: Send {
    /// 下一个事件;`[DONE]` 或流结束返回 Ok(None)。
    fn next_value(&mut self) -> Result<Option<serde_json::Value>, LlmError>;
}

/// dialect 把一个 SSE JSON 值映射为引擎动作。
pub enum ReplyChunk {
    Token(String), // 追加一个增量 token
    Complete,      // 该次生成正常完成
    Error,         // provider 报错
    Ignore,        // 无关事件(role-only、usage、keep-alive 等)
}

/// 共享 reqwest blocking SSE 读取:strip `data:`,`[DONE]`/EOF→None,
/// 跳过非 `data:`(注释/空行),解析 JSON。两 dialect 复用。
pub struct ReqwestSseStream { /* BufReader<reqwest::blocking::Response> */ }
impl SseValueStream for ReqwestSseStream { ... }

/// 共享 worker:起线程,先发 Started;循环 next_value→map(&Value)→
/// Token 追加/转发、Complete→Final、Error/EOF→按 final_text 决定 Final 或
/// failure-final;unbounded channel + poll()/Pending(沿用现状)。返回 Box<dyn ReplyGeneration>。
pub fn spawn_streaming_reply<M>(
    request: ReplyRequest,
    open: impl FnOnce() -> Result<Box<dyn SseValueStream>, LlmError> + Send + 'static,
    map: M,
) -> Box<dyn ReplyGeneration>
where M: Fn(&serde_json::Value) -> ReplyChunk + Send + 'static;
```

- `open` 是延迟到 worker 线程内执行的"建连+发请求"闭包(失败→failure-final + Done),保持现状的错误处理与"线程内才发网络请求"语义。
- 取消:沿用现状(drop generation → channel 断开)。**改进**:worker 每次 `next_value` 前检查"输出 channel 是否已断开"(`sender` 端 disconnected),若是则提前 return 并 drop 流,**主动中断上游 HTTP**,避免被 latest-wins 取代的生成继续烧 token。
- `GENERIC_FAILURE_TEXT`、`now_ms`、`truncate_for_error` 移到此共享模块。

## Responses 适配器重构(`openai_responses.rs`)

- 保留全部公开 API:`OpenAiReplyClient`、`OpenAiReplyConfig`、`with_transport`、`from_api_key`、`from_env`、`build_responses_body`、`ResponsesTransport`、`ResponsesEventStream`。**现有 `tests/openai_responses_llm.rs` 不改且全绿。**
- 内部:`OpenAiReplyGeneration` 改为调用 `spawn_streaming_reply`,传入:`open` 闭包(调用 `ResponsesTransport::stream` 得到 `ResponsesEventStream`,适配为 `SseValueStream`),与 Responses 的 `map`:
  - `type=="response.output_text.delta"` → `Token(delta)`
  - `type=="response.completed"` → `Complete`
  - `type` 为 `response.error`/`error` → `Error`
  - 其余 → `Ignore`
- `ResponsesEventStream`(产出 Value)适配为 `SseValueStream`(`next_value` 委托 `next_event`),保持测试 seam。

## Chat Completions 适配器(新增 `src-tauri/src/llm/openai_compatible.rs`)

```rust
pub struct ProviderConfig { pub base_url: String, pub api_key: String, pub model: String }
pub struct OpenAiCompatibleReplyClient { config: ProviderConfig, transport: Arc<dyn ChatTransport> }
```

- `ChatTransport`(可 mock,镜像 ResponsesTransport)：`stream(&config, &request) -> Result<Box<dyn SseValueStream>, LlmError>`。真实实现 `ReqwestChatTransport`:`POST {base_url}/chat/completions`(base_url 去尾斜杠后拼接),`bearer_auth(api_key)`,body = `build_chat_body`,返回 `ReqwestSseStream`(共享)。
- `build_chat_body(config, request)`(公开,供测试):
  ```json
  {"model": <model>, "stream": true, "messages": [
    {"role":"system","content": <同 Responses 的 meeting-assistant 指令>},
    {"role":"user","content": "Conversation context:\n<context>\n\nCurrent turn:\n<transcript>\n\nWrite the suggested reply only."}
  ]}
  ```
  复用 `format_context`。
- Chat dialect `map(&Value) -> ReplyChunk`(**SSE 容错矩阵**):
  - `choices[0].delta.content` 为非空字符串 → `Token(content)`
  - `choices[0].delta.content` 缺失/为 null(role-only chunk、finish chunk)→ `Ignore`
  - `choices[0].delta.reasoning_content`(推理模型)→ `Ignore`(不污染建议)
  - `choices` 为空数组(末尾 usage 统计 chunk)→ `Ignore`
  - 顶层 `error` 字段 → `Error`
  - 其余 → `Ignore`
  - 结束:由 `ReqwestSseStream` 在 `data: [DONE]` 或流 EOF 时返回 None(无需在 map 里判 finish_reason);worker 收到 None 即用累计 final_text 收尾。
- `start()` 调用 `spawn_streaming_reply(request, open=|| transport.stream(...), chat_map)`。`name()` → `"openai-compatible-llm"`。
- `with_transport(config, Arc<dyn ChatTransport>)` 供确定性测试;构造时校验 `api_key`/`base_url` 非空,否则 `LlmError::Provider`。

## commands.rs:env resolver(provider 选择)

新增 `build_reply_client_from_env() -> Result<(Box<dyn StreamingReplyClient>, bool /*using_mock*/), String>`(替换现 `build_reply_client`),逻辑:

- 读 `LLM_PROVIDER`(缺省 = `openai`),小写匹配:
  - `openai` → 若 `OPENAI_API_KEY` 非空 → `OpenAiReplyClient`(Responses,现状);否则 mock。
  - `dashscope` → `ProviderConfig{ base_url: DASHSCOPE_BASE_URL || "https://dashscope.aliyuncs.com/compatible-mode/v1", api_key: DASHSCOPE_API_KEY, model: DASHSCOPE_LLM_MODEL || "qwen-plus" }`。
  - `zhipu` → `base_url: ZHIPU_BASE_URL || "https://open.bigmodel.cn/api/paas/v4"`, `api_key: ZHIPU_API_KEY || ZAI_API_KEY`, `model: ZHIPU_LLM_MODEL || "glm-4-plus"`(注:用当前确实存在的型号作默认;待核实可换,但默认不能是不存在的 id)。
  - `siliconflow` → `base_url || "https://api.siliconflow.cn/v1"`, `api_key: SILICONFLOW_API_KEY`, `model || "Qwen/Qwen3-8B"`。
  - `openai_compatible` → `OPENAI_COMPATIBLE_BASE_URL` / `OPENAI_COMPATIBLE_API_KEY` / `OPENAI_COMPATIBLE_MODEL`(均必填)。
  - 兼容类 provider 若 key/base_url/model 任一缺失 → 回退 mock(并经现有 `system.status` 提示"<provider> 配置缺失,使用 mock LLM")。
- `reply_provider_name_for_test` 等测试辅助相应更新(支持注入 env 解析的纯函数版,便于不依赖真实 env 的单测)。
- 不在适配器里读 env。

## 测试与验证

确定性、无网络(reqwest 经 mock transport 绕过):

- **Chat dialect map**(表驱动,核心):喂各类 SSE JSON `Value` 断言 `ReplyChunk`:正常 `delta.content`→Token;缺失/null content→Ignore;`reasoning_content`→Ignore;空 `choices`(usage)→Ignore;`error`→Error。
- **OpenAiCompatibleReplyClient(mock ChatTransport)**:喂 `[delta("Hi "), delta("there."), DONE-via-None]` → `Started → Token("Hi ") → Token("there.") → Final("Hi there.")`;错误事件 → failure-final 不泄露 key。
- **build_chat_body**:含 `stream:true`、`model`、system 指令、context + 当前轮。
- **base_url 拼接**:带尾斜杠与不带都拼成正确的 `/chat/completions`(单测纯函数)。
- **env resolver**(纯函数版,注入 env map):各 `LLM_PROVIDER` → 正确 provider 名;缺配置 → mock。更新现有 `commands.rs` 两个 provider 选择测试。
- **共享引擎重构回归**:`tests/openai_responses_llm.rs`、`tests/llm_orchestration.rs`、`tests/commands.rs` 全部保持绿。
- 全量 `cargo test` 绿、`cargo check` 干净;前端 `npm test` 不受影响。
- 网络 e2e(`tests/e2e_real_network.rs`)新增各 provider 的门控用例(`#[ignore]`,需对应 key 时手动跑)。

## 验收标准

- 共享引擎抽出,Responses 与 Chat Completions 两 dialect 复用同一 worker/SSE 读取;无重复的线程/poll/取消逻辑。
- 设置 `LLM_PROVIDER=dashscope|zhipu|siliconflow|openai_compatible` + 对应 key 后,真实回复经各家 `/chat/completions` 流式返回(门控 e2e 验证);缺配置自动回退 mock 并提示。
- SSE 解析容忍缺失/null content、reasoning_content、空 choices/usage、注释行、无 `[DONE]` 直接断流。
- key 不出现在任何错误信息/日志中(沿用现有 redaction)。
- 适配器不读 env;config 为结构体。

## 后续跟进
- 桌面端设置面板:选 provider + 填 key/base_url/model,落地到安全存储,供 resolver。
- 多家实时 ASR provider(另一轮)。
- 取消时主动中断上游 HTTP 的进一步验证(本轮在共享引擎内实现 sender-disconnected 检查)。
