# Respondent

Respondent 是一个 Windows 优先的低延迟会议悬浮助手。它通过 WASAPI loopback 采集系统输出音频，也就是会议里播放到耳机或扬声器的对方声音，实时显示字幕，并在对方完成一个语义片段后流式生成可直接使用的建议回复。

项目当前是 Tauri + React + Rust 架构：

- 前端：React、TypeScript、Vite
- 桌面壳：Tauri 2
- 原生能力：Rust WASAPI loopback、SQLite、ASR/LLM provider 适配器
- 测试：Vitest、Testing Library、Rust integration tests

## 功能概览

- Windows 系统输出音频采集，不走麦克风采集路径
- 实时字幕区，展示 ASR partial/final 文本
- 端点触发的流式 AI 建议回复
- 一键复制当前建议回复
- 可折叠会话历史
- 会话文本本地 SQLite 持久化
- Markdown / 纯文本导出命令
- 应用内供应商配置菜单，支持 LLM 和 ASR provider/API Key/model/base URL 配置

## 开发环境

需要安装：

- Node.js
- npm
- Rust stable toolchain
- Windows 上的 Visual Studio Build Tools C++ 工具链

快速检查：

```powershell
node --version
npm --version
rustc --version
cargo --version
```

## 安装依赖

```powershell
npm install
```

Rust 依赖会在第一次运行 `cargo` 或 `tauri` 命令时自动下载。

## 运行

只运行前端开发服务器：

```powershell
npm run dev
```

运行 Tauri 桌面应用：

```powershell
npm run tauri:dev
```

构建前端：

```powershell
npm run build
```

构建桌面应用：

```powershell
npm run tauri:build
```

## Provider 配置

桌面应用顶部工具栏有供应商配置入口，可同时配置 LLM 和 ASR。手动配置会保存到应用数据目录的 `provider-profiles.json`（仅元数据），启动 native session 时优先使用手动配置；手动配置不完整时回退到环境变量；仍不可用时，**调试构建**或显式设置 `RESPONDENT_ALLOW_MOCK=1` 才允许演示 mock；**发布构建默认阻断隐式 mock**。

API Key 不会在配置摘要中回显，也不会写入前端 state 的加载结果。密钥通过系统凭据库保存（Windows Credential Manager / macOS Keychain / Linux Secret Service，`keyring` 已启用 `windows-native` 等平台 feature）；JSON 文件只保留 profile 元数据。若凭据库探针失败，不会剥离 JSON 中的历史明文密钥。首次加载在探针通过后会自动迁移。

### LLM provider

LLM 用于根据转写文本生成建议回复。

| Provider | 环境变量 | 默认值 |
| --- | --- | --- |
| OpenAI Responses | `LLM_PROVIDER=openai`, `OPENAI_API_KEY`, `OPENAI_LLM_MODEL` | model 由 OpenAI adapter 默认值决定 |
| DashScope / 阿里百炼 | `LLM_PROVIDER=dashscope`, `DASHSCOPE_API_KEY`, `DASHSCOPE_BASE_URL`, `DASHSCOPE_LLM_MODEL` | base URL `https://dashscope.aliyuncs.com/compatible-mode/v1`, model `qwen-plus` |
| Zhipu / Z.ai | `LLM_PROVIDER=zhipu`, `ZHIPU_API_KEY` 或 `ZAI_API_KEY`, `ZHIPU_BASE_URL`, `ZHIPU_LLM_MODEL` | base URL `https://open.bigmodel.cn/api/paas/v4`, model `glm-4-plus` |
| SiliconFlow | `LLM_PROVIDER=siliconflow`, `SILICONFLOW_API_KEY`, `SILICONFLOW_BASE_URL`, `SILICONFLOW_LLM_MODEL` | base URL `https://api.siliconflow.cn/v1`, model `Qwen/Qwen3-8B` |
| OpenAI Compatible | `LLM_PROVIDER=openai_compatible`, `OPENAI_COMPATIBLE_API_KEY`, `OPENAI_COMPATIBLE_BASE_URL`, `OPENAI_COMPATIBLE_MODEL` | 三项均需配置 |

### ASR provider

ASR 用于把会议音频转成实时字幕。

| Provider | 环境变量 | 默认值 |
| --- | --- | --- |
| OpenAI Realtime | `ASR_PROVIDER=openai_realtime`, `OPENAI_API_KEY`, `OPENAI_ASR_MODEL` | model `gpt-realtime-whisper` |
| DashScope Realtime / 百炼实时 ASR | `ASR_PROVIDER=bailian_realtime`, `DASHSCOPE_API_KEY`, `DASHSCOPE_ASR_MODEL`, `DASHSCOPE_ASR_LANGUAGE_HINT`, `DASHSCOPE_ASR_MAX_SENTENCE_SILENCE_MS`, `DASHSCOPE_ASR_HEARTBEAT` | model `fun-asr-realtime` |
| SiliconFlow File ASR | `ASR_PROVIDER=siliconflow_file`, `SILICONFLOW_API_KEY`, `SILICONFLOW_BASE_URL`, `SILICONFLOW_ASR_MODEL` | base URL `https://api.siliconflow.cn/v1`, model `FunAudioLLM/SenseVoiceSmall` |

## 测试

前端测试：

```powershell
npm test
```

Rust 测试：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

前端类型检查和构建：

```powershell
npm run build
```

Rust 格式化：

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml
```

部分真实网络或真实音频测试带有 `#[ignore]`，需要手动提供 API Key 或系统音频环境后再单独运行。

## 目录结构

```text
.
├── docs/                       # 设计、计划和验证文档
├── src/                        # React 前端
│   ├── domain/                 # 前端领域逻辑
│   ├── services/               # Tauri API、mock realtime、事件桥
│   └── state/                  # 会话状态 reducer/store
├── src-tauri/
│   ├── src/
│   │   ├── asr/                # ASR provider 和转写会话
│   │   ├── audio/              # WASAPI loopback、音频格式和转换
│   │   ├── llm/                # LLM provider、流式回复和触发策略
│   │   ├── session/            # SQLite 会话存储和导出
│   │   ├── commands.rs         # Tauri commands 和 runtime 编排
│   │   └── provider_config.rs  # Provider 配置读写与摘要
│   └── tests/                  # Rust integration tests
└── package.json
```

## 隐私和限制

- 应用目标是采集系统输出音频，不主动采集麦克风。
- 如果用户启用了麦克风监听回放、立体声混音或把自己的声音路由到系统输出，应用仍可能捕获这些回放内容。
- 默认只保存文本和元数据，不保存原始音频。
- 云端 ASR/LLM provider 会收到音频或文本内容，使用前请确认会议隐私要求和 provider 合规要求。

## 当前状态

这是一个低延迟会议助手 MVP。核心链路已经包括音频采集、ASR、端点触发、LLM 流式建议、会话持久化和 provider 配置菜单。后续可以继续补强设备切换恢复、系统凭据管理器、更多导出入口、成本统计和更细的延迟观测。
