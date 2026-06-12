# 流式 ASR 编排设计(抽象层 + Mock + 编排)

日期：2026-06-13

## 背景与范围

真实管线三件套的第二件。第一件(WASAPI loopback 采集)已交付,产出 `Receiver<AudioFrame>`(16 kHz / 单声道 / i16,320 样本/20 ms 帧)。本设计把这条帧流接到流式 ASR,产出 `AsrEvent`(partial / final / endpoint),供前端 transcript/reply 引擎消费。

延续整个项目 mock-first 的节奏:本轮只做**与服务商无关的可测核心**——把 `StreamingAsrClient` 变成真正的流式接口、做确定性 `MockAsrClient`、加本地能量端点器、写"采集帧 → ASR → AsrEvent"的编排,全部确定性单测,无网络、无凭证。

**范围内**:

- `StreamingAsrClient` 流式接口 + `AsrError`。
- `MockAsrClient` 确定性实现。
- `EnergyEndpointer` 本地能量/静音端点检测。
- `TranscriptionSession` 编排(帧流 → AsrEvent 流)。
- 全部确定性单测(合成帧驱动)。

**范围外(各自后续小项目)**:

- 真实云端流式 ASR 的 WebSocket 适配器(需选服务商 + 凭证 + 语言)。
- 把 `AsrEvent` 经 Tauri `emit` 桥接到前端 UI(薄胶水)。
- 麦克风采集(永久排除,违反 MVP 契约)。

## 设计原则

隔离与可测:把"文本识别"(ASR)与"轮次/静音检测"(端点器)拆成两个独立组件,编排只负责接线与顺序。端点检测**与服务商无关**——从音频帧本地计算,而不是依赖某个 ASR 服务的端点能力。所有逻辑用合成帧确定性测试,不引入网络或异步运行时;未来真实云适配器在内部桥接其异步 WebSocket,对外仍是同步的 `StreamingAsrClient`。

## 现有契约(不变)

`src-tauri/src/asr/client.rs` 已定义 `AsrEvent`,序列化为前端 RealtimeEvent 契约(内部 `type` tag + camelCase 字段),本设计保持其字段不变:

- `Partial { session_id, text, started_at_ms, ended_at_ms, received_at_ms }`
- `Final { session_id, text, started_at_ms, ended_at_ms, received_at_ms }`
- `Endpoint { session_id, silence_ms, detected_at_ms }`

## 模块划分(`src-tauri/src/asr/`)

- `client.rs`(改造):`StreamingAsrClient` 流式 trait + `AsrError`;`AsrEvent` 保持不变。
- `mock.rs`(充实):`MockAsrClient`。
- `endpointer.rs`(新增):`EnergyEndpointer`。
- `session.rs`(新增):`TranscriptionSession`。
- `mod.rs`(改):`pub mod endpointer; pub mod session;`。

## 流式接口

```rust
#[derive(Debug, thiserror::Error)]
pub enum AsrError {
    #[error("asr stream closed")]
    Closed,
    #[error("asr provider error: {0}")]
    Provider(String),
}

pub trait StreamingAsrClient: Send {
    fn name(&self) -> &'static str;
    /// 送入一帧 16k/mono/i16 音频;可能在内部产生 partial(经 events() 取出)。
    fn push_frame(&mut self, frame: &AudioFrame) -> Result<(), AsrError>;
    /// 取该会话的事件接收端(partial / final)。clone 安全。
    fn events(&self) -> Receiver<AsrEvent>;
    /// 收束当前 utterance:产出 final 并准备下一段。
    fn finalize(&mut self) -> Result<(), AsrError>;
}
```

`events()` 只承载 `Partial` 与 `Final`;`Endpoint` 由编排从端点器产生。客户端按会话实例化(一个实例服务一次会话)。

## MockAsrClient

确定性,由帧序列驱动,无网络:

- 内部持有 `session_id`、累积帧计数 `frames_since_partial`、当前 utterance 的 `started_at_ms`(首帧的 `captured_at_ms`)、最近帧 `ended_at_ms`、一个脚本短语列表。
- `push_frame`:更新计数与时间戳;每累积 `PARTIAL_EVERY_FRAMES`(如 25 帧 ≈ 0.5 s)发一个递进 `Partial`(取脚本短语前缀),`received_at_ms` 用帧时间。
- `finalize`:发当前 utterance 的 `Final`(完整脚本短语),重置计数与短语游标到下一句。
- 事件通过内部 `Sender<AsrEvent>` 投递,`events()` 返回对应 `Receiver` 的 clone。

## EnergyEndpointer

本地、与服务商无关、纯逻辑:

```rust
pub enum EndpointSignal { StartOfSpeech, EndOfSpeech }

pub struct EnergyEndpointer {
    speech_threshold: f32,     // RMS 归一化阈值
    silence_window_ms: u32,    // 自适应静音确认窗口
    in_speech: bool,
    silence_accum_ms: u32,
}
```

- `observe(&mut self, frame: &AudioFrame) -> Option<EndpointSignal>`:
  - 计算该帧 RMS(i16 归一化到 [-1,1])。
  - RMS ≥ `speech_threshold` 视为语音:若此前非语音 → 置 `in_speech`,清零静音累计,返回 `StartOfSpeech`;否则清零静音累计。
  - RMS < 阈值视为静音:若 `in_speech`,累加该帧时长(`frame.duration_ms()`,20 ms);累计 ≥ `silence_window_ms` → 置非语音、清零,返回 `EndOfSpeech`;否则 `None`。
- 静音窗口沿用前端 endpointing 思路(默认约 300 ms,可配),初版用固定值,自适应留作配置点。
- 纯函数式状态机,合成帧确定性可测。

## TranscriptionSession 编排

```rust
pub struct TranscriptionSession { /* receiver, stop, handle */ }

impl TranscriptionSession {
    pub fn start(
        session_id: String,
        frames: Receiver<AudioFrame>,
        client: Box<dyn StreamingAsrClient>,
        endpointer: EnergyEndpointer,
    ) -> TranscriptionSession;
    pub fn events(&self) -> Receiver<AsrEvent>;
    pub fn stop(self) -> Result<(), AsrError>;
}
```

worker 线程循环:

1. `frames.recv()` 取帧(channel 关闭 → 进入收尾)。
2. `let signal = endpointer.observe(&frame);`
3. `client.push_frame(&frame)?;` 然后排空 `client.events()`,把 `Partial` 转发到输出。
4. 若 `signal == EndOfSpeech`:先发 `AsrEvent::Endpoint { silence_ms, detected_at_ms = frame.captured_at_ms }`,再 `client.finalize()?`,排空 `client.events()` 把 `Final` 转发到输出 —— **保证 endpoint 先于 final**(前端 replyEngine 所需)。
5. 收尾(channel 关闭或 stop):若处于语音中,`finalize()` 一次,转发剩余事件,退出。

- 输出用足够大的 `bounded` channel(如 256),转发用阻塞 `send`。ASR 事件速率低(partial ~每秒数个、endpoint/final 每轮一次),与 50 帧/秒的音频不同,消费者正常排空时实际不会阻塞;关键的 `endpoint`/`final` **绝不丢弃**(不采用 try_send 丢弃策略)。`send` 在消费端断开时返回错误,worker 据此退出。
- `stop()`:置停止标志 + join,返回线程 `Result`。`Drop` 兜底。

## 数据流

```
capture.receiver()  (16k/mono/i16, 20ms 帧)
  -> TranscriptionSession worker
       -> EnergyEndpointer.observe  --(EndOfSpeech)-->  endpoint.detected
       -> client.push_frame         --> Partial(s)
       -> (EndOfSpeech) client.finalize --> Final
  -> events(): Receiver<AsrEvent>   (partial -> endpoint -> final, 顺序保证)
  -> (后续) Tauri emit -> 前端 transcript/reply 引擎
```

## 错误处理

- `AsrError` 区分 `Closed`(流已关)与 `Provider(String)`(真实适配器的协议/网络错,mock 不产生)。
- worker 致命错误:`tracing` 记录 + 关闭输出 channel 退出;消费者见断开即知结束;`stop()`/join 返回 `Result`。翻译成 `system.status` 由上层(Tauri 桥)负责,不在本范围。

## 测试与验证

全部确定性,合成 `AudioFrame`(用已有 `AudioFrame`/`PcmFormat`,样本为合成正弦/常数/零),无网络、无异步运行时。

- **EnergyEndpointer**:
  - 纯静音帧 → 无信号。
  - 响帧(高 RMS)→ `StartOfSpeech`(仅首次)。
  - 响帧后连续静音帧:累计未达窗口 → `None`;达到窗口 → `EndOfSpeech`,且仅一次。
  - 再次响帧 → 再次 `StartOfSpeech`(状态复位)。
- **MockAsrClient**:
  - 喂 N 帧 → 收到预期数量、文本递进的 `Partial`。
  - `finalize` → 收到 `Final`(完整短语),游标推进到下一句。
  - 跨 session_id 不混。
- **TranscriptionSession(端到端)**:
  - 构造合成帧流:一段响帧 + 一段足够长的静音帧,经内存 channel 送入。
  - 断言输出 `AsrEvent` 顺序为 `Partial(>=1) → Endpoint → Final`,session_id 一致,时间戳取自帧。
  - channel 关闭后 `stop()` 返回 Ok,输出 channel 断开。

## 验收标准

- 上述全部单测通过,`cargo test` 绿,`cargo check` 干净。
- `events()` 输出对单条 utterance 满足 `partial… → endpoint.detected → final` 顺序。
- 端点检测与服务商无关(仅依赖音频帧,不依赖 ASR 文本/服务)。
- `AsrEvent` 字段与前端 RealtimeEvent 契约一致(camelCase),序列化测试保持。
- 不存在麦克风/输入采集路径。

## 后续跟进(独立项)

- 真实云端流式 ASR WebSocket 适配器(实现 `StreamingAsrClient`):选服务商、语言、凭证管理。
- Tauri 命令 + `emit` 桥:启动会话、把 `AsrEvent` 推送到前端 UI。
- 流式 LLM 接入(真实管线第三件)。
