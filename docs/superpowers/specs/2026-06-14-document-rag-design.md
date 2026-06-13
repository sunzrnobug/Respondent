# 文档知识库 RAG 设计

日期：2026-06-14

## 目标

为会议助手增加"文档知识库"能力：用户会前上传 Markdown 技术文档，LLM 在生成建议回复时自动检索并引用相关内容，使回复更贴合具体产品或技术背景。

## 场景约束

- 文档格式：Markdown（`.md`）
- 单文档长度：约 15,000 字符
- 文档数量上限：10 篇
- 总语料：约 150,000 字符（中英混合技术文档）
- 检索时机：每次 `ReplyTrigger` 触发后，构建 `ReplyRequest` 前
- 检索延迟目标：< 5ms（纯内存 BM25，不调外部 API）

## 方案选型

| 方案 | 是否可行 | 原因 |
|---|---|---|
| 全文注入 | 否 | 10 篇 × ~75k tokens 远超上下文限制 |
| 会前摘要 | 否 | 技术文档细节（接口、参数、代码）压缩后损失过大 |
| BM25 关键词检索 | 是 | 技术术语精确匹配效果好，无需 embedding API，延迟极低 |
| Embedding 向量检索 | 备选 | 语义更强，但每次查询需额外 API 调用，增加延迟 |

**选定方案**：BM25 关键词检索。如后期发现语义相关但词不重叠的场景频繁，可在此基础上叠加 embedding 重排序。

## 架构

```
用户上传 .md 文件
     ↓
前端读取文件内容（File API）
     ↓ Tauri 命令 load_document(name, content)
Rust: Markdown 解析 → 分块 → BM25 索引更新（内存）
                                    ↓
            ReplyTrigger 触发（Final ASR + Endpoint）
                                    ↓
            document_store.query(transcript, top_k=3)
                                    ↓
                     top-3 chunks → document_context: Option<String>
                                    ↓
                    build_chat_body 将 document_context 插入 prompt
```

### 新增模块

| 路径 | 职责 |
|---|---|
| `src-tauri/src/docs/mod.rs` | 对外暴露 `DocumentStore`、`DocumentSummary` |
| `src-tauri/src/docs/chunk.rs` | Markdown 解析与分块逻辑 |
| `src-tauri/src/docs/bm25.rs` | BM25 打分与检索 |
| `src-tauri/src/docs/store.rs` | 文档注册/注销，持有 BM25 索引 |

### 改动现有文件

| 文件 | 改动 |
|---|---|
| `src-tauri/src/llm/client.rs` | `ReplyRequest` 加 `document_context: Option<String>` |
| `src-tauri/src/llm/reply_trigger.rs` | 构建 request 时查询 `DocumentStore` |
| `src-tauri/src/llm/openai_compatible.rs` | prompt 中插入 document_context |
| `src-tauri/src/llm/openai_responses.rs` | 同步插入 document_context（若有相同 prompt 构建逻辑） |
| `src-tauri/src/commands.rs` | 注册新 Tauri 命令 |
| `src-tauri/src/lib.rs` | 注入 `DocumentStore` 到 Tauri state |
| `src/App.tsx` | 文档管理面板 UI |
| `src/services/tauriApi.ts` | 新增前端 API 调用封装 |

## 分块策略

### 目标

- 每块保留完整语义单元（标题 + 所属段落）
- 每块携带标题路径，确保检索结果有上下文
- 块大小控制在 500–800 字符，便于注入 prompt

### 算法

```
1. 按行扫描 Markdown
2. 遇到 ## 或 ### 标题：结束当前 chunk，开新 chunk，更新标题路径
3. 遇到 # 标题：更新 H1 标题记录，不单独成块（H1 通常是文件标题）
4. 累积行到当前 chunk
5. 若当前 chunk 超过 800 字符且遇到空行：在此处切分
6. 结束时收尾最后一个 chunk
7. 过滤掉正文少于 50 字符的 chunk（纯标题行等噪声）
```

### Chunk 数据结构

```rust
pub struct Chunk {
    pub doc_name: String,      // 来源文件名
    pub heading_path: String,  // "安装指南 > 环境配置 > Python 依赖"
    pub text: String,          // 正文内容（不含标题行本身）
}
```

**估算**：15,000 字符 / 平均 600 字符 ≈ 25 块/篇，10 篇共约 250 块，内存约 1.5MB。

## BM25 检索

### 参数

- `k1 = 1.5`（词频饱和因子）
- `b = 0.75`（文档长度归一化因子）

### 分词规则

- 中文：每个汉字作为独立 token
- 英文 / 代码标识符：按空白和标点切分，转小写
- 混合文档直接支持，无需语言检测

### 检索流程

```
query = tokenize(transcript)
for each chunk in all_chunks:
    score = bm25_score(query, chunk.text)
results = top_k(chunks where score > 0, k=3)
```

### DocumentStore 接口

```rust
pub struct DocumentStore {
    chunks: Vec<Chunk>,
    // 预计算的 IDF 和文档统计，随 chunks 更新
    idf: HashMap<String, f32>,
    avg_doc_len: f32,
}

impl DocumentStore {
    pub fn load(&mut self, name: String, content: String);
    pub fn unload(&mut self, name: &str);
    pub fn list(&self) -> Vec<DocumentSummary>;
    pub fn query(&self, text: &str, top_k: usize) -> Vec<&Chunk>;
    pub fn is_empty(&self) -> bool;
}

pub struct DocumentSummary {
    pub name: String,
    pub chunk_count: usize,
}
```

## LLM Prompt 集成

### ReplyRequest 变更

```rust
pub struct ReplyRequest {
    pub session_id: String,
    pub generation_id: String,
    pub transcript: String,
    pub context: Vec<String>,
    pub document_context: Option<String>,  // 新增
}
```

### Prompt 结构（有文档时）

```
[system]
You are a live meeting assistant. Suggest one concise, useful reply the user could say next. Keep it natural, specific, and short.

Reference documents (use these to inform your reply if relevant):
---
{heading_path_1}:
{chunk_text_1}
---
{heading_path_2}:
{chunk_text_2}
---
{heading_path_3}:
{chunk_text_3}
---

[user]
Conversation context:
{最近 6 轮对话}

Current turn:
{transcript}

Write the suggested reply only.
```

无文档时 prompt 保持原样不变，兼容现有行为。

### document_context 拼接格式

```rust
fn format_document_context(chunks: &[&Chunk]) -> String {
    chunks.iter().map(|c| {
        format!("{}:\n{}", c.heading_path, c.text)
    }).collect::<Vec<_>>().join("\n---\n")
}
```

## Tauri 命令

```rust
#[tauri::command]
fn load_document(
    state: State<Mutex<DocumentStore>>,
    name: String,
    content: String,
) -> DocumentSummary

#[tauri::command]
fn unload_document(
    state: State<Mutex<DocumentStore>>,
    name: String,
)

#[tauri::command]
fn list_documents(
    state: State<Mutex<DocumentStore>>,
) -> Vec<DocumentSummary>
```

## 前端文档管理 UI

### 入口

TopBar 新增 `FileText` 图标按钮，行为与现有 `Settings`、`History` 按钮一致：
- Tauri 环境：`openDialogWindow("documents")`
- 浏览器环境：切换内联 modal

### Documents Panel

```
Documents                                [×]
3 documents loaded · 72 chunks
──────────────────────────────────────────
📄 api-reference.md        28 chunks  [×]
📄 installation.md         21 chunks  [×]
📄 architecture.md         23 chunks  [×]
──────────────────────────────────────────
[+ Upload .md file]
```

### 数据持久化

- 文档内容存 localStorage，key：`respondent.documents`
- 格式：`Array<{ name: string; content: string }>`
- App 启动时读取，逐一调 `load_document` 恢复索引
- 10 篇 × 15k 字符 = 150k 字符，远小于 localStorage 5MB 限制

## ReplyTrigger 集成

`ReplyTrigger` 需持有 `DocumentStore` 的共享引用，在 `observe` 产生 `ReplyRequest` 时同步查询：

```rust
// ReplyTrigger::observe 伪代码
if self.endpoint_armed {
    let document_context = self.doc_store
        .lock()
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| {
            let chunks = s.query(&text, 3);
            format_document_context(&chunks)
        });

    Some(ReplyRequest {
        document_context,
        ..
    })
}
```

## 测试策略

### 单元测试

- `chunk.rs`：不同 Markdown 结构的分块正确性（多级标题、长段落、代码块、空文档）
- `bm25.rs`：已知 query/corpus 的分数正确性；空 query 返回空结果
- `store.rs`：load/unload 后 list 和 query 的行为

### 集成测试

- 加载 2 篇文档，触发 reply，验证 prompt 中包含相关 chunk
- 无文档时 prompt 不含 document_context 段落
- 卸载所有文档后 query 返回空

### 手动验证

- 上传 api-reference.md，对话中提到 API 认证，验证建议回复引用了文档中的认证说明
- 上传 10 篇文档，检索延迟 < 5ms（日志打点）

## MVP 范围

**包含**：
- Markdown 文档上传、分块、BM25 检索
- 检索结果注入 LLM prompt
- 文档管理面板（上传、列表、删除）
- localStorage 持久化

**不包含**：
- PDF / Word 支持
- Embedding 向量检索
- 跨会话文档分析或摘要
- 文档内容预览
