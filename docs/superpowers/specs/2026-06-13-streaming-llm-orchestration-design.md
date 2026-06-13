# 流式 LLM 编排设计(抽象层 + Mock + 触发 + 编排)

日期：2026-06-13

## 背景与范围

真实管线三件套的第三件。前两件已交付:WASAPI loopback 采集产出 `Receiver<AudioFrame>`;流式 ASR 编排(`TranscriptionSession`)消费帧流、产出 `Receiver<AsrEvent>`(partial / final / endpoint)。本设计把 `AsrEvent` 流接到流式 LLM,在 endpoint+final 时触发回复生成,产出 `ReplyEvent`(started / token / final)供前端消费。

整条管线(采集 → ASR → LLM)都在 Rust 侧;前端只负责显示。前端已有的 `replyEngine.ts` 此后仅保留给纯前端 mock-UI 模式。延续项目 mock-first 节奏:本轮只做与服务商无关的可测核心。

**范围内**:

- `StreamingReplyClient` 流式接口 + `ReplyGeneration` / `ReplyPoll` + `LlmError`。
- `MockReplyClient` 确定性实现。
- `ReplyTrigger` 触发策略(端点触发 + 滚动上下文,纯逻辑)。
- `ReplySession` 编排(AsrEvent 流 → ReplyEvent 流,最新优先)。
- 全部确定性单测(合成 AsrEvent 驱动)。

**范围外(各自后续小项目)**:

- 真实 Claude/Anthropic 流式 LLM 适配器(实现 `StreamingReplyClient`,需凭证 + prompt 工程)。
- 把 capture → ASR → LLM 三段串起来、经 Tauri `emit` 桥接到前端 UI。
- 麦克风采集(永久排除)。

## 设计原则

隔离与可测,镜像 ASR 子项目:把"何时触发"(`ReplyTrigger`,纯)与"如何生成"(`StreamingReplyClient`)分开,`ReplySession` 只负责接线、最新优先与顺序。同步线程 + crossbeam channel,无 async 运行时;未来真实适配器在内部桥接其异步 WebSocket,对外仍是同步 trait。LLM 用**拉模型**(`poll`)而非推模型,既让 mock 确定性(无线程无真实时序),又让取消可观测(丢弃 generation 即停)。

## 现有契约(不变)

`src-tauri/src/llm/client.rs` 已定义 `ReplyRequest` 与 `ReplyEvent`(序列化为前端 RealtimeEvent 契约:内部 `type` tag + camelCase),本设计保持其字段不变:

- `ReplyRequest { session_id, generation_id, transcript, context: Vec<String> }`
- `ReplyEvent::Started { session_id, generation_id, based_on_transcript_event_id, received_at_ms }`
- `ReplyEvent::Token { session_id, generation_id, token, received_at_ms }`
- `ReplyEvent::Final { session_id, generation_id, text, received_at_ms }`

## 模块划分(`src-tauri/src/llm/`)

- `client.rs`(改造):流式 trait + `ReplyGeneration` + `ReplyPoll` + `LlmError`;`ReplyRequest`/`ReplyEvent` 不变。
- `mock.rs`(充实):`MockReplyClient` + `MockReplyGeneration`。
- `reply_trigger.rs`(新增):`ReplyTrigger`。
- `session.rs`(新增):`ReplySession`。
- `mod.rs`(改):`pub mod reply_trigger; pub mod session;`。

## 流式接口

```rust
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("reply stream closed")]
    Closed,
    #[error("llm provider error: {0}")]
    Provider(String),
}

pub enum ReplyPoll {
    Event(ReplyEvent),
    Pending, // 真实适配器等待网络下一 token 时返回
    Done,
}

pub trait ReplyGeneration: Send {
    /// 拉取该次生成的下一个事件。丢弃本对象即取消生成。
    fn poll(&mut self) -> ReplyPoll;
}

pub trait StreamingReplyClient: Send {
    fn name(&self) -> &'static str;
    /// 开始一次回复生成,返回该次生成的拉取句柄。
    fn start(&self, request: ReplyRequest) -> Box<dyn ReplyGeneration>;
}
```

**取消语义**:`ReplySession` 在新触发时把 `active` 替换为新的 generation,旧的 `Box<dyn ReplyGeneration>` 被 drop 即停止;无显式取消事件——前端依据新的 `reply.started`(新 `generationId`)重置当前建议。真实适配器在其 `Drop` 中 abort 异步任务。

## MockReplyClient

确定性,由 `start` 的 `ReplyRequest` 决定输出,无网络无线程:

- `start(request)` 返回 `MockReplyGeneration`,持有 `generation_id`、`session_id`、一个事件脚本队列、游标。
- 脚本:对给定 `transcript` 生成固定回复,例如 `"Acknowledged: <transcript的前若干词>"` 拆成 token。队列为 `[Started, Token(t1), Token(t2), …, Final]`。
- `based_on_transcript_event_id` 取 `format!("transcript-{generation_id}")`。
- `poll()` 依次弹出队列下一项并包成 `ReplyPoll::Event(...)`,队列空后返回 `ReplyPoll::Done`;永不返回 `Pending`(确定性)。
- 所有 `ReplyEvent` 的 `received_at_ms` 用一个从 0 起、每 token 递增的合成时钟(确定性,不依赖真实时间)。

## ReplyTrigger

纯逻辑,移植前端 `replyEngine.ts` 的端点触发语义:

```rust
pub struct ReplyTrigger {
    session_id: String,
    endpoint_armed: bool,
    context: Vec<String>,   // 滚动保留最近的 final,最多 MAX_CONTEXT_TURNS
    generation_counter: u64,
}
```

- 常量 `MAX_CONTEXT_TURNS = 6`。
- `observe(&mut self, event: &AsrEvent) -> Option<ReplyRequest>`:
  - `Endpoint` → `endpoint_armed = true`,返回 `None`。
  - `Final { text }` → 压入 `context`(超出 6 则移除最旧);若 `endpoint_armed` 则清旗、`generation_counter += 1`,返回 `Some(ReplyRequest { session_id, generation_id: format!("gen-{generation_counter}"), transcript: text, context: context.clone() })`;否则 `None`。
  - `Partial` → `None`。
- 纯函数式,合成 `AsrEvent` 确定性可测。

## ReplySession 编排

```rust
pub struct ReplySession { /* events 接收端, stop, handle */ }

impl ReplySession {
    pub fn start(
        asr_events: Receiver<AsrEvent>,
        client: Box<dyn StreamingReplyClient>,
        trigger: ReplyTrigger,
    ) -> ReplySession;
    pub fn events(&self) -> Receiver<ReplyEvent>;
    pub fn stop(self) -> Result<(), LlmError>;
}
```

worker 线程每轮:

1. **排空当前所有可用 `AsrEvent`**(`try_recv` 循环):每个过 `trigger.observe`;若返回 `Some(req)`,则 `active = Some(client.start(req))`(替换并丢弃旧的 generation = 取消)。记录 channel 是否 `Disconnected`。
2. **泵一次** `active`:`active.poll()` → `Event(e)` 转发到输出;`Pending` → 短 `sleep` 让出(mock 不会触发);`Done` → `active = None`。
3. 若 `active.is_none()`:若输入已 `Disconnected` 则退出;否则 `recv_timeout(IDLE_WAIT)` 阻塞等下一个输入(同时周期性检查 stop 标志),取到则同样过 `trigger`。

- **最新优先天然成立且确定性**:预填两组 `endpoint+final` 时,worker 在第 1 步一次性排空两者,gen-1 在被 poll 之前即被 gen-2 取代,故只输出 gen-2。
- 输出用足够大的 `bounded` channel,转发用 `send_timeout`,消费端断开 → `LlmError::Closed`,worker 退出。
- `stop()`:置 `AtomicBool` + join,返回线程 `Result`;`Drop` 兜底 signal + join。

## 数据流

```
TranscriptionSession.events()  (Receiver<AsrEvent>: partial/final/endpoint)
  -> ReplySession worker
       -> ReplyTrigger.observe  --(endpoint+final)-->  ReplyRequest
       -> client.start(req)     --> active generation (latest-wins on new trigger)
       -> active.poll()         --> Started -> Token(s) -> Final
  -> events(): Receiver<ReplyEvent>
  -> (后续) Tauri emit -> 前端 sessionStore (reply.started/token/final)
```

## 错误处理

- `LlmError`:`Closed`(输出/流已关)与 `Provider(String)`(真实适配器协议/网络错;mock 不产生)。
- worker 致命错误或输出断开:`tracing` 记录(若可用,否则静默)+ 关闭输出 channel 退出;消费者见断开即知结束;`stop()`/join 返回 `Result`。翻译成 `system.status` 由上层(Tauri 桥)负责。

## 测试与验证

全部确定性,合成 `AsrEvent`(用既有 `AsrEvent`)与内存 channel,无网络、无真实时序、无 async 运行时。

- **ReplyTrigger**:
  - `Endpoint` 后 `Final` → `Some`,`generation_id == "gen-1"`,`transcript`/`context` 正确。
  - `Final` 无前置 `Endpoint` → `None`。
  - `Partial` → `None`。
  - 连续 7 轮 `endpoint+final` → `context` 只保留最近 6 个;`generation_counter` 递增(gen-1..gen-7)。
- **MockReplyClient / MockReplyGeneration**:
  - `start(req)` 后反复 `poll` 依次得 `Started`(带正确 generation_id/session_id)→ 至少一个 `Token` → `Final`(完整文本)→ `Done`。
- **ReplySession(端到端)**:
  - 喂 `partial → endpoint → final`,drop 输入 → 输出顺序 `Started → Token(>=1) → Final`,generation_id 为 `gen-1`,session_id 一致。
  - **最新优先**:预填 `endpoint, final("A"), endpoint, final("B")`,drop 输入 → 收集输出,断言末个 `Final` 的 `generation_id == "gen-2"`(gen-1 被取代,不出 Final)。
  - 输入 channel 关闭后 `stop()` 返回 `Ok`,输出 channel 断开。

## 验收标准

- 上述全部单测通过,`cargo test` 绿,`cargo check` 干净。
- `events()` 对单次触发满足 `started → token… → final` 顺序;新触发以新 `generationId` 取代在途生成。
- 触发与上下文逻辑与服务商无关(仅依赖 `AsrEvent`,不依赖具体 LLM)。
- `ReplyEvent` 字段与前端 RealtimeEvent 契约一致(camelCase),序列化测试保持。
- 不存在麦克风/输入采集路径。

## 后续跟进(独立项)

- 真实 Claude/Anthropic 流式 LLM 适配器(实现 `StreamingReplyClient`):凭证管理、prompt 构造、流式 token、取消(Drop abort)。
- 端到端编排:capture → ASR → LLM 串联,Tauri 命令 + `emit` 桥,把 `AsrEvent`/`ReplyEvent` 推送到前端 UI。
