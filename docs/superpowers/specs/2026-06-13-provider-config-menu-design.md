# Provider 配置菜单设计

## 背景

当前项目已经支持多家 LLM 和 ASR provider,但配置入口主要是环境变量。LLM 回复建议由 `LLM_PROVIDER` 及各 provider key/model/base URL 决定;ASR 字幕识别由 `ASR_PROVIDER` 及 OpenAI、DashScope、SiliconFlow 相关环境变量决定。桌面用户需要在应用内手动配置 provider 和 API Key,不再依赖启动前设置环境变量。

本轮采用“前端菜单 + 后端结构化配置文件”的方案。API Key 可以通过菜单录入和保存,但 UI 不回显密钥原文,后端日志和错误信息不输出密钥。后续可把密钥存储替换为系统凭据管理器,本轮先打通产品流程。

## 目标

- 在主界面提供一个供应商配置菜单,同时配置 LLM 和 ASR。
- 后端提供读取、保存、清除配置的 Tauri 命令。
- 启动 native session 时优先使用手动配置;手动配置缺失时回退到现有环境变量;仍缺失时继续使用 mock provider。
- 保留现有 env 驱动路径,不破坏测试和命令行开发体验。
- API Key 不通过配置摘要、日志、错误消息或前端再次加载泄露。

## 非目标

- 不在本轮接入系统安全凭据管理器。
- 不新增 ASR provider 或 LLM provider。
- 不改变音频捕获、端点检测、会话持久化或流式事件协议。
- 不把 ASR OpenAI Realtime 的 language/delay 暴露为菜单项,因为当前 resolver 尚未从 env 支持这些字段。

## 前端交互

主界面顶部操作区新增设置按钮,点击后展开或弹出“供应商配置”面板。面板包含两组配置。

LLM 配置字段:
- Provider: `openai`, `dashscope`, `zhipu`, `siliconflow`, `openai_compatible`
- API Key: 密码输入框,加载已保存配置时显示为空,旁边用状态文案表示已配置
- Base URL: 对 OpenAI Compatible 必填;对 DashScope、Zhipu、SiliconFlow 可选并预填默认值
- Model: 可编辑,按 provider 使用现有默认值预填

ASR 配置字段:
- Provider: `openai_realtime`, `bailian_realtime`, `siliconflow_file`
- API Key: 密码输入框,加载已保存配置时显示为空,旁边显示已配置状态
- Base URL: 仅 SiliconFlow File 使用,默认 `https://api.siliconflow.cn/v1`
- Model: 可编辑,按 provider 使用现有默认值预填
- DashScope 额外字段: language hint、max sentence silence ms、heartbeat

保存后面板显示成功状态。清除按钮删除手动配置,下一次启动 session 将只使用 env/mock 回退。若用户不重新输入 API Key 就保存其他字段,后端保留已有密钥;若清除整组配置,对应密钥也删除。

## 后端配置模型

新增 provider 配置模块,负责 app data 目录下的结构化配置文件读写。建议文件名为 `provider-config.json`。配置按 LLM 和 ASR 分组:

```text
ProviderSettings
  llm: Option<LlmProviderSettings>
  asr: Option<AsrProviderSettings>
```

保存命令接收完整表单数据,并支持“API Key 未提供则保留旧值”的语义。读取命令返回给前端的摘要只包含 provider、base URL、model、可选参数和 `hasApiKey`,不返回 `apiKey` 原文。

Tauri 命令:
- `get_provider_config() -> ProviderConfigSummary`
- `save_provider_config(payload) -> ProviderConfigSummary`
- `clear_provider_config(scope) -> ProviderConfigSummary`

命令注册在 `lib.rs` 的 invoke handler 中。前端 API 包装放在 `src/services/tauriApi.ts`。

## Resolver 接入

现有 `resolve_reply_client(env)` 和 `resolve_asr_client(session_id, env)` 保留,并新增显式配置优先的 resolver:

- `resolve_reply_client_with_settings(env, settings)`
- `resolve_asr_client_with_settings(session_id, env, settings)`

`SessionRuntime::start` 读取保存的 provider settings,传入 resolver。若手动配置完整,使用手动配置;若对应 provider 缺少必填 API Key/base URL/model,回退到 env;若 env 也不可用,使用 mock provider。回退行为继续通过现有 `system.status` 提示用户。

## 默认值

默认值沿用现有后端实现:

- LLM OpenAI: `OPENAI_LLM_MODEL` 或 OpenAI adapter 默认值
- LLM DashScope: base URL `https://dashscope.aliyuncs.com/compatible-mode/v1`, model `qwen-plus`
- LLM Zhipu: base URL `https://open.bigmodel.cn/api/paas/v4`, model `glm-4-plus`
- LLM SiliconFlow: base URL `https://api.siliconflow.cn/v1`, model `Qwen/Qwen3-8B`
- ASR OpenAI Realtime: model `gpt-realtime-whisper`
- ASR DashScope Realtime: model `fun-asr-realtime`
- ASR SiliconFlow File: base URL `https://api.siliconflow.cn/v1`, model `FunAudioLLM/SenseVoiceSmall`

## 错误处理和安全

- 保存时修剪空白;空字符串视为未提供。
- 必填字段缺失不阻塞保存,但启动 session 时按 provider 完整性决定是否可用。
- 配置摘要和前端 state 不保存 API Key 原文。
- `Debug` 输出、错误消息、系统状态消息不包含 API Key。
- JSON 配置文件属于本轮折中方案,文档明确标注后续可迁移到系统凭据管理器。

## 测试

Rust 测试:
- 配置保存/读取摘要不返回 API Key。
- 保存不含 API Key 的更新会保留旧密钥。
- LLM 手动配置优先于 env。
- ASR 手动配置优先于 env。
- 手动配置不完整时回退 env/mock。

前端测试:
- 设置按钮能打开供应商配置面板。
- 切换 LLM/ASR provider 会显示对应字段和默认值。
- 保存会调用 Tauri API,并显示已配置状态。
- 清除配置会调用 Tauri API,并重置状态。

验证命令:
- `npm test`
- `cargo test --manifest-path src-tauri/Cargo.toml`
