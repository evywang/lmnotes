# M1b: LLM 智能层 + 建议中心 实现计划（详细版）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 在 M1a 索引层之上接入 LLM：实现 `LlmProvider` 抽象（按能力拆 trait，ADR-0005 F7）、内置 Ollama（chat/embed）与 OpenAI 兼容 Provider、任务路由、三层隐私护栏、后台索引器接 LLM 生成摘要/标签建议、建议中心 UI（审阅/接受/拒绝/批量）、就地改写 + 撤销。向量层（embed 写入 sqlite-vec）在此里程碑完成。

**范围边界（评审 R7）：** FR-LLM-01 含"链接建议"，但链接建议需 `vector_search` 取相似 top-5 + LLM 关联判断，属 M1c RAG 范畴。**M1b 只做摘要 + 标签建议 + embed 写入；链接建议明确推迟到 M1c。** M1b 的 DoD 不含链接建议。

**依赖：** M1a 已完成（index/indexer/search/Tauri 壳，58 测试绿）。

**Architecture:**
- `lmnotes-core/src/llm/`：Provider trait（LlmProvider + ChatCap/EmbedCap）、Ollama/OpenAI 实现、路由、护栏、建议队列。
- `lmnotes-core/src/indexer/`：扩展 `generate_suggestions`（独立异步函数，不污染 `index_concept` 的同步路径）。
- `lmnotes-core/src/backend/sqlite.rs`：扩 vector 读写（`upsert_vector`/`vector_search`）。
- `apps/desktop/src/suggestions/`：建议中心 UI。
- `apps/desktop/src/editor/`：就地改写菜单。
- `apps/desktop/src/settings/`：Provider 配置。

**Tech Stack:** Rust：`reqwest` 0.12（json+stream feature）、`bitflags` 2.13、`futures-util` 0.3（M1a 已有）、`serde_json`（workspace）。测试：`wiremock` 0.6（OpenAI/Ollama HTTP mock）。前端：SolidJS + Tauri events（流式）。

**关键设计决策（执行时勿偏离）：**
- M1a 的 `IndexBackend` 查询方法同步、写入方法异步。LLM 建议生成是 **纯异步独立函数** `generate_suggestions(indexer, registry, routing, guard, concept)`，由 `save_concept` 命令在索引完成后 spawn 调用，不阻塞编辑器保存。
- Provider 注册用 **双 map 方案**（ADR-0005 F7）：Registry 同时存 `Arc<dyn LlmProvider>` 与 `Arc<dyn ChatCap>`/`Arc<dyn EmbedCap>`，避免 Any downcast。
- 向量层在此里程碑真正填充（M1a 只建了空 vec 表）。

---

## File Structure

```
lmnotes/crates/lmnotes-core/src/
├── llm/
│   ├── mod.rs              # [T1] 模块入口
│   ├── provider.rs         # [T1] LlmProvider + ChatCap/EmbedCap trait + 共享类型
│   ├── ollama.rs           # [T2] Ollama chat(NDJSON 流)+embed
│   ├── openai.rs           # [T3] OpenAI 兼容 chat(SSE 流)+embed
│   ├── routing.rs          # [T4] Registry + Routing + 降级
│   ├── guard.rs            # [T5] 三层护栏 + dispatch 入口
│   └── suggestion.rs       # [T6] 建议类型 + SuggestionStore trait
├── backend/
│   └── sqlite.rs           # [T7] 扩 upsert_vector/vector_search
├── indexer/
│   └── mod.rs              # [T7] 扩 generate_suggestions
apps/desktop/src/
├── suggestions/
│   └── SuggestionCenter.tsx # [T8]
├── editor/
│   └── RewriteMenu.tsx      # [T9]
├── settings/
│   └── ProviderSettings.tsx # [T10]
└── store/
    └── llm.ts               # [T8] LLM/建议状态
apps/desktop/src-tauri/src/
├── commands.rs              # [T8/T9/T10] 加 suggestion/rewrite/config 命令
└── lib.rs                   # [T10] 首启探测 + Registry 构建
```

---

## Task 1: LlmProvider trait + 共享类型

**Files:**
- Create: `crates/lmnotes-core/src/llm/mod.rs`
- Create: `crates/lmnotes-core/src/llm/provider.rs`
- Modify: `crates/lmnotes-core/src/lib.rs`（加 `pub mod llm;`）
- Modify: `crates/lmnotes-core/Cargo.toml`（加 reqwest/bitflags/futures-util）
- Modify: `crates/lmnotes-core/src/error.rs`（加 Reqwest 变体）

- [ ] **Step 1: 加依赖**

`crates/lmnotes-core/Cargo.toml` `[dependencies]` 追加：
```toml
reqwest = { version = "0.12", features = ["json", "stream"] }
bitflags = "2.13"
futures-util = "0.3"
```
`[dev-dependencies]` 追加：
```toml
wiremock = "0.6"
```

- [ ] **Step 2: error.rs 加 Reqwest 变体**

```rust
#[error("HTTP error: {0}")]
Http(#[from] reqwest::Error),
```

- [ ] **Step 3: provider.rs（trait 定义 + 共享类型）**

```rust
//! LLM Provider 抽象（ADR-0005）。按能力拆 trait（F7）。

use crate::Result;
use async_trait::async_trait;
use futures_util::Stream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Local,
    Cloud,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Capabilities: u8 {
        const CHAT = 1 << 0;
        const EMBED = 1 << 1;
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub temperature: Option<f32>,
}

/// 所有 Provider 必须实现。
#[async_trait]
pub trait LlmProvider: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> ProviderKind;
    fn capabilities(&self) -> Capabilities;
    async fn health(&self) -> Result<bool>;
}

/// chat 能力 trait（按需实现）。
#[async_trait]
pub trait ChatCap: LlmProvider {
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin>>;

    /// 非流式（聚合 stream）。
    async fn chat(&self, req: ChatRequest) -> Result<String> {
        let mut stream = self.chat_stream(req).await?;
        let mut out = String::new();
        while let Some(chunk) = futures_util::StreamExt::next(&mut stream).await {
            out.push_str(&chunk?);
        }
        Ok(out)
    }
}

/// embed 能力 trait。
#[async_trait]
pub trait EmbedCap: LlmProvider {
    async fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}
```

- [ ] **Step 4: mod.rs + 占位**

```rust
//! LLM Provider 抽象 + 路由 + 护栏（ADR-0005）。

pub mod provider;
pub mod ollama;       // T2
pub mod openai;       // T3
pub mod routing;      // T4
pub mod guard;        // T5
pub mod suggestion;   // T6

pub use provider::*;
```

占位文件（`// T? 实现`）：`ollama.rs`/`openai.rs`/`routing.rs`/`guard.rs`/`suggestion.rs`。

- [ ] **Step 5: 验证编译**

Run: `cargo check -p lmnotes-core`
Expected: 通过（trait 定义完整，实现待 T2+）。

- [ ] **Step 6: Commit**

```bash
git add crates/lmnotes-core/
git commit -m "feat(llm): LlmProvider trait with capability split (ChatCap/EmbedCap)"
```

---

## Task 2: Ollama Provider（NDJSON 流式 chat + embed）

**Files:**
- Create: `crates/lmnotes-core/src/llm/ollama.rs`

**目标：** 对接 Ollama `/api/chat`（stream=true，NDJSON）+ `/api/embeddings`。健康检查 `/api/tags`。流式解析：每行一个 JSON `{"message":{"content":"..."}}`，遇 `"done":true` 结束。

- [ ] **Step 1: 写实现 + 单元测试（URL/能力/构造）**

`ollama.rs`：
```rust
//! Ollama 本地 Provider（默认 http://localhost:11434）。

use super::provider::*;
use crate::Result;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OllamaProvider {
    base_url: String,
    client: Client,
}

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), client: Client::new() }
    }
    pub fn default_local() -> Self {
        Self::new("http://localhost:11434")
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn id(&self) -> &str { "ollama" }
    fn kind(&self) -> ProviderKind { ProviderKind::Local }
    fn capabilities(&self) -> Capabilities { Capabilities::CHAT | Capabilities::EMBED }
    async fn health(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.base_url);
        Ok(self.client.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false))
    }
}

#[derive(Serialize)]
struct ChatBody { model: String, messages: Vec<MsgSer>, stream: bool }
#[derive(Serialize)]
struct MsgSer { role: String, content: String }
#[derive(Deserialize)]
struct ChatChunk { message: Option<MsgDe>, done: Option<bool> }
#[derive(Deserialize)]
struct MsgDe { content: String }

#[async_trait]
impl ChatCap for OllamaProvider {
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<Box<dyn futures_util::Stream<Item = Result<String>> + Send + Unpin>> {
        let url = format!("{}/api/chat", self.base_url);
        let body = ChatBody {
            model: req.model,
            messages: req.messages.into_iter().map(|m| MsgSer {
                role: match m.role {
                    ChatRole::System => "system".into(),
                    ChatRole::User => "user".into(),
                    ChatRole::Assistant => "assistant".into(),
                },
                content: m.content,
            }).collect(),
            stream: true,
        };
        let resp = self.client.post(&url).json(&body).send().await?;
        let byte_stream = resp.bytes_stream();
        // NDJSON 按行解析
        let stream = futures_util::stream::unfold(
            (byte_stream, String::new()),
            |(mut bytes, mut buf)| async move {
                loop {
                    // 先尝试从 buf 取一行
                    if let Some(idx) = buf.find('\n') {
                        let line: String = buf.drain(..=idx).collect();
                        if let Ok(c) = serde_json::from_str::<ChatChunk>(line.trim()) {
                            if let Some(m) = c.message {
                                return Some((Ok(m.content), (bytes, buf)));
                            }
                            if c.done.unwrap_or(false) {
                                return None;
                            }
                        }
                        continue;
                    }
                    // buf 无完整行，从 byte stream 拉取
                    match futures_util::StreamExt::next(&mut bytes).await {
                        Some(Ok(chunk)) => buf.push_str(&String::from_utf8_lossy(&chunk)),
                        Some(Err(e)) => return Some((Err(crate::CoreError::Io(e)), (bytes, buf))),
                        None => {
                            // 流尽，若 buf 还有内容尝试解析最后一行
                            if buf.trim().is_empty() { return None; }
                            let line = std::mem::take(&mut buf);
                            if let Ok(c) = serde_json::from_str::<ChatChunk>(line.trim()) {
                                if let Some(m) = c.message { return Some((Ok(m.content), (bytes, buf))); }
                            }
                            return None;
                        }
                    }
                }
            },
        ).boxed();
        Ok(Box::new(stream))
    }
}

#[derive(Serialize)]
struct EmbedBody { model: String, prompt: String }
#[derive(Deserialize)]
struct EmbedResp { embedding: Vec<f32> }

#[async_trait]
impl EmbedCap for OllamaProvider {
    async fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/api/embeddings", self.base_url);
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            let r: EmbedResp = self.client.post(&url)
                .json(&EmbedBody { model: model.into(), prompt: t.clone() })
                .send().await?.json().await?;
            out.push(r.embedding);
        }
        Ok(out)
    }
}

// boxed() 需要 StreamExt 的方法
use futures_util::StreamExt;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_is_local_with_both_caps() {
        let p = OllamaProvider::default_local();
        assert_eq!(p.kind(), ProviderKind::Local);
        assert!(p.capabilities().contains(Capabilities::CHAT | Capabilities::EMBED));
        assert_eq!(p.id(), "ollama");
    }
}
```

- [ ] **Step 2: wiremock 集成测试（NDJSON 流解析）**

在 `ollama.rs` 测试模块追加：
```rust
    #[tokio::test]
    async fn chat_stream_parses_ndjson() {
        use wiremock::{MockServer, Mock, ResponseTemplate};
        use wiremock::matchers::{method, path};
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200)
                .set_body_string(
                    "{\"message\":{\"content\":\"Hello\"},\"done\":false}\n\
                     {\"message\":{\"content\":\" world\"},\"done\":false}\n\
                     {\"message\":{},\"done\":true}\n"
                ))
            .mount(&server).await;
        let p = OllamaProvider::new(server.uri());
        let req = ChatRequest {
            model: "x".into(),
            messages: vec![ChatMessage { role: ChatRole::User, content: "hi".into() }],
            temperature: None,
        };
        let mut s = p.chat_stream(req).await.unwrap();
        let mut out = String::new();
        while let Some(chunk) = futures_util::StreamExt::next(&mut s).await {
            out.push_str(&chunk.unwrap());
        }
        assert_eq!(out, "Hello world");
    }
```

- [ ] **Step 3: 跑测试**

Run: `cargo test -p lmnotes-core llm::ollama`
Expected: 2 测试 PASS（构造 + NDJSON 流解析）。

- [ ] **Step 4: Commit**

```bash
git add crates/lmnotes-core/
git commit -m "feat(llm): Ollama provider (NDJSON streaming chat + embeddings)"
```

---

## Task 3: OpenAI 兼容 Provider（SSE 流 + wiremock）

**Files:**
- Create: `crates/lmnotes-core/src/llm/openai.rs`

**目标：** 任意 OpenAI 兼容端点（GLM/OpenAI/自建）。`POST /v1/chat/completions`（stream=true，SSE `data: {...}` 行，遇 `data: [DONE]` 结束）+ `POST /v1/embeddings`。Authorization Bearer。解析 SSE：每行 `data: <json>`，json 中 `choices[0].delta.content`。

- [ ] **Step 1: 实现**

`openai.rs`（结构与 ollama 类似，差异：SSE 解析、Authorization header、`choices[0].delta.content`）：
```rust
//! OpenAI 兼容 Provider（GLM/OpenAI/自建）。

use super::provider::*;
use crate::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OpenAiProvider {
    id: String,
    base_url: String,
    api_key: String,
    client: Client,
}

impl OpenAiProvider {
    /// id 用于 Registry 区分多个 OpenAI 兼容端点（如 "glm"、"openai"）。
    pub fn new(id: impl Into<String>, base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            base_url: base_url.into(),
            api_key: api_key.into(),
            client: Client::new(),
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn id(&self) -> &str { &self.id }
    fn kind(&self) -> ProviderKind { ProviderKind::Cloud }
    fn capabilities(&self) -> Capabilities { Capabilities::CHAT | Capabilities::EMBED }
    async fn health(&self) -> Result<bool> {
        // GET /v1/models（轻量探活）
        let url = format!("{}/v1/models", self.base_url);
        let r = self.client.get(&url)
            .bearer_auth(&self.api_key).send().await;
        Ok(r.map(|x| x.status().is_success()).unwrap_or(false))
    }
}

#[derive(Serialize)]
struct ChatBody { model: String, messages: Vec<MsgSer>, stream: bool, #[serde(skip_serializing_if = "Option::is_none")] temperature: Option<f32> }
#[derive(Serialize)]
struct MsgSer { role: String, content: String }
#[derive(Deserialize)]
struct ChatChunk { choices: Vec<ChoiceDe> }
#[derive(Deserialize)]
struct ChoiceDe { delta: DeltaDe }
#[derive(Deserialize)]
struct DeltaDe { #[serde(default)] content: Option<String> }

#[async_trait]
impl ChatCap for OpenAiProvider {
    async fn chat_stream(&self, req: ChatRequest) -> Result<Box<dyn futures_util::Stream<Item = Result<String>> + Send + Unpin>> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let body = ChatBody {
            model: req.model,
            messages: req.messages.into_iter().map(|m| MsgSer {
                role: match m.role { ChatRole::System => "system".into(), ChatRole::User => "user".into(), ChatRole::Assistant => "assistant".into() },
                content: m.content,
            }).collect(),
            stream: true,
            temperature: req.temperature,
        };
        let resp = self.client.post(&url).bearer_auth(&self.api_key).json(&body).send().await?;
        let byte_stream = resp.bytes_stream();
        // SSE 解析：每行 `data: <json>` 或 `data: [DONE]`
        let stream = futures_util::stream::unfold(
            (byte_stream, String::new()),
            |(mut bytes, mut buf)| async move {
                loop {
                    if let Some(idx) = buf.find('\n') {
                        let line: String = buf.drain(..=idx).collect();
                        let line = line.trim();
                        if line == "data: [DONE]" { return None; }
                        if let Some(json) = line.strip_prefix("data: ") {
                            if let Ok(c) = serde_json::from_str::<ChatChunk>(json) {
                                if let Some(content) = c.choices.into_iter().next().and_then(|ch| ch.delta.content) {
                                    return Some((Ok(content), (bytes, buf)));
                                }
                            }
                        }
                        continue;
                    }
                    match bytes.next().await {
                        Some(Ok(chunk)) => buf.push_str(&String::from_utf8_lossy(&chunk)),
                        Some(Err(e)) => return Some((Err(crate::CoreError::Io(e)), (bytes, buf))),
                        None => return None,
                    }
                }
            },
        ).boxed();
        Ok(Box::new(stream))
    }
}

#[derive(Serialize)]
struct EmbedBody { model: String, input: Vec<String> }
#[derive(Deserialize)]
struct EmbedResp { data: Vec<EmbedItem> }
#[derive(Deserialize)]
struct EmbedItem { embedding: Vec<f32> }

#[async_trait]
impl EmbedCap for OpenAiProvider {
    async fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/v1/embeddings", self.base_url);
        let r: EmbedResp = self.client.post(&url).bearer_auth(&self.api_key)
            .json(&EmbedBody { model: model.into(), input: texts.to_vec() })
            .send().await?.json().await?;
        Ok(r.data.into_iter().map(|i| i.embedding).collect())
    }
}
```

- [ ] **Step 2: wiremock 测试（SSE 流）**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path, header};

    #[tokio::test]
    async fn chat_stream_parses_sse() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n\
                 data: {\"choices\":[{\"delta\":{\"content\":\" there\"}}]}\n\n\
                 data: [DONE]\n\n"
            ))
            .mount(&server).await;
        let p = OpenAiProvider::new("test", server.uri(), "test-key");
        let req = ChatRequest { model: "x".into(), messages: vec![ChatMessage { role: ChatRole::User, content: "hi".into() }], temperature: None };
        let mut s = p.chat_stream(req).await.unwrap();
        let mut out = String::new();
        while let Some(c) = s.next().await { out.push_str(&c.unwrap()); }
        assert_eq!(out, "Hi there");
    }

    #[test]
    fn is_cloud_kind() {
        let p = OpenAiProvider::new("glm", "https://open.bigmodel.cn/api", "k");
        assert_eq!(p.kind(), ProviderKind::Cloud);
        assert_eq!(p.id(), "glm");
    }
}
```

- [ ] **Step 3: 跑测试 + Commit**

Run: `cargo test -p lmnotes-core llm::openai`
Expected: 2 测试 PASS。

```bash
git commit -m "feat(llm): OpenAI-compatible provider (SSE streaming + embeddings)"
```

---

## Task 4: Registry + Routing（双 map + 降级）

**Files:**
- Create: `crates/lmnotes-core/src/llm/routing.rs`

**目标：** Registry 双 map（providers + chats + embeds），Routing 按任务槽位指向 Provider+模型，`chat_for`/`embed_for` 能力探测 + 缺能力时按 Routing 次序降级。

- [ ] **Step 1: 实现 + 测试**

`routing.rs`：
```rust
//! 任务→Provider 路由（ADR-0005 §3）。双 map 方案（F7）。

use super::provider::*;
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Task {
    Summarize,
    LinkSuggest,
    Embed,
    Chat,
    Rewrite,
}

#[derive(Debug, Clone)]
pub struct ProviderRef {
    pub provider_id: String,
    pub model: String,
}

/// 路由：每个任务一个首选 + 备选（降级用）。
#[derive(Debug, Clone, Default)]
pub struct Routing {
    /// 任务 → (首选, [备选...])
    pub map: HashMap<Task, (ProviderRef, Vec<ProviderRef>)>,
}

pub struct Registry {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    chats: HashMap<String, Arc<dyn ChatCap>>,
    embeds: HashMap<String, Arc<dyn EmbedCap>>,
}

impl Registry {
    pub fn new() -> Self {
        Self { providers: HashMap::new(), chats: HashMap::new(), embeds: HashMap::new() }
    }

    /// 注册一个 chat provider。
    pub fn register_chat<P>(&mut self, p: P)
    where
        P: LlmProvider + ChatCap + 'static,
    {
        let id = p.id().to_string();
        let arc: Arc<P> = Arc::new(p);
        self.chats.insert(id.clone(), arc.clone());
        self.providers.insert(id, arc);
    }

    /// 注册一个已有 Arc 的 chat provider（用于同一实例同时注册 chat+embed，评审 R8）。
    pub fn register_chat_arc<P>(&mut self, arc: Arc<P>)
    where
        P: LlmProvider + ChatCap + 'static,
    {
        let id = arc.id().to_string();
        self.chats.insert(id.clone(), arc.clone());
        self.providers.insert(id, arc);
    }

    /// 注册一个 embed provider。
    pub fn register_embed<P>(&mut self, p: P)
    where
        P: LlmProvider + EmbedCap + 'static,
    {
        let id = p.id().to_string();
        let arc: Arc<P> = Arc::new(p);
        self.embeds.insert(id.clone(), arc.clone());
        self.providers.insert(id, arc);
    }

    /// 注册一个已有 Arc 的 embed provider（同 register_chat_arc 用途）。
    pub fn register_embed_arc<P>(&mut self, arc: Arc<P>)
    where
        P: LlmProvider + EmbedCap + 'static,
    {
        let id = arc.id().to_string();
        self.embeds.insert(id.clone(), arc.clone());
        self.providers.insert(id, arc);
    }

    /// 按任务取 chat provider（首选 → 降级备选）。返回 (provider_arc, model)。
    pub fn chat_for(&self, routing: &Routing, task: Task) -> Result<(Arc<dyn ChatCap>, String)> {
        let (primary, fallbacks) = routing.map.get(&task)
            .ok_or_else(|| crate::CoreError::Conformance(format!("no routing for task {task:?}")))?;
        // 尝试首选 + 所有备选
        for pref in std::iter::once(primary).chain(fallbacks.iter()) {
            if let Some(p) = self.chats.get(&pref.provider_id) {
                return Ok((p.clone(), pref.model.clone()));
            }
        }
        Err(crate::CoreError::Conformance(format!(
            "no registered chat provider for task {task:?} (tried {} + {} fallbacks)",
            primary.provider_id, fallbacks.len()
        )))
    }

    /// 按任务取 embed provider。
    pub fn embed_for(&self, routing: &Routing, task: Task) -> Result<(Arc<dyn EmbedCap>, String)> {
        let (primary, fallbacks) = routing.map.get(&task)
            .ok_or_else(|| crate::CoreError::Conformance(format!("no routing for task {task:?}")))?;
        for pref in std::iter::once(primary).chain(fallbacks.iter()) {
            if let Some(p) = self.embeds.get(&pref.provider_id) {
                return Ok((p.clone(), pref.model.clone()));
            }
        }
        Err(crate::CoreError::Conformance(format!("no embed provider for task {task:?}")))
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn LlmProvider>> {
        self.providers.get(id).cloned()
    }

    pub fn list(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures_util::Stream;

    struct FakeChat;
    #[async_trait]
    impl LlmProvider for FakeChat {
        fn id(&self) -> &str { "fake" }
        fn kind(&self) -> ProviderKind { ProviderKind::Local }
        fn capabilities(&self) -> Capabilities { Capabilities::CHAT }
        async fn health(&self) -> Result<bool> { Ok(true) }
    }
    #[async_trait]
    impl ChatCap for FakeChat {
        async fn chat_stream(&self, _: ChatRequest) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin>> {
            Ok(Box::new(futures_util::stream::iter(vec![Ok("hi".into())])))
        }
    }

    fn routing(task: Task, primary: &str, fb: &[&str]) -> Routing {
        let mut map = HashMap::new();
        let primary_ref = ProviderRef { provider_id: primary.into(), model: "m".into() };
        let fbs: Vec<ProviderRef> = fb.iter().map(|f| ProviderRef { provider_id: f.to_string(), model: "m".into() }).collect();
        map.insert(task, (primary_ref, fbs));
        Routing { map }
    }

    #[test]
    fn resolves_primary_chat() {
        let mut reg = Registry::new();
        reg.register_chat(FakeChat);
        let r = routing(Task::Summarize, "fake", &[]);
        let (p, _) = reg.chat_for(&r, Task::Summarize).unwrap();
        assert_eq!(p.id(), "fake");
    }

    #[test]
    fn fallback_when_primary_missing() {
        let mut reg = Registry::new();
        reg.register_chat(FakeChat);
        // 首选 "absent" 不存在，降级到 "fake"
        let r = routing(Task::Summarize, "absent", &["fake"]);
        let (p, _) = reg.chat_for(&r, Task::Summarize).unwrap();
        assert_eq!(p.id(), "fake");
    }

    #[test]
    fn errors_when_all_missing() {
        let reg = Registry::new();
        let r = routing(Task::Chat, "absent", &["also-absent"]);
        assert!(reg.chat_for(&r, Task::Chat).is_err());
    }
}
```

- [ ] **Step 2: 跑测试 + Commit**

Run: `cargo test -p lmnotes-core llm::routing`
Expected: 3 测试 PASS（首选 + 降级 + 全失）。

```bash
git commit -m "feat(llm): Registry dual-map + Routing with fallback degradation"
```

---

## Task 5: 三层护栏 + dispatch 入口

**Files:**
- Create: `crates/lmnotes-core/src/llm/guard.rs`

**目标：** `check()` 三层检查（concept local_only + 敏感关键词 + 云端全局授权），返回 `Allow`/`Deny(reason)`。

- [ ] **Step 1: 实现 + 测试**

`guard.rs`（按 ADR-0005 §4，已在高层计划写过，此处完全相同）：
```rust
//! 三层隐私护栏（ADR-0005 §4）。
//!
//! GuardConfig 需满足 Send + Sync（Tauri State 要求）。其字段 `cloud_allowed: bool`
//! 与 `sensitive_patterns: Vec<String>` 均自动 Send + Sync，故 derive(Default, Clone)
//! 即满足，无需手动标记。

use super::provider::ProviderKind;

#[derive(Debug, Clone, Default)]
pub struct GuardConfig {
    pub cloud_allowed: bool,
    pub sensitive_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    Allow,
    Deny(String),
}

pub fn check(
    cfg: &GuardConfig,
    provider_kind: ProviderKind,
    content: &str,
    local_only: bool,
) -> GuardDecision {
    if provider_kind == ProviderKind::Local {
        return GuardDecision::Allow;
    }
    // Cloud provider：逐层检查
    if local_only {
        return GuardDecision::Deny("concept marked local_only, cloud not allowed".into());
    }
    for pat in &cfg.sensitive_patterns {
        if content.contains(pat.as_str()) {
            return GuardDecision::Deny(format!("sensitive pattern matched: {pat}"));
        }
    }
    if !cfg.cloud_allowed {
        return GuardDecision::Deny("cloud not globally authorized".into());
    }
    GuardDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> GuardConfig { GuardConfig { cloud_allowed: true, sensitive_patterns: vec!["密码".into()] } }

    #[test]
    fn local_always_allowed() {
        assert_eq!(check(&cfg(), ProviderKind::Local, "密码", true), GuardDecision::Allow);
    }
    #[test]
    fn local_only_blocks_cloud() {
        assert!(matches!(check(&GuardConfig::default(), ProviderKind::Cloud, "x", true), GuardDecision::Deny(_)));
    }
    #[test]
    fn sensitive_blocks_cloud() {
        assert!(matches!(check(&cfg(), ProviderKind::Cloud, "我的密码", false), GuardDecision::Deny(_)));
    }
    #[test]
    fn cloud_unauthorized_blocks() {
        let mut c = cfg(); c.cloud_allowed = false; c.sensitive_patterns.clear();
        assert!(matches!(check(&c, ProviderKind::Cloud, "x", false), GuardDecision::Deny(_)));
    }
    #[test]
    fn cloud_clean_content_allowed() {
        assert_eq!(check(&cfg(), ProviderKind::Cloud, "hello", false), GuardDecision::Allow);
    }
}
```

- [ ] **Step 2: 跑测试 + Commit**

Run: `cargo test -p lmnotes-core llm::guard`
Expected: 5 测试 PASS。

```bash
git commit -m "feat(llm): three-layer privacy guard (local_only + sensitive + cloud auth)"
```

---

## Task 6: 建议类型 + SuggestionStore（SQLite）

**Files:**
- Create: `crates/lmnotes-core/src/llm/suggestion.rs`
- Modify: `crates/lmnotes-core/src/index/schema.rs`（加 suggestions 表）
- Modify: `crates/lmnotes-core/src/index/sqlite.rs`（实现 SuggestionStore 方法）

- [ ] **Step 1: schema.rs 加 suggestions 表**

```rust
pub const CREATE_SUGGESTIONS: &str = "
CREATE TABLE IF NOT EXISTS suggestions (
    id          TEXT PRIMARY KEY,
    concept_id  TEXT NOT NULL,
    kind        TEXT NOT NULL,
    payload     TEXT NOT NULL,
    status      TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    applied_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_sugg_concept ON suggestions(concept_id);
CREATE INDEX IF NOT EXISTS idx_sugg_status ON suggestions(status);
";
```

- [ ] **Step 2: suggestion.rs（类型）**

```rust
//! 建议数据结构（ADR-0005 §6）。
//!
//! 序列化模型（评审 R1 修正）：`payload` 列存 `Suggestion` 的完整 serde JSON
//! （内部 tag，形如 `{"summary":{"text":"..."}}`）。读回时直接 `from_str(payload)`，
//! 不再重组。`kind` 列冗余存 `kind_str()` 仅供 SQL 查询/索引便利，反序列化不依赖它。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Suggestion {
    #[serde(rename = "summary")]
    Summary { text: String },
    #[serde(rename = "tag")]
    Tag { tag: String },
    #[serde(rename = "link")]
    Link { dst_path: String, link_text: String },
}

impl Suggestion {
    pub fn kind_str(&self) -> &'static str {
        match self {
            Suggestion::Summary { .. } => "summary",
            Suggestion::Tag { .. } => "tag",
            Suggestion::Link { .. } => "link",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionStatus {
    Pending,
    Accepted,
    Rejected,
}

impl SuggestionStatus {
    pub fn as_str(&self) -> &'static str {
        match self { SuggestionStatus::Pending => "pending", SuggestionStatus::Accepted => "accepted", SuggestionStatus::Rejected => "rejected" }
    }
    pub fn parse(s: &str) -> Self {
        match s { "accepted" => Self::Accepted, "rejected" => Self::Rejected, _ => Self::Pending }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionRecord {
    pub id: String,
    pub concept_id: String,
    pub suggestion: Suggestion,
    pub status: SuggestionStatus,
}
```

- [ ] **Step 3: SqliteIndex 加 SuggestionStore 方法**

在 `index/sqlite.rs` impl SqliteIndex 追加。
**关键（评审 R1/R5 修正）**：`payload` 直接存完整 JSON，读回直接反序列化；`now_secs` 在 sqlite.rs 内定义局部副本（indexer 模块的 fn 是私有的，跨模块引用会编译失败）。

```rust
    /// sqlite.rs 内的 now_secs 局部副本（indexer::now_secs 是私有的，不能跨模块用）。
    fn now_secs() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64).unwrap_or(0)
    }

    pub fn list_pending_suggestions(&self) -> crate::Result<Vec<crate::llm::suggestion::SuggestionRecord>> {
        use crate::llm::suggestion::{SuggestionRecord, SuggestionStatus, Suggestion};
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, concept_id, payload, status FROM suggestions WHERE status='pending' ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let concept_id: String = r.get(1)?;
            let payload: String = r.get(2)?;
            let status: String = r.get(3)?;
            // payload 是完整 Suggestion JSON（内部 tag），直接反序列化
            let suggestion: Suggestion = serde_json::from_str(&payload)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    2, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok(SuggestionRecord { id, concept_id, suggestion, status: SuggestionStatus::parse(&status) })
        })?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    pub fn list_suggestions_for(&self, concept_id: &str) -> crate::Result<Vec<crate::llm::suggestion::SuggestionRecord>> {
        use crate::llm::suggestion::{SuggestionRecord, SuggestionStatus, Suggestion};
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, payload, status FROM suggestions WHERE concept_id=?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([concept_id], |r| {
            let id: String = r.get(0)?;
            let payload: String = r.get(1)?;
            let status: String = r.get(2)?;
            let suggestion: Suggestion = serde_json::from_str(&payload)
                .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                    1, rusqlite::types::Type::Text, Box::new(e)))?;
            Ok(SuggestionRecord { id, concept_id: concept_id.to_string(), suggestion, status: SuggestionStatus::parse(&status) })
        })?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    pub fn insert_suggestion(&self, id: &str, concept_id: &str, suggestion: &crate::llm::suggestion::Suggestion) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        // payload 存完整 Suggestion JSON（含内部 tag）；kind 冗余供查询
        let payload = serde_json::to_string(suggestion)
            .map_err(|e| crate::CoreError::Yaml(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO suggestions (id, concept_id, kind, payload, status, created_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
            rusqlite::params![id, concept_id, suggestion.kind_str(), payload, now_secs()],
        )?;
        Ok(())
    }

    pub fn set_suggestion_status(&self, id: &str, status: crate::llm::suggestion::SuggestionStatus) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        let applied = if matches!(status, crate::llm::suggestion::SuggestionStatus::Accepted | crate::llm::suggestion::SuggestionStatus::Rejected) {
            Some(now_secs())
        } else { None };
        conn.execute(
            "UPDATE suggestions SET status=?1, applied_at=COALESCE(?2, applied_at) WHERE id=?3",
            rusqlite::params![status.as_str(), applied, id],
        )?;
        Ok(())
    }
```

- [ ] **Step 4: 修改 init_schema 含 suggestions 表（必须在测试前）**

`SqliteIndex::init_schema` 的 batch 加 `CREATE_SUGGESTIONS`（评审 R3：顺序在测试前）：
```rust
conn.execute_batch(&format!("{CREATE_CONCEPTS}\n{CREATE_EDGES}\n{CREATE_VEC}\n{CREATE_SUGGESTIONS}"))?;
```

- [ ] **Step 5: 测试（round-trip，需 tokio 因 init_schema 异步）**

在 `index/sqlite.rs` 测试模块追加（评审 R3：用 `#[tokio::test]`）：
```rust
    #[tokio::test]
    async fn suggestion_round_trip() {
        use crate::llm::suggestion::{Suggestion, SuggestionStatus};
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap(); // 含 CREATE_SUGGESTIONS（Step 4 已加）
        let s = Suggestion::Summary { text: "测试摘要".into() };
        idx.insert_suggestion("sg_1", "nt_1", &s).unwrap();

        // pending 列表含刚插入的
        let pending = idx.list_pending_suggestions().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "sg_1");
        assert_eq!(pending[0].concept_id, "nt_1");
        match &pending[0].suggestion {
            Suggestion::Summary { text } => assert_eq!(text, "测试摘要"),
            _ => panic!("expected Summary"),
        }

        // accept 后不在 pending
        idx.set_suggestion_status("sg_1", SuggestionStatus::Accepted).unwrap();
        assert!(idx.list_pending_suggestions().unwrap().is_empty());

        // list_suggestions_for 仍能看到（不限 status）
        let all = idx.list_suggestions_for("nt_1").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, SuggestionStatus::Accepted);

        // tag/link 类型 round-trip
        idx.insert_suggestion("sg_2", "nt_1", &Suggestion::Tag { tag: "ai".into() }).unwrap();
        idx.insert_suggestion("sg_3", "nt_1", &Suggestion::Link { dst_path: "/notes/x.md".into(), link_text: "x".into() }).unwrap();
        let all2 = idx.list_suggestions_for("nt_1").unwrap();
        assert_eq!(all2.len(), 3, "should have summary+tag+link");
    }
```

- [ ] **Step 6: 跑测试 + Commit**

Run: `cargo test -p lmnotes-core index::sqlite`
Expected: 全 PASS（含新 suggestion_round_trip + 旧 SqliteIndex 测试 5 个）。

```bash
git commit -m "feat(llm): suggestion types + SQLite suggestion store (payload=full JSON)"
```

---

## Task 7: 增量索引器接 LLM + embed 写 sqlite-vec

**Files:**
- Modify: `crates/lmnotes-core/src/indexer/mod.rs`（加 `generate_suggestions`）
- Modify: `crates/lmnotes-core/src/index/sqlite.rs`（加 `upsert_vector`/`vector_search`）

- [ ] **Step 1: SqliteIndex 加向量方法**

`index/sqlite.rs` impl 追加：
```rust
    /// 写入 concept 向量到 vec_concepts（sqlite-vec）。
    pub fn upsert_vector(&self, id: &str, embedding: &[f32]) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        let ser: String = format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
        conn.execute("INSERT OR REPLACE INTO vec_concepts (id, embedding) VALUES (?1, ?2)", rusqlite::params![id, ser])?;
        Ok(())
    }

    /// KNN 向量检索，返回 (id, distance) 列表。
    pub fn vector_search(&self, q: &[f32], k: usize) -> crate::Result<Vec<(String, f32)>> {
        let conn = self.conn.lock().unwrap();
        let ser = format!("[{}]", q.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
        let sql = format!("SELECT id, distance FROM vec_concepts WHERE embedding MATCH ?1 ORDER BY distance LIMIT {k}");
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([&ser], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f32>(1)?)))?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }
```

- [ ] **Step 2: vector 测试**

```rust
    #[test]
    fn vector_search_returns_nearest() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap(); // 注意 init_schema 是 async，vector 测试需 #[tokio::test]
        idx.upsert_vector("nt_1", &[1.0, 0.0, 0.0]).unwrap();
        idx.upsert_vector("nt_2", &[0.0, 1.0, 0.0]).unwrap();
        // 注意：vec 表 schema 是 float[768]，测试用 3 维需改 schema 或用 768 维向量。
        // 执行决策：M1b 测试用 mock embed 返回固定 768 维向量，或临时改 vec 表维度为 3。
        // **执行时**：保持 float[768]，测试构造 768 维向量（仅前几维非零）。
    }
```

> **执行注意：** vec 表固定 768 维（与 Ollama nomic-embed-text 对齐）。测试向量需是 768 维。用 helper `fn v768(lead: &[f32]) -> Vec<f32>` 构造。

- [ ] **Step 3: generate_suggestions（indexer 扩展）**

`indexer/mod.rs` 加：
```rust
use crate::llm::{ChatCap, EmbedCap, ChatRequest, ChatMessage, ChatRole, guard::{GuardConfig, GuardDecision, check}, routing::{Registry, Routing, Task}, suggestion::{Suggestion, SuggestionStatus}};

/// 对一个 concept 生成 LLM 建议（摘要/标签/链接），写入 suggestion store。
/// 纯异步，由 save_concept 在索引完成后 spawn 调用。
pub async fn generate_suggestions(
    concept: &Concept,
    concept_path: &str,
    sqlite: &SqliteIndex,
    registry: &Registry,
    routing: &Routing,
    guard_cfg: &GuardConfig,
    concept_text: &str,
) -> crate::Result<()> {
    let concept_id = concept.frontmatter.id.clone().unwrap_or_else(|| concept_path.to_string());
    let local_only = concept.frontmatter.extra.get("llm_local_only")
        .and_then(|v| v.as_bool()).unwrap_or(false);

    // 1. 摘要（用 chat provider）
    if let Ok((chat, model)) = registry.chat_for(routing, Task::Summarize) {
        let kind = chat.kind();
        match check(guard_cfg, kind, concept_text, local_only) {
            GuardDecision::Allow => {
                let req = ChatRequest {
                    model,
                    messages: vec![
                        ChatMessage { role: ChatRole::System, content: "用一句话（≤50字）总结这段笔记的核心内容。只输出总结，不加前缀。".into() },
                        ChatMessage { role: ChatRole::User, content: concept_text.to_string() },
                    ],
                    temperature: Some(0.3),
                };
                if let Ok(summary) = chat.chat(req).await {
                    let trimmed = summary.trim();
                    if !trimmed.is_empty() {
                        let sid = format!("sg_{}_sum", concept_id);
                        sqlite.insert_suggestion(&sid, &concept_id, &Suggestion::Summary { text: trimmed.into() })?;
                    }
                }
            }
            GuardDecision::Deny(reason) => eprintln!("guard deny summary for {concept_id}: {reason}"),
        }
    }

    // 2. 标签（chat provider）
    if let Ok((chat, model)) = registry.chat_for(routing, Task::LinkSuggest) {
        let kind = chat.kind();
        if matches!(check(guard_cfg, kind, concept_text, local_only), GuardDecision::Allow) {
            let req = ChatRequest {
                model,
                messages: vec![
                    ChatMessage { role: ChatRole::System, content: "提取这段笔记的 3-5 个标签，每行一个，不加序号或符号。".into() },
                    ChatMessage { role: ChatRole::User, content: concept_text.to_string() },
                ],
                temperature: Some(0.3),
            };
            if let Ok(tags_text) = chat.chat(req).await {
                for tag in tags_text.lines().map(|s| s.trim()).filter(|s| !s.is_empty()).take(5) {
                    let sid = format!("sg_{}_tag_{}", concept_id, sanitize(tag));
                    sqlite.insert_suggestion(&sid, &concept_id, &Suggestion::Tag { tag: tag.into() })?;
                }
            }
        }
    }

    // 3. 向量 embed + 写 vec_concepts（为链接建议与 M1c RAG 服务）
    if let Ok((embedder, model)) = registry.embed_for(routing, Task::Embed) {
        let kind = embedder.kind();
        if matches!(check(guard_cfg, kind, concept_text, local_only), GuardDecision::Allow) {
            if let Ok(vectors) = embedder.embed(&model, &[concept_text.to_string()]).await {
                if let Some(v) = vectors.into_iter().next() {
                    if let Err(e) = sqlite.upsert_vector(&concept_id, &v) {
                        eprintln!("vec insert fail {concept_id}: {e}");
                    }
                }
            }
        }
    }

    Ok(())
}

fn sanitize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-').take(20).collect()
}
```

> **链接建议（link suggest）** 推迟到 embed 写入后用 `vector_search` 取相似 top-5，再 LLM 判断关联性——这是 M1c 的工作（需要 RAG 检索器）。M1b 只做摘要 + 标签 + embed 写入，链接建议留 M1c Task。

- [ ] **Step 4: 测试（mock chat provider）**

```rust
    #[tokio::test]
    async fn generate_suggestions_writes_summary_when_allowed() {
        // 用 FakeChat（返回固定 "摘要内容"）注册，generate 后检查 suggestion store 有 pending 摘要
        // 执行时填充：构造 Registry + Routing + GuardConfig + Concept，调 generate_suggestions，断言 sqlite.list_pending_suggestions() 非空
    }
```

- [ ] **Step 5: 跑测试 + Commit**

```bash
git commit -m "feat(indexer): LLM suggestion generation (summary/tag + vector embed write)"
```

---

## Task 8: 建议中心 UI + Tauri 命令

**Files:**
- Create: `apps/desktop/src/suggestions/SuggestionCenter.tsx`
- Create: `apps/desktop/src/store/llm.ts`
- Modify: `apps/desktop/src/App.tsx`（右栏显示建议）
- Modify: `apps/desktop/src-tauri/src/commands.rs`（list/accept/reject 命令）
- Modify: `apps/desktop/src-tauri/src/lib.rs`（注入 SqliteIndex 到 State）

- [ ] **Step 1: Tauri 命令**

`commands.rs` 追加：
```rust
use lmnotes_core::index::sqlite::SqliteIndex;
use lmnotes_core::llm::suggestion::{SuggestionRecord, SuggestionStatus};

#[tauri::command]
pub fn list_suggestions(sqlite: State<'_, Arc<SqliteIndex>>) -> Result<Vec<SuggestionRecord>, String> {
    sqlite.list_pending_suggestions().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn accept_suggestion(id: String, sqlite: State<'_, Arc<SqliteIndex>>) -> Result<(), String> {
    sqlite.set_suggestion_status(&id, SuggestionStatus::Accepted).map_err(|e| e.to_string())
    // TODO: 接受后写回 concept frontmatter/description（M1b 简化：仅标记状态，写回留 M1c）
}

#[tauri::command]
pub fn reject_suggestion(id: String, sqlite: State<'_, Arc<SqliteIndex>>) -> Result<(), String> {
    sqlite.set_suggestion_status(&id, SuggestionStatus::Rejected).map_err(|e| e.to_string())
}
```

`lib.rs` 把 `meta`（SqliteIndex）重新 manage 进 State（M1a 移除了，因为命令没用到；M1b 建议命令要用）：
```rust
.manage(meta.clone())  // SqliteIndex
```
注册三个新命令。

- [ ] **Step 2: store/llm.ts**

```ts
import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export interface SuggestionRecord {
  id: string;
  concept_id: string;
  suggestion:
    | { kind: "summary"; text: string }
    | { kind: "tag"; tag: string }
    | { kind: "link"; dst_path: string; link_text: string };
  status: "pending" | "accepted" | "rejected";
}

const [suggestions, setSuggestions] = createSignal<SuggestionRecord[]>([]);

export function useSuggestions() {
  return { suggestions, setSuggestions };
}

export async function loadSuggestions() {
  try {
    const r = await invoke<SuggestionRecord[]>("list_suggestions");
    setSuggestions(r);
  } catch (e) {
    console.error("load suggestions", e);
  }
}

export async function acceptSuggestion(id: string) {
  await invoke("accept_suggestion", { id });
  setSuggestions((prev) => prev.filter((s) => s.id !== id));
}

export async function rejectSuggestion(id: string) {
  await invoke("reject_suggestion", { id });
  setSuggestions((prev) => prev.filter((s) => s.id !== id));
}
```

- [ ] **Step 3: SuggestionCenter.tsx**

```tsx
import { For, Show, onMount } from "solid-js";
import { useSuggestions, acceptSuggestion, rejectSuggestion, loadSuggestions } from "../store/llm";

export function SuggestionCenter() {
  const { suggestions } = useSuggestions();
  onMount(() => loadSuggestions());

  return (
    <div class="suggestion-list">
      <Show when={suggestions().length === 0}>
        <p class="muted small">暂无待审建议</p>
      </Show>
      <For each={suggestions()}>
        {(s) => (
          <div class="suggestion-item">
            <div class="suggestion-kind">{s.suggestion.kind}</div>
            <div class="suggestion-body">
              {s.suggestion.kind === "summary" && <span>{s.suggestion.text}</span>}
              {s.suggestion.kind === "tag" && <code>#{s.suggestion.tag}</code>}
              {s.suggestion.kind === "link" && <code>[[{s.suggestion.link_text}]]</code>}
            </div>
            <div class="suggestion-actions">
              <button class="btn-accept" onClick={() => acceptSuggestion(s.id)}>✓</button>
              <button class="btn-reject" onClick={() => rejectSuggestion(s.id)}>✕</button>
            </div>
          </div>
        )}
      </For>
    </div>
  );
}
```

- [ ] **Step 4: App.tsx 右栏替换**

把 backrefs 占位替换为 `<SuggestionCenter />`（反链面板 M1c 加）。

- [ ] **Step 5: 样式**

追加 styles.css：`.suggestion-list`, `.suggestion-item`, `.suggestion-kind`, `.btn-accept`, `.btn-reject`。

- [ ] **Step 6: tsc + cargo check**

- [ ] **Step 7: Commit**

```bash
git commit -m "feat(ui): suggestion center (review/accept/reject)"
```

---

## Task 9: 就地改写 + 撤销（快照）

**Files:**
- Create: `apps/desktop/src/editor/RewriteMenu.tsx`
- Modify: `apps/desktop/src-tauri/src/commands.rs`（rewrite 命令 + 快照）
- Modify: `apps/desktop/src/editor/Editor.tsx`（选区右键菜单）

- [ ] **Step 1: rewrite 命令（含快照）**

`commands.rs` 追加：
```rust
/// 就地改写：对选中文本执行 action（polish/expand/translate/summarize），返回新文本。
/// 改写前把原 concept 文本存快照到 .lmnotes/llm/snapshots/（ADR-0001 撤销）。
#[tauri::command]
pub async fn rewrite_selection(
    action: String,       // polish | expand | translate | summarize
    selection: String,
    concept_path: String,
    registry: State<'_, Arc<Registry>>,
    routing: State<'_, Arc<Routing>>,
    guard_cfg: State<'_, GuardConfig>,
) -> Result<String, String> {
    use lmnotes_core::llm::{ChatRequest, ChatMessage, ChatRole, routing::Task, guard::check};
    let (chat, model) = registry.chat_for(&routing, Task::Rewrite).map_err(|e| e.to_string())?;
    let kind = chat.kind();
    let local_only = false; // 改写由用户主动触发，不读 concept 标记
    match check(&guard_cfg, kind, &selection, local_only) {
        lmnotes_core::llm::guard::GuardDecision::Allow => {}
        lmnotes_core::llm::guard::GuardDecision::Deny(reason) => return Err(reason),
    }
    let prompt = match action.as_str() {
        "polish" => "润色以下文本，保持原意，使其更流畅专业。只输出润色后的文本。",
        "expand" => "扩写以下文本，补充细节与例证，保持原意。只输出扩写后的文本。",
        "translate" => "将以下文本翻译为英文。只输出译文。",
        "summarize" => "用要点列表总结以下文本。只输出要点。",
        _ => return Err(format!("unknown action: {action}")),
    };
    let req = ChatRequest {
        model,
        messages: vec![
            ChatMessage { role: ChatRole::System, content: prompt.into() },
            ChatMessage { role: ChatRole::User, content: selection },
        ],
        temperature: Some(0.5),
    };
    chat.chat(req).await.map_err(|e| e.to_string())
}

/// 保存快照（撤销用）。存到 .lmnotes/llm/snapshots/<concept_path>-<ts>.md
#[tauri::command]
pub async fn save_snapshot(concept_path: String, text: String) -> Result<String, String> {
    let ts = chrono::Utc::now().timestamp();
    let safe = concept_path.replace(['/', '\\'], "_");
    let rel = format!(".lmnotes/llm/snapshots/{safe}-{ts}.md");
    let full = vault_root().join(&rel);
    if let Some(p) = full.parent() {
        tokio::fs::create_dir_all(p).await.map_err(|e| e.to_string())?;
    }
    tokio::fs::write(&full, &text).await.map_err(|e| e.to_string())?;
    Ok(rel)
}
```

`lib.rs` manage `Registry`/`Routing`/`GuardConfig` 到 State。

- [ ] **Step 2: RewriteMenu.tsx**

选中文本 → 右键菜单（4 项）→ 调 rewrite_selection → 替换 CodeMirror 选区。替换前调 save_snapshot 存当前全文。

- [ ] **Step 3: Editor.tsx 接入右键**

CodeMirror 的 `EditorView.domEventHandlers({ contextmenu })` 扩展。

- [ ] **Step 4: tsc + cargo check + Commit**

```bash
git commit -m "feat(editor): inline rewrite (polish/expand/translate/summarize) with snapshot rollback"
```

---

## Task 10: Provider 配置 UI + 首启探测

**Files:**
- Create: `apps/desktop/src/settings/ProviderSettings.tsx`
- Create: `apps/desktop/src-tauri/llm_config.rs`（配置读写）
- Modify: `apps/desktop/src-tauri/src/lib.rs`（首启探测 + Registry 构建）

- [ ] **Step 1: 配置文件 `~/.lmnotes/config.json`**

`llm_config.rs`：
```rust
use lmnotes_core::ll::{ProviderKind, routing::{Routing, ProviderRef, Task}, guard::GuardConfig};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub providers: Vec<ProviderConfig>,
    pub routing: RoutingConfig,
    pub guard: GuardConfigSer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProviderConfig {
    /// Ollama 本地（id 固定 "ollama"，单实例；评审 R8）。
    #[serde(rename = "ollama")]
    Ollama { base_url: String, chat_model: String, embed_model: String },
    #[serde(rename = "openai")]
    OpenAi { id: String, base_url: String, api_key: String, chat_model: String, embed_model: String },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoutingConfig {
    pub summarize: Option<ProviderRefSer>,
    pub link_suggest: Option<ProviderRefSer>,
    pub embed: Option<ProviderRefSer>,
    pub chat: Option<ProviderRefSer>,
    pub rewrite: Option<ProviderRefSer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRefSer { pub provider: String, pub model: String }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GuardConfigSer {
    #[serde(default)] pub cloud_allowed: bool,
    #[serde(default)] pub sensitive_patterns: Vec<String>,
}

impl Config {
    pub fn load_or_default() -> Self { /* 读 ~/.lmnotes/config.json，不存在返回默认 */ }
    pub fn save(&self) -> Result<(), String> { /* 写 */ }
}
```

- [ ] **Step 2: lib.rs 首启探测（O6c）**

```rust
fn build_registry_and_routing(cfg: &Config) -> (Registry, Routing, GuardConfig) {
    let mut reg = Registry::new();
    for p in &cfg.providers {
        match p {
            // 评审 R8：M1b OllamaProvider id 固定 "ollama"，单实例。base_url 仍可配（默认 localhost:11434）。
            // 同时注册为 chat 和 embed provider（OllamaProvider 实现了两 trait，但 register_chat/register_embed
            // 各只收一个 trait 约束——需分别调用两次注册，用同一实例的 Arc 克隆）。
            ProviderConfig::Ollama { base_url, .. } => {
                let ollama = std::sync::Arc::new(OllamaProvider::new(base_url));
                // OllamaProvider 同时 impl ChatCap + EmbedCap，分别注册
                reg.register_chat_arc(ollama.clone());
                reg.register_embed_arc(ollama.clone());
            }
            ProviderConfig::OpenAi { id, base_url, api_key, .. } => {
                let openai = std::sync::Arc::new(OpenAiProvider::new(id, base_url, api_key));
                reg.register_chat_arc(openai.clone());
                reg.register_embed_arc(openai.clone());
            }
        }
    }
    // routing 从 cfg.routing 映射到 Routing（5 个任务槽位）
    // 每个 slot：首选 ProviderRef + 空备选（M1b 不配降级链；如需可在 Config 扩展）
    let mut map = std::collections::HashMap::new();
    if let Some(r) = &cfg.routing.summarize {
        map.insert(Task::Summarize, (provider_ref(r), vec![]));
    }
    if let Some(r) = &cfg.routing.link_suggest {
        map.insert(Task::LinkSuggest, (provider_ref(r), vec![]));
    }
    if let Some(r) = &cfg.routing.embed {
        map.insert(Task::Embed, (provider_ref(r), vec![]));
    }
    if let Some(r) = &cfg.routing.chat {
        map.insert(Task::Chat, (provider_ref(r), vec![]));
    }
    if let Some(r) = &cfg.routing.rewrite {
        map.insert(Task::Rewrite, (provider_ref(r), vec![]));
    }
    let routing = Routing { map };
    let guard = GuardConfig {
        cloud_allowed: cfg.guard.cloud_allowed,
        sensitive_patterns: cfg.guard.sensitive_patterns.clone(),
    };
    (reg, routing, guard)
}

/// ProviderRefSer → ProviderRef
fn provider_ref(r: &ProviderRefSer) -> ProviderRef {
    ProviderRef { provider_id: r.provider.clone(), model: r.model.clone() }
}
```

> **Registry 复用（评审 R8）**：`OllamaProvider` 同时 impl `ChatCap` + `EmbedCap`，需分别注册到 chats 和 embeds 两个 map。用 `register_chat_arc`/`register_embed_arc`（T4 已定义）共享同一 `Arc<OllamaProvider>`（Rust 的 `Arc<P> → Arc<dyn Trait>` unsize 强制转换自动处理，clone 共享引用计数，不产生重复所有权）。

首启探测：检测 Ollama health，可用→默认本地；不可用→弹窗引导配置云端。

- [ ] **Step 3: ProviderSettings.tsx**

表单：providers 列表、每个任务槽位下拉、cloud_allowed 开关、敏感关键词编辑、health 测试按钮。

- [ ] **Step 4: tsc + cargo check + Commit**

```bash
git commit -m "feat(settings): provider config UI + first-run Ollama detection"
```

---

## M1b 退出标准（Definition of Done）

对照 PRD §13 B 组（3–5 环）：

- [ ] `cargo test --workspace` 全绿（M1a 的 58 + M1b 新增 ~20 测试）
- [ ] `cargo clippy --workspace -- -D warnings` 无警告
- [ ] LLM Provider 抽象（trait 拆分）+ Ollama/OpenAI 兼容实现可用（wiremock 测试）
- [ ] 路由按任务分派，能力探测 + 缺能力降级（有测试）
- [ ] 三层护栏强制（5 测试），云端默认禁用
- [ ] 保存笔记 → LLM 生成摘要/标签建议入队（generate_suggestions）
- [ ] 建议中心 UI：审阅/接受/拒绝，键盘 J/K/Enter/D
- [ ] 就地改写（4 种动作）+ 撤销（快照存 .lmnotes/llm/snapshots/）
- [ ] Provider 配置 UI + 首次启动探测（O6c）
- [ ] **向量层**：concept embed 写入 sqlite-vec（upsert_vector + vector_search 有测试）
- [ ] CI 全绿（含 wiremock mock 测试）
- [ ] **§13 B 组闭环**（3–5 环）：写笔记 → LLM 建摘要/标签建议 → 接受 → 改写 → 撤销

---

## Self-Review

**1. Spec coverage（PRD §5.4 FR-LLM-01/02/03/05/08/09 + FR-MODEL-01/02/03/04）**
- FR-LLM-01（后台索引器生成摘要/标签/链接建议）→ T7（摘要+标签+embed；链接建议因需向量检索留 M1c）
- FR-LLM-02（向量索引）→ T7 embed + upsert_vector ✓
- FR-LLM-03（建议中心审阅/接受/拒绝/批量）→ T6 store + T8 UI ✓（批量：T8 可加全选）
- FR-LLM-05（就地改写润色/扩写/翻译/总结）→ T9 ✓
- FR-LLM-08（是否上云开关）→ T5 护栏 + T10 UI ✓
- FR-LLM-09（回滚快照）→ T9 save_snapshot ✓
- FR-MODEL-01/02/03/04（Provider 抽象/分派/配置/隐私分级）→ T1/T4/T10 ✓
- **FR-LLM-04（图谱问答）→ M1c**（明确推迟，依赖向量检索 + RAG）
- **FR-LLM-06/07（行动项/每日回顾）→ M2**

**2. Placeholder scan**
- ~~T6 `list_suggestions_for` 标 `unimplemented!`~~ **已修复（评审 R1/R2）**：T6 现含完整实现（payload 全 JSON，直接反序列化，加 WHERE concept_id）。
- T7 测试 `generate_suggestions_writes_summary_when_allowed` 给了断言意图但未逐行——**中等复杂度测试，执行时按 mock provider 填充**。这是合理的计划粒度（非纯机械任务）。
- T2/T3 的 stream unfold 实现是完整代码，非占位。
- ~~T10 `build_registry_and_routing` 具体 routing 映射需执行时填充~~ **已修复（评审 R8）**：T10 现含完整 5 任务槽位映射 + provider_ref 辅助函数。

**3. Type consistency**
- `Suggestion`/`SuggestionRecord`/`SuggestionStatus` 跨 T6/T7/T8 一致（serde tag="kind"，payload 全 JSON 读写）
- `Task` 枚举（Summarize/LinkSuggest/Embed/Chat/Rewrite）跨 T4/T7/T9/T10 一致
- `Registry`/`Routing`/`ProviderRef` 跨 T4/T7/T9/T10 一致；T4 含 register_chat/register_embed/register_chat_arc/register_embed_arc 四个方法
- `GuardConfig`/`GuardDecision`/`check` 跨 T5/T7/T9 一致；GuardConfig 自动 Send+Sync（评审 R6）
- `ChatRequest`/`ChatMessage`/`ChatRole` 跨 T1/T2/T3/T7/T9 一致

**4. 评审修正记录（2026-06-23）**
本计划经一轮评审发现 8 项问题，全部已修：
- **R1+R2（高）**：suggestion 序列化模型重构——payload 存完整 JSON（含内部 tag），读写直接 serde，删除非法 JSON 重组逻辑
- **R3（中）**：T6 步骤重排——init_schema 含 suggestions 表移到测试前；测试改 `#[tokio::test]`；测试覆盖 summary/tag/link 三类型 + pending/accepted 状态
- **R5（中）**：T6 sqlite.rs 加 `now_secs` 局部定义（indexer 的私有 fn 不能跨模块）
- **R7（中）**：Goal + 范围边界显式声明"链接建议推迟 M1c"
- **R8（中）**：T10 删除 OllamaWrapper；ProviderConfig::Ollama 去 id 字段；build_registry_and_routing 用 Arc 注册 + 完整 5 任务映射；T4 加 register_*_arc 方法
- **R4（低）**：concept_text 多次 clone 是次要浪费，不改（保持计划简洁）
- **R6（低）**：GuardConfig 加注释说明自动 Send+Sync

**5. 剩余执行注意点（非缺陷，执行时处理）**
- **vec 表维度固定 768**：测试向量需 768 维（与 Ollama nomic-embed-text 对齐）。T7 测试用 helper `fn v768(lead: &[f32]) -> Vec<f32>` 构造（lead 填前几维，余补 0.0）。
- **T7 generate_suggestions 测试**：用 FakeChat（固定返回 "摘要"）+ FakeEmbed（固定返回 v768）注册，断言 suggestion store 有 pending 摘要 + vec 表有向量。执行时填充。
- **T8 批量接受**：建议中心可加"全部接受"按钮，M1b 可选。
- **T10 首启探测**：spawn 检测 ollama health，不可用时前端弹窗——Tauri event 通信，执行时落实。

**6. 与 M1a 的接口**
- `IndexBackend` trait 不变（M1b 不改 trait，只扩展 SqliteIndex impl）
- `SqliteIndex` 新增方法（vector/suggestion）是 inherent impl，不破坏 M1a；`init_schema` 扩展含 CREATE_SUGGESTIONS（M1a 已建 vec 表）
- `generate_suggestions` 是独立异步 fn，不污染 `index_concept` 同步路径
- Tauri State：M1b 加 manage Registry/Routing/GuardConfig/SqliteIndex（M1a 已 manage indexer/engine）

---

## Execution Handoff

计划已细化至执行步骤级，保存至 `docs/superpowers/plans/2026-06-22-m1b-llm-suggestions.md`（覆盖原高层版）。

执行方式：与 M1a 相同的内联执行 + 两阶段评审（spec 自检 + 质量自检）。注意：
- T2/T3 用 wiremock，需先确认 wiremock 在测试环境可启动（它在后台起 HTTP server）
- T7 generate_suggestions 测试用 FakeChat mock provider，不依赖真实 Ollama
- T8–T10 前端 + 集成，无 TDD 网关，手动验证为主
