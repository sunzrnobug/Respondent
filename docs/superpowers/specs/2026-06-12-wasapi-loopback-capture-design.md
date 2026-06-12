# WASAPI Loopback 真实采集设计

日期：2026-06-12

## 背景与范围

低延迟会议助手当前的 `audio` 模块只有契约和骨架：`LoopbackCapture` 包了一个 crossbeam channel，由 `push_test_frame` 投递假帧。本设计把这层骨架替换为**真实的事件驱动 WASAPI loopback 采集**，作为"真实管线"三个独立子系统中的第一个（其余为流式 ASR、流式 LLM，各自走独立 spec→plan）。

本子项目交付：从当前默认输出（render）端点采集系统输出音频，转换为 **16 kHz / 单声道 / i16 PCM** 帧，通过现有 `receiver()` 接缝流式输出，并提供干净的 start/stop。

明确**不在本子项目范围**（留作后续跟进项）：

- 会话中途设备切换的自动重连。
- 友好设备名枚举与多设备选择 UI。
- 把采集错误翻译成 `system.status` 事件（属上层编排）。
- 流式 ASR / LLM 接入。
- 麦克风采集（违反 MVP 契约，永久排除）。

## 设计原则

核心是**隔离**：把纯转换逻辑与 `unsafe` 的 WASAPI I/O 彻底分开。

- 纯逻辑（下混、重采样、量化、分帧）放在 `convert.rs`，无 `windows`、无 I/O，**全部单测**。
- `unsafe` 的 COM/WASAPI 调用放在 `capture.rs` 的 `cfg(windows)` 实现里，做薄壳，靠编译 + 手动/门控集成测试验证。

接缝保持不变：`LoopbackCapture::receiver() -> Receiver<AudioFrame>`，消费者代码零改动。

## 模块划分

`src-tauri/src/audio/`：

- `frame.rs`（不变）：`AudioFrame`、`PcmFormat`。
- `convert.rs`（新增，纯逻辑）：下混 + 重采样 + 量化 + 分帧。
- `capture.rs`（重写）：`cfg(windows)` 的 WASAPI loopback 采集线程与生命周期；非 Windows / 出错时保留 channel 形态。

## 纯转换流水线（convert.rs）

WASAPI 线程按设备 `WAVEFORMATEX` 把缓冲解释为**交错 f32**后，交给以下纯组件。目标格式常量：`TARGET_RATE = 16_000`、`TARGET_FRAME_SAMPLES = 320`（20 ms @ 16 kHz 单声道）。

1. **下混**：`downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32>`，按声道求平均。`channels == 0` 视为空输入返回空。

2. **重采样（有状态）**：
   ```
   LinearResampler { src_rate: u32, dst_rate: u32, pos: f64, last: f32 }
   fn process(&mut self, input: &[f32]) -> Vec<f32>
   ```
   线性插值。`pos` 表示当前输出采样落在输入序列上的小数位置，`step = src_rate / dst_rate`。跨帧保留 `pos` 的小数部分和上一帧最后一个样本 `last`，保证帧边界连续，不在每帧开头产生不连续。`src_rate == dst_rate` 时直通。

3. **量化**：`to_pcm16(&[f32]) -> Vec<i16>`，先 clamp 到 `[-1.0, 1.0]`，再乘 `i16::MAX` 四舍五入。

4. **分帧（有状态）**：
   ```
   FrameChunker { buf: Vec<i16> }
   fn push(&mut self, samples: &[i16]) -> Vec<Vec<i16>>
   ```
   累积样本，每满 `TARGET_FRAME_SAMPLES` 切出一帧，余数留在 `buf`。

这四块组合成 `convert.rs` 内的一个 `CapturePipeline { resampler, chunker, src_rate, channels }`，对外只暴露 `fn push_interleaved_f32(&mut self, interleaved: &[f32], captured_at_ms: u64) -> Vec<AudioFrame>`：下混→重采样→量化→分帧，输出 0 个或多个 `AudioFrame`（format 固定 16k/mono/i16）。

## WASAPI 采集线程（capture.rs, cfg(windows)）

### 生命周期

```
LoopbackCapture::start(device_id: &str) -> Result<LoopbackCapture, CaptureError>
  - 起采集线程,返回持有 receiver() 与停止句柄的 LoopbackCapture
LoopbackCapture::receiver(&self) -> Receiver<AudioFrame>
LoopbackCapture::stop(self) -> Result<(), CaptureError>
  - 发停止信号(AtomicBool 或 event) + join 线程,返回线程结果
```

### 线程主体

1. `CoInitializeEx(None, COINIT_MULTITHREADED)`（忽略已初始化）。
2. 取目标 render 端点 `IMMDevice`（`device_id` 为空或匹配失败时退默认端点）。
3. `IMMDevice::Activate` 得 `IAudioClient`。
4. `GetMixFormat` 得 `WAVEFORMATEX`：`src_rate`、`channels`、格式标签（IEEE float vs PCM）、位深。
5. `Initialize` 共享模式，flags = `AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK`，`SetEventHandle`。
6. 取 `IAudioCaptureClient`，`Start`。
7. 循环直到停止信号：`WaitForSingleObject(event, timeout)` → `GetBuffer` → 按格式把字节解释为交错 f32（float 直读；PCM16 转换）→ `pipeline.push_interleaved_f32(...)` → 帧入 channel → `ReleaseBuffer`。`AUDCLNT_BUFFERFLAGS_SILENT` 标记的包按静音处理。
8. 退出前 `Stop`、释放 COM。

### 关键决策

- **背压**：channel `bounded(128)`，用 `try_send`；满时**丢最旧帧并递增丢帧计数**（`AtomicU64`，可经 `stop()` 结果或方法读出），绝不阻塞 WASAPI 循环——阻塞会导致采集 underrun/爆音。
- **错误处理**：线程致命错误（COM 失败、设备消失）→ `tracing` 记录 + 关闭 channel 退出；消费者见 channel 断开即知采集结束；`stop()`/join 返回 `Result<(), CaptureError>`。翻译成 `system.status` 由上层负责。
- **时间戳**：`captured_at_ms` 取单调时钟（`std::time::Instant` 相对会话起点）的毫秒，标在帧上。
- **实时安全**：共享模式采集非硬实时上下文；循环内只做必要的转换与 `try_send`，避免阻塞调用、锁、重分配尖峰（`pipeline` 缓冲复用）。

### CaptureError

`thiserror` 定义：`Com(windows::core::Error)`、`NoDefaultEndpoint`、`Unsupported(String)`（不支持的格式标签/位深）。

## 数据流

```
WASAPI event-driven loopback (设备混音格式: 常 48kHz/f32/stereo)
  -> GetBuffer 原始字节
  -> 解释为交错 f32
  -> downmix_to_mono
  -> LinearResampler.process (-> 16kHz, 跨帧连续)
  -> to_pcm16
  -> FrameChunker (-> 320 样本/帧)
  -> AudioFrame { 16k/mono/i16, captured_at_ms }
  -> try_send 到 bounded channel
  -> receiver() 消费 (下一子系统: 流式 ASR)
```

## 测试与验证

### 单元测试（convert.rs，纯逻辑全覆盖）

- 下混：立体声 `[l, r, l, r]` → 单声道平均；单声道直通；`channels == 0` → 空。
- 重采样：
  - `src == dst` 直通（输出等于输入）。
  - 48k→16k：整数 3:1，已知输入得预期长度与插值值。
  - 44.1k→16k：分数比，长度近似 `len * 16000 / 44100`，值单调插值。
  - **跨帧连续性**：把一段斜坡信号分两次 `process`，拼接结果与一次性处理在边界处一致（无跳变）。
- 量化：clamp 后乘 `i16::MAX`，故 `1.5 -> 32767`、`-1.5 -> -32767`、`0.0 -> 0`。
- 分帧：推 800 样本 → 切出 2 帧(各 320) + 余 160；再推 160 → 再出 1 帧。
- 流水线：喂一段 48k/stereo/f32 → 得到若干 320 样本、format 为 16k/mono/i16 的帧。

### WASAPI 线程验证

WASAPI loopback 在渲染端点**空闲时不投递数据包**，故静音冒烟测试可能收不到帧。因此：

- 一个默认 `#[ignore]` 的门控集成测试 `loopback_capture_smoke`：放着音频时手动 `cargo test -- --ignored` 运行，断言数秒内收到 ≥1 帧、format == 16k/mono、长度 320、且至少一帧非全零。
- 验证清单新增手动步骤：本机播放音频 → 启动采集 → 观察到非静音 16k/mono 帧；停止后 channel 断开、`stop()` 返回 Ok。

## 验收标准

- `convert.rs` 全部纯逻辑单测通过。
- `cargo build`（含 `cfg(windows)` WASAPI 代码）在本机工具链编译通过。
- 不存在任何麦克风/输入采集路径（仅 render/loopback）。
- 门控集成测试在播放音频时通过：收到 16k/mono/320 的非静音帧。
- `receiver()` 接缝签名不变，现有消费者无需改动。

## 后续跟进（独立项）

- 设备切换自动重连。
- 友好设备名枚举与选择。
- 采集错误 → `system.status` 上层编排。
- 流式 ASR 接入（消费 `receiver()` 的 16k/mono 帧）。
