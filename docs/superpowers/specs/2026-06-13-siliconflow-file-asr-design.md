# 硅基流动文件式 ASR 适配器设计

日期：2026-06-13

## 背景与范围

ASR 目前只有 OpenAI Realtime(WebSocket 流式)一家,无 key 时回退 mock。硅基流动提供**文件式转写**:`POST {base_url}/audio/transcriptions`,multipart/form-data 上传音频文件,返回 `{ "text": string }`,模型如 `FunAudioLLM/SenseVoiceSmall`。本轮新增一个文件式 ASR 适配器,并把 ASR provider 选择泛化(镜像已有的 LLM resolver)。

**关键洞察**:现有 `StreamingAsrClient`(`push_frame` + `events` + `finalize`)已能容纳文件式形态——编排 `TranscriptionSession` 已在 `EndOfSpeech` 时调用 `client.finalize()`(`session.rs:123`)。文件式客户端只需:`push_frame` 缓冲 PCM,`finalize` 编码 WAV + 上传 + 发一个 `Final`。**编排零改动,不改 trait。**

**范围内**:
- `SiliconFlowFileAsrClient`(实现 `StreamingAsrClient`):缓冲 → finalize 上传转写 → 发 Final(无 partial)。
- 纯函数 WAV(PCM16/mono)内存编码器。
- 可 mock 的 `TranscriptionTransport`(真实实现:reqwest blocking multipart)。
- `commands.rs` 泛化 ASR resolver:`ASR_PROVIDER` + per-provider 配置(env 只在 commands)。
- 确定性单测 + 一个门控真实网络冒烟。
- reqwest 增加 `multipart` feature。

**范围外**:
- 百炼 fun-asr-realtime(流式 WebSocket)——另一轮(同形于 OpenAI Realtime)。
- 超长句强制分段上限(本轮靠端点器自然切句,记为跟进)。
- 实时 partial(文件式本就没有)。

## 设计原则

不改 `StreamingAsrClient` trait。文件式与流式在同一 trait 下,差别仅在"push_frame 是否联网产 partial"与"finalize 做什么"。env 解析只在 commands,适配器吃显式 config 结构体(便于将来设置面板)。转写传输抽象为可 mock 的 trait,核心逻辑(缓冲、WAV 编码、事件)无网络确定性可测。

## 现有契约(不变)

- `AudioFrame { format: PcmFormat{sample_rate,channels,bits_per_sample}, samples: Vec<i16>, captured_at_ms: u64 }`,管线帧恒为 16 kHz / mono / i16。
- `AsrEvent::Final { session_id, text, started_at_ms, ended_at_ms, received_at_ms }`(camelCase 序列化,前端契约)。
- `StreamingAsrClient { name, push_frame, events, finalize }`,`AsrError`。

## 模块划分(`src-tauri/src/asr/`)

- `siliconflow_file.rs`(新增):`SiliconFlowFileConfig`、`SiliconFlowFileAsrClient`、`TranscriptionTransport` trait、`ReqwestTranscriptionTransport`、`encode_wav_pcm16_mono`。
- `mod.rs`(改):`pub mod siliconflow_file;`。
- `commands.rs`(改):ASR resolver。

## WAV 编码器(纯函数)

```rust
/// 把 16-bit PCM 单声道样本编码成内存 WAV(canonical 44 字节头 + LE i16 数据)。
pub fn encode_wav_pcm16_mono(samples: &[i16], sample_rate: u32) -> Vec<u8>;
```

- 头:`RIFF` + chunk size(36 + data_len)+ `WAVE` + `fmt `(16, PCM=1, channels=1, sample_rate, byte_rate=sample_rate*2, block_align=2, bits=16)+ `data` + data_len;随后小端 i16 样本。
- 纯函数,单测断言:总长 = 44 + 2*samples.len()、魔数 `RIFF`/`WAVE`/`fmt `/`data`、sample_rate/声道/位深字段、首个样本字节。

## SiliconFlowFileAsrClient

```rust
pub struct SiliconFlowFileConfig { pub base_url: String, pub api_key: String, pub model: String }

pub trait TranscriptionTransport: Send + Sync {
    /// 上传一段 WAV,返回转写文本。
    fn transcribe(&self, config: &SiliconFlowFileConfig, wav: &[u8]) -> Result<String, AsrError>;
}

pub struct SiliconFlowFileAsrClient { /* session_id, config, transport, sender/receiver<AsrEvent>, buffer: Vec<i16>, started_at_ms: Option<i64>, last_ended_at_ms: i64, sample_rate: u32 */ }
```

- `new`/`connect(session_id, config)` + `with_transport(session_id, config, transport)`;构造校验 `api_key`/`base_url`/`model` 非空,否则 `AsrError`。`sample_rate` 取 16000(管线恒定)。
- `name()` → `"siliconflow-file-asr"`。
- `push_frame(&frame)`:首帧记 `started_at_ms = frame.captured_at_ms`;更新 `last_ended_at_ms = captured_at_ms + duration_ms`;`buffer.extend_from_slice(&frame.samples)`。**不发事件、不联网。**(若帧 sample_rate ≠ 16000,记 warning;管线已保证 16k,直接用 buffer。)
- `events()` → receiver 克隆。
- `finalize()`:
  - `buffer` 为空 → 返回 Ok(无 utterance)。
  - 否则:`wav = encode_wav_pcm16_mono(&buffer, 16000)`;`transport.transcribe(&config, &wav)`:
    - `Ok(text)` 且 text 非空 → 发 `AsrEvent::Final { session_id, text, started_at_ms, ended_at_ms=last_ended_at_ms, received_at_ms=now }`。
    - `Ok(空串)` → 不发 Final(静默段)。
    - `Err(_)` → **不发 Final、记 tracing/eprintln、返回 Ok**(单次转写失败不终止会话)。
  - 无论结果,清空 `buffer`、`started_at_ms=None`(为下一段武装)。
- 不产生 `AsrEvent::Partial`(文件式无 interim);`AsrEvent::Endpoint` 仍由编排的 `EnergyEndpointer` 产生(不变)。

### ReqwestTranscriptionTransport（真实）
- reqwest blocking,`POST join_url(base_url,"/audio/transcriptions")`(去尾斜杠拼接,复用与 LLM 同样的规范化思路),`bearer_auth(api_key)`,multipart:`file`(wav 字节,filename `audio.wav`,mime `audio/wav`)+ `model` 文本字段。
- 非 200 → `AsrError`(body 经截断,**不含 key**)。200 → 解析 JSON 取 `text` 字段(缺失视为空串)。
- 需要 reqwest `multipart` feature。

## ASR Resolver（commands.rs）

镜像 LLM 的 `resolve_reply_client`。新增 `resolve_asr_client(session_id: &str, env: &HashMap<String,String>) -> Result<(Box<dyn StreamingAsrClient>, bool /*using_mock*/), String>`:

- `ASR_PROVIDER`(缺省 `openai_realtime`),小写匹配:
  - `openai_realtime` → `OPENAI_API_KEY` 非空 → `OpenAiRealtimeAsrClient`(现状)否则 mock。
  - `siliconflow_file` → `SILICONFLOW_API_KEY` 非空 → `SiliconFlowFileAsrClient`,`base_url = SILICONFLOW_BASE_URL || "https://api.siliconflow.cn/v1"`,`model = SILICONFLOW_ASR_MODEL || "FunAudioLLM/SenseVoiceSmall"`;否则 mock。
  - 其他/未知 → mock。
- `resolve_asr_provider_name(session_id, env) -> &'static str`(测试辅助,构造离线、不联网)。
- `build_asr_client(session_id)` 改为 `resolve_asr_client(session_id, &current_env())`。env 只在 commands,不进适配器。
- 注:ASR 与 LLM 复用同一个 `SILICONFLOW_API_KEY`,但 ASR 用 `SILICONFLOW_ASR_MODEL`(区别于 LLM 的 `SILICONFLOW_LLM_MODEL`)。

## 数据流（文件式）

```
capture (16k/mono/i16) -> TranscriptionSession
  -> EnergyEndpointer.observe --(EndOfSpeech)--> endpoint.detected
  -> SiliconFlowFileAsrClient.push_frame  (缓冲, 无事件)
  -> (EndOfSpeech) client.finalize() -> WAV 编码 -> multipart POST -> {text} -> AsrEvent::Final
  -> events(): Receiver<AsrEvent>  (无 partial, 一段一个 final)
  -> ReplyTrigger(endpoint+final) -> LLM ...
```

## 测试与验证

确定性、无网络(经 mock transport):
- **WAV 编码器**:头魔数/字段/总长/首样本字节;空样本 → 44 字节头。
- **SiliconFlowFileAsrClient(mock TranscriptionTransport)**:推若干帧 + finalize → 收到一个 `Final`(文本=mock 返回值、时间戳取自帧);空缓冲 finalize → 无事件;transport 返回空串 → 无 Final;transport 返回 Err → 无 Final 且 `finalize` 返回 Ok(会话存活);finalize 后缓冲清空(再推+finalize 出新段)。
- **ASR resolver**(env map 注入):`siliconflow_file`+key → `"siliconflow-file-asr"`;缺 key → `"mock-asr"`;`openai_realtime`+key → OpenAI realtime 的 name();默认无 key → mock。更新现有 ASR 选择测试。
- **回归**:全量 `cargo test` 绿、`cargo check` 干净;前端 `npm test` 不受影响。
- **门控真实网络冒烟**(`#[ignore]`,需 `SILICONFLOW_API_KEY`):用 `ReqwestTranscriptionTransport` 真上传一小段 WAV,断言 HTTP 往返成功、返回 JSON 含 `text` 字段(合成音频文本可能为空,只验证往返与解析)。真实"语音→文本"质量由 `tauri dev` + 真实音频人工验证。

## 验收标准
- 文件式适配器在 `finalize` 时上传并发 `Final`,编排零改动。
- `ASR_PROVIDER=siliconflow_file` + key 后,真实转写经 `/audio/transcriptions` 返回(门控冒烟 + tauri dev 人工验证)。
- 转写失败不终止会话;key 不出现在错误/日志。
- 适配器不读 env;config 为结构体。
- WAV 编码正确(可被标准解析)。

## 后续跟进
- 百炼 fun-asr-realtime 流式 WebSocket ASR(同形于 OpenAI Realtime)。
- 超长句强制分段上限。
- 设置面板:provider/key/model 录入 + 安全存储。
