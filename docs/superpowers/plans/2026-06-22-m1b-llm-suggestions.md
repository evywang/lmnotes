# M1b: LLM 智能层 + 建议中心 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans. Steps use checkbox (`- [ ]`) syntax.

**Goal:** 在 M1a 索引层之上接入 LLM：实现 `LlmProvider` 抽象（按能力拆 trait，ADR-0005 F7）、内置 Ollama（chat/embed）与 OpenAI 兼容 Provider、任务路由、三层隐私护栏、后台索引器接 LLM 生成摘要/标签/链接建议、建议中心 UI（审阅/接受/拒绝/批量）、就地改写 + 撤销。向量层（embed 写入 sqlite-vec）在此里程碑完成。

**依赖：** M1a 已完成（index/indexer/search/Tauri 壳）。

**Architecture:**
- `lmnotes-core/src/llm/`：Provider trait（LlmProvider + ChatCap/EmbedCap）、Ollama/OpenAI 实现、路由、护栏、建议队列。
- `lmnotes-core/src/indexer/`：扩展，保存后异步触发 LLM 建议（摘要/标签/链接）。
- `apps/desktop/src/suggestions/`：建议中心 UI。
- `apps/desktop/src/editor/`：就地改写菜单（选中 → 润色/扩写/翻译/总结）。

**Tech Stack:** Rust：`reqwest` 0.12（HTTP，blocking + async feature）、`tokio`、`futures`（流式）。前端：SolidJS streams + Tauri events。

---

## File Structure

```
lmnotes/crates/lmnotes-core/src/
├── llm/
│   ├── mod.rs              # [T1] 模块入口 + trait
│   ├── provider.rs         # [T1] LlmProvider + ChatCap/EmbedCap/... trait
│   ├── ollama.rs           # [T2] Ollama 实现
│   ├── openai.rs           # [T3] OpenAI 兼容实现
│   ├── routing.rs          # [T4] Routing + 调度
│   ├── guard.rs            # [T5] 三层护栏
│   └── suggestion.rs       # [T6] 建议类型 + 队列
├── indexer/
│   └── mod.rs              # [T7] 扩展：LLM 建议生成
└── ...（M1a 已有）
apps/desktop/src/
├── suggestions/
│   └── SuggestionCenter.tsx  # [T8] 建议中心 UI
├── editor/
│   └── RewriteMenu.tsx       # [T9] 就地改写
└── settings/
    └── ProviderSettings.tsx  # [T10] Provider 配置 UI
```

---

## Task 1: LlmProvider trait + 能力拆分

**Files:**
- Create: `crates/lmnotes-core/src/llm/mod.rs`
- Create: `crates/lmnotes-core/src/llm/provider.rs`
- Modify: `crates/lmnotes-core/src/lib.rs`（加 `pub mod llm;`）
- Modify: `crates/lmnotes-core/Cargo.toml`（加 reqwest/tokio-stream/futures）

**目标：** 按 ADR-0005 F7 拆 trait：`LlmProvider`（身份/能力/健康）+ `ChatCap`/`EmbedCap`（能力 trait）。调度器按能力动态分发。

- [ ] **Step 1: Cargo.toml 加依赖**

```toml
reqwest = { version = "0.12", features = ["json", "stream"] }
tokio-stream = "0.1"
futures-util = "0.3"
```

- [ ] **Step 2: provider.rs（trait 定义 + 共享类型）**

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
    /// 流式 chat。
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<Box<dyn Stream<Item = Result<String>> + Send + Unpin>>;
    /// 非流式（内部聚合 stream）。
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

> 需加 `bitflags = "2"` 依赖。

- [ ] **Step 3: mod.rs**

```rust
//! LLM Provider 抽象 + 路由 + 护栏（ADR-0005）。

pub mod provider;
pub mod ollama;
pub mod openai;
pub mod routing;
pub mod guard;
pub mod suggestion;

pub use provider::*;
```

占位文件（T2–T6 填充）：`ollama.rs`/`openai.rs`/`routing.rs`/`guard.rs`/`suggestion.rs` 各 `// T? 实现`。

- [ ] **Step 4: 验证编译**

Run: `cargo check -p lmnotes-core`
Expected: 编译通过（trait 定义完整，实现待 T2+）。

- [ ] **Step 5: Commit**

```bash
git add crates/lmnotes-core/
git commit -m "feat(llm): LlmProvider trait with capability split (ChatCap/EmbedCap)"
```

---

## Task 2: Ollama Provider 实现

**Files:**
- Create: `crates/lmnotes-core/src/llm/ollama.rs`

**目标：** Ollama chat（流式）+ embed，对接 `http://localhost:11434/api/chat` 与 `/api/embeddings`。健康检查 `/api/tags`。

- [ ] **Step 1: 实现 + 测试（mock 测试，不依赖真实 Ollama）**

`ollama.rs`：

```rust
//! Ollama 本地 Provider（默认 http://localhost:11434）。

use super::provider::*;
use crate::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

pub struct OllamaProvider {
    base_url: String,
    client: Client,
}

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: Client::new(),
        }
    }

    pub fn default_local() -> Self {
        Self::new("http://localhost:11434")
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn id(&self) -> &str {
        "ollama"
    }
    fn kind(&self) -> ProviderKind {
        ProviderKind::Local
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::CHAT | Capabilities::EMBED
    }
    async fn health(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.base_url);
        Ok(self.client.get(&url).send().await.map(|r| r.status().is_success()).unwrap_or(false))
    }
}

#[derive(Serialize)]
struct ChatBody {
    model: String,
    messages: Vec<MsgSer>,
    stream: bool,
}
#[derive(Serialize)]
struct MsgSer {
    role: String,
    content: String,
}
#[derive(Deserialize)]
struct ChatChunk {
    message: Option<MsgDe>,
}
#[derive(Deserialize)]
struct MsgDe {
    content: String,
}

#[async_trait]
impl ChatCap for OllamaProvider {
    async fn chat_stream(
        &self,
        req: ChatRequest,
    ) -> Result<Box<dyn futures_util::Stream<Item = Result<String>> + Send + Unpin>> {
        let url = format!("{}/api/chat", self.base_url);
        let body = ChatBody {
            model: req.model,
            messages: req
                .messages
                .into_iter()
                .map(|m| MsgSer {
                    role: match m.role {
                        ChatRole::System => "system".into(),
                        ChatRole::User => "user".into(),
                        ChatRole::Assistant => "assistant".into(),
                    },
                    content: m.content,
                })
                .collect(),
            stream: true,
        };
        let resp = self.client.post(&url).json(&body).send().await?;
        let byte_stream = resp.bytes_stream();
        // 逐行解析 NDJSON
        let stream = byte_stream
            .map(|chunk| chunk.map_err(crate::CoreError::Io))
            .scan(String::new(), |buf, chunk| {
                let chunk = chunk?;
                buf.push_str(&String::from_utf8_lossy(&chunk));
                let mut lines_out = Vec::new();
                while let Some(idx) = buf.find('\n') {
                    let line: String = buf.drain(..=idx).collect();
                    if let Ok(c) = serde_json::from_str::<ChatChunk>(line.trim()) {
                        if let Some(m) = c.message {
                            lines_out.push(Ok(m.content));
                        }
                    }
                }
                std::future::ready(Some(futures_util::stream::iter(lines_out)))
            })
            .flatten();
        Ok(Box::new(stream))
    }
}

#[derive(Serialize)]
struct EmbedBody {
    model: String,
    prompt: String,
}
#[derive(Deserialize)]
struct EmbedResp {
    embedding: Vec<f32>,
}

#[async_trait]
impl EmbedCap for OllamaProvider {
    async fn embed(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/api/embeddings", self.base_url);
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            let body = EmbedBody {
                model: model.into(),
                prompt: t.clone(),
            };
            let r: EmbedResp = self.client.post(&url).json(&body).send().await?.json().await?;
            out.push(r.embedding);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_url_is_localhost() {
        let p = OllamaProvider::default_local();
        assert_eq!(p.kind(), ProviderKind::Local);
        assert!(p.capabilities().contains(Capabilities::CHAT | Capabilities::EMBED));
    }
}
```

- [ ] **Step 2: 跑测试 + 编译**

Run: `cargo test -p lmnotes-core llm::ollama`
Expected: 1 个测试 PASS（健康检查依赖真实 Ollama，CI 中跳过——加 `#[ignore]` 标注的集成测试单独跑）。

- [ ] **Step 3: Commit**

```bash
git add crates/lmnotes-core/
git commit -m "feat(llm): Ollama provider (streaming chat + embeddings)"
```

---

## Task 3: OpenAI 兼容 Provider

**Files:**
- Create: `crates/lmnotes-core/src/llm/openai.rs`

**目标：** 任意 OpenAI 兼容端点（GLM / OpenAI / 自建）。`/v1/chat/completions`（stream）+ `/v1/embeddings`。API key 从配置注入（密钥存 OS 钥匙串留 M2，M1b 暂用配置文件）。

- [ ] **Step 1: 实现**

`openai.rs`（结构同 ollama.rs，URL 模板 `/v1/chat/completions`，header 加 `Authorization: Bearer <key>`，解析 SSE `data: {...}` 行）。约 120 行，模板与 ollama 类似，差异在请求/响应 JSON 结构与 SSE 解析。

- [ ] **Step 2: 测试（mock server）**

用 `wiremock` 或 `httpmock` 起一个假端点验证请求格式与流解析。加 `wiremock = "0.6"` 到 dev-dependencies。

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(llm): OpenAI-compatible provider (GLM/OpenAI/self-hosted)"
```

---

## Task 4: Routing（任务→Provider 分派）

**Files:**
- Create: `crates/lmnotes-core/src/llm/routing.rs`

**目标：** 全局 Routing 配置（每个任务槽位指向 Provider+模型）+ 能力探测 + 降级回退。

- [ ] **Step 1: 实现 + 测试**

```rust
//! 任务→Provider 路由（ADR-0005 §3）。

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
}

#[derive(Debug, Clone)]
pub struct ProviderRef {
    pub provider_id: String,
    pub model: String,
}

#[derive(Debug, Clone, Default)]
pub struct Routing {
    pub map: HashMap<Task, ProviderRef>,
}

pub struct Registry {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
}

impl Registry {
    pub fn new() -> Self {
        Self { providers: HashMap::new() }
    }
    pub fn register(&mut self, p: Arc<dyn LlmProvider>) {
        self.providers.insert(p.id().to_string(), p);
    }
    pub fn get(&self, id: &str) -> Option<Arc<dyn LlmProvider>> {
        self.providers.get(id).cloned()
    }
    /// 按任务取 chat provider（能力探测）。
    pub fn chat_for(&self, routing: &Routing, task: Task) -> Result<Arc<dyn ChatCap>> {
        let pref = routing.map.get(&task).ok_or_else(|| {
            crate::CoreError::Conformance(format!("no provider for task {task:?}"))
        })?;
        let p = self.get(&pref.provider_id).ok_or_else(|| {
            crate::CoreError::Conformance(format!("provider {} not registered", pref.provider_id))
        })?;
        if !p.capabilities().contains(Capabilities::CHAT) {
            return Err(crate::CoreError::Conformance(format!(
                "provider {} lacks chat capability", pref.provider_id
            )));
        }
        // 能力匹配：downcast 到 ChatCap。
        // Arc<dyn LlmProvider> → Arc<dyn ChatCap> 需 Provider 同时实现两 trait；
        // 用 Any 不优雅——改为 Registry 存 Arc<dyn Any> 并按需 downcast，或
        // 让所有 Provider 也注册为 ChatCap。**简化方案**：Registry 同时存 chat providers。
        unimplemented!("见 Step 2 调整")
    }
}
```

> **设计调整（执行时细化）：** `Arc<dyn LlmProvider>` 无法直接 downcast 到 `Arc<dyn ChatCap>`。两种方案：
> - A：`Registry` 维护两个 map（`providers: HashMap<id, Arc<dyn LlmProvider>>` + `chats: HashMap<id, Arc<dyn ChatCap>>`），register 时两个都填。
> - B：用 `dyn Any` + downcast。
> **选 A**（类型安全，无 Any）。实现时 register 接收具体类型 Provider，内部 clone 到两个 map。

- [ ] **Step 2: 应用方案 A 重写 Registry**

```rust
pub struct Registry {
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    chats: HashMap<String, Arc<dyn ChatCap>>,
    embeds: HashMap<String, Arc<dyn EmbedCap>>,
}

impl Registry {
    pub fn register_chat<P>(&mut self, p: P)
    where
        P: LlmProvider + ChatCap + 'static,
    {
        let id = p.id().to_string();
        let arc = Arc::new(p);
        self.chats.insert(id.clone(), arc.clone());
        self.providers.insert(id, arc);
    }
    pub fn chat_for(&self, routing: &Routing, task: Task) -> Result<Arc<dyn ChatCap>> {
        let pref = routing.map.get(&task)
            .ok_or_else(|| crate::CoreError::Conformance(format!("no provider for task {task:?}")))?;
        self.chats.get(&pref.provider_id).cloned()
            .ok_or_else(|| crate::CoreError::Conformance(
                format!("provider {} not registered as chat", pref.provider_id)))
    }
}
```

- [ ] **Step 3: 测试（mock provider）**

```rust
#[cfg(test)]
mod tests {
    use super::*;
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
        async fn chat_stream(&self, _: ChatRequest) -> Result<Box<dyn futures_util::Stream<Item = Result<String>> + Send + Unpin>> {
            Ok(Box::new(futures_util::stream::iter(vec![Ok("hi".into())])))
        }
    }
    #[test]
    fn routing_resolves_chat() {
        let mut reg = Registry::new();
        reg.register_chat(FakeChat);
        let routing = Routing { map: [(Task::Summarize, ProviderRef { provider_id: "fake".into(), model: "m".into() })].into_iter().collect() };
        let p = reg.chat_for(&routing, Task::Summarize);
        assert!(p.is_ok());
    }
    #[test]
    fn missing_task_errors() {
        let reg = Registry::new();
        let routing = Routing::default();
        assert!(reg.chat_for(&routing, Task::Chat).is_err());
    }
}
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(llm): routing registry with capability-based resolution"
```

---

## Task 5: 三层隐私护栏

**Files:**
- Create: `crates/lmnotes-core/src/llm/guard.rs`

**目标：** dispatch 入口强制三层检查（ADR-0005 §4）：concept `llm_local_only` 标记 + 敏感关键词 + 云端全局授权。

- [ ] **Step 1: 实现 + 测试**

```rust
//! 三层隐私护栏（ADR-0005 §4）。dispatch 入口强制。

use super::provider::ProviderKind;

#[derive(Debug, Clone, Default)]
pub struct GuardConfig {
    pub cloud_allowed: bool,                  // 全局授权（默认 false）
    pub sensitive_patterns: Vec<String>,      // 正则/关键词
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    Allow,
    Deny(String),  // 原因
}

/// 检查某次调用的护栏。
pub fn check(
    cfg: &GuardConfig,
    provider_kind: ProviderKind,
    content: &str,
    local_only: bool,
) -> GuardDecision {
    // 第一层：concept 标记 local_only 且 provider 是 cloud
    if local_only && provider_kind == ProviderKind::Cloud {
        return GuardDecision::Deny("concept marked local_only, cloud provider not allowed".into());
    }
    // 第二层：敏感关键词命中且 provider 是 cloud
    if provider_kind == ProviderKind::Cloud {
        for pat in &cfg.sensitive_patterns {
            if content.contains(pat.as_str()) {
                return GuardDecision::Deny(format!("sensitive pattern matched: {pat}"));
            }
        }
        // 第三层：云端全局授权
        if !cfg.cloud_allowed {
            return GuardDecision::Deny("cloud providers not globally authorized".into());
        }
    }
    GuardDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> GuardConfig { GuardConfig { cloud_allowed: true, sensitive_patterns: vec!["密码".into()] } }

    #[test]
    fn local_provider_always_allowed() {
        assert_eq!(check(&cfg(), ProviderKind::Local, "密码 123", true), GuardDecision::Allow);
    }
    #[test]
    fn local_only_blocks_cloud() {
        assert_eq!(check(&GuardConfig::default(), ProviderKind::Cloud, "x", true), GuardDecision::Deny("concept marked local_only, cloud provider not allowed".into()));
    }
    #[test]
    fn sensitive_blocks_cloud() {
        assert_eq!(check(&cfg(), ProviderKind::Cloud, "我的密码是", false), GuardDecision::Deny("sensitive pattern matched: 密码".into()));
    }
    #[test]
    fn cloud_unauthorized_blocks() {
        let mut c = cfg(); c.cloud_allowed = false; c.sensitive_patterns.clear();
        assert!(matches!(check(&c, ProviderKind::Cloud, "x", false), GuardDecision::Deny(_)));
    }
    #[test]
    fn cloud_authorized_cleans_content_allowed() {
        assert_eq!(check(&cfg(), ProviderKind::Cloud, "hello world", false), GuardDecision::Allow);
    }
}
```

- [ ] **Step 2: Commit**

```bash
git commit -m "feat(llm): three-layer privacy guard (local_only + sensitive + cloud auth)"
```

---

## Task 6: 建议类型 + 队列

**Files:**
- Create: `crates/lmnotes-core/src/llm/suggestion.rs`

**目标：** 建议数据结构（摘要/标签/链接建议）+ 队列（存 SQLite suggestions 表）+ 接受/拒绝/回滚。

- [ ] **Step 1: schema 扩展 + 类型**

在 `index/schema.rs` 加：
```rust
pub const CREATE_SUGGESTIONS: &str = "
CREATE TABLE IF NOT EXISTS suggestions (
    id          TEXT PRIMARY KEY,
    concept_id  TEXT NOT NULL,
    kind        TEXT NOT NULL,        -- summary | tag | link
    payload     TEXT NOT NULL,        -- JSON
    status      TEXT NOT NULL,        -- pending | accepted | rejected
    created_at  INTEGER NOT NULL,
    applied_at  INTEGER
);
CREATE INDEX IF NOT EXISTS idx_sugg_concept ON suggestions(concept_id);
CREATE INDEX IF NOT EXISTS idx_sugg_status ON suggestions(status);
";
```

`suggestion.rs`：
```rust
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionRecord {
    pub id: String,
    pub concept_id: String,
    pub suggestion: Suggestion,
    pub status: SuggestionStatus,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionStatus { Pending, Accepted, Rejected }
```

队列 CRUD 方法加到 SqliteIndex（或单独 SuggestionStore trait）。建议接受时写回 concept frontmatter/description（ADR-0001），记录 applied_at。

- [ ] **Step 2: 测试（队列 round-trip）**

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(llm): suggestion types + queue (pending/accepted/rejected)"
```

---

## Task 7: 索引器接 LLM（摘要/标签/链接建议生成）

**Files:**
- Modify: `crates/lmnotes-core/src/indexer/mod.rs`

**目标：** concept 保存后异步触发：① 摘要（写入建议队列）② 标签建议 ③ 基于全文相似度找候选笔记 → LLM 判断是否值得建链。向量写入 sqlite-vec（embed）。

- [ ] **Step 1: 扩展 indexer**

indexer 持有 `Registry`/`Routing`/`GuardConfig`。`index_concept` 后 spawn `generate_suggestions(c)`：
- 摘要：`chat` provider，prompt "用一句话总结"，结果作为 Suggestion::Summary 入队。
- 标签：prompt "提取 3-5 个标签"。
- 链接：向量相似 top-5（首次需 embed 该 concept）→ LLM 判断关联性 → 命中则 Suggestion::Link。
- embed：用 embed provider 把 concept body 向量化，写入 sqlite-vec。

- [ ] **Step 2: 向量写入 sqlite-vec**

SqliteIndex 加 `upsert_vector(id, embedding)`：
```rust
pub fn upsert_vector(&self, id: &str, embedding: &[f32]) -> Result<()> {
    let conn = self.conn.lock().unwrap();
    let ser: String = format!("[{}]", embedding.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
    conn.execute("INSERT OR REPLACE INTO vec_concepts (id, embedding) VALUES (?1, ?2)", [id, ser])?;
    Ok(())
}
pub fn vector_search(&self, q: &[f32], k: usize) -> Result<Vec<(String, f32)>> {
    // KNN 查询
    let conn = self.conn.lock().unwrap();
    let ser = format!("[{}]", q.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(","));
    let mut stmt = conn.prepare(&format!("SELECT id, distance FROM vec_concepts WHERE embedding MATCH ?1 ORDER BY distance LIMIT {k}"))?;
    let rows = stmt.query_map([&ser], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f32>(1)?)))?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}
```

- [ ] **Step 3: 测试（mock provider 生成建议）**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(indexer): LLM-backed suggestion generation (summary/tag/link + vector embed)"
```

---

## Task 8: 建议中心 UI

**Files:**
- Create: `apps/desktop/src/suggestions/SuggestionCenter.tsx`
- Modify: `apps/desktop/src/App.tsx`（右侧面板改显示建议）
- Modify: `apps/desktop/src-tauri/src/commands.rs`（建议查询/接受/拒绝命令）

**目标：** 右侧面板列出 pending 建议，J/K 选择，Enter 接受，D 拒绝，支持批量。

- [ ] **Step 1: Tauri 命令**

```rust
#[tauri::command]
pub async fn list_suggestions(store: State<'_, Arc<SuggestionStore>>) -> Result<Vec<SuggestionRecord>, String> {
    store.list_pending().await.map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn accept_suggestion(id: String, ...) -> Result<(), String> { ... }  // 写回 concept + 回滚快照
#[tauri::command]
pub async fn reject_suggestion(id: String, ...) -> Result<(), String> { ... }
```

- [ ] **Step 2: SuggestionCenter.tsx**

键盘驱动列表，diff 预览（summary/tag 显示文本，link 显示 `[text](path)`），操作经 IPC。

- [ ] **Step 3: 端到端验证**

写笔记 → 等 ~10s（LLM 生成）→ 建议中心出现 pending → 接受 → concept frontmatter/description 更新。

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(ui): suggestion center (review/accept/reject with keyboard)"
```

---

## Task 9: 就地改写 + 撤销

**Files:**
- Create: `apps/desktop/src/editor/RewriteMenu.tsx`
- Modify: `apps/desktop/src-tauri/src/commands.rs`（rewrite 命令 + 快照）

**目标：** 选中正文 → 右键/快捷键菜单（润色/扩写/翻译/总结要点）→ LLM 流式返回 → 替换选区，Cmd+Z 撤销（快照存 `.lmnotes/llm/snapshots/`，ADR-0001 合规要求）。

- [ ] **Step 1: rewrite 命令（流式）**

```rust
#[tauri::command]
pub async fn rewrite_selection(
    action: String,   // polish | expand | translate | summarize
    selection: String,
    language: Option<String>,
    chat: State<'_, Arc<dyn ChatCap>>,
    // ...
) -> Result<...> { ... }
```

返回前先存快照（concept_id + 旧文本 + 时间戳 → `.lmnotes/llm/snapshots/<id>-<ts>.md`）。

- [ ] **Step 2: RewriteMenu.tsx**

CodeMirror 选区 → 菜单 → 流式插入（经 Tauri events）→ 完成后保存触发重索引。

- [ ] **Step 3: 撤销**

undo 走 CodeMirror 原生 history（短期）+ 快照文件（跨会话，FR-LLM-09）。

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(editor): inline rewrite (polish/expand/translate/summarize) with rollback"
```

---

## Task 10: Provider 配置 UI

**Files:**
- Create: `apps/desktop/src/settings/ProviderSettings.tsx`
- Modify: `apps/desktop/src-tauri/src/commands.rs`（配置读写）

**目标：** 设置面板：列表 Providers、按任务槽位下拉选 Provider+模型、测试连通（health）、全局云端授权开关、敏感关键词清单。

- [ ] **Step 1: 配置文件 `~/.lmnotes/config.json`**

```json
{
  "providers": [
    { "id": "ollama", "kind": "local", "base_url": "http://localhost:11434", "chat_model": "qwen3:8b", "embed_model": "nomic-embed-text" }
  ],
  "routing": { "summarize": {"provider": "ollama", "model": "qwen3:8b"}, ... },
  "guard": { "cloud_allowed": false, "sensitive_patterns": [] }
}
```

- [ ] **Step 2: UI**

SolidJS 表单，保存写 config.json，重启生效（或热加载 Registry）。

- [ ] **Step 3: 首次启动探测（O6c）**

应用首次启动检测 Ollama health：可用→默认本地；不可用→引导配置云端 Provider（弹窗告知隐私含义）。

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(settings): provider configuration UI + first-run detection"
```

---

## M1b 退出标准

- [ ] LLM Provider 抽象（trait 拆分）+ Ollama/OpenAI 兼容实现可用
- [ ] 路由按任务分派，能力探测 + 缺能力降级
- [ ] 三层护栏强制（有测试），云端默认禁用
- [ ] 保存笔记 → LLM 生成摘要/标签/链接建议入队
- [ ] 建议中心 UI：审阅/接受/拒绝/批量，键盘驱动
- [ ] 就地改写（4 种动作）+ 撤销（快照）
- [ ] Provider 配置 UI + 首次启动探测
- [ ] **向量层**：concept embed 写入 sqlite-vec，相似查询可用
- [ ] CI 全绿（含 mock provider 测试）
- [ ] **§13 B 组闭环**：写笔记 → LLM 建链接 → 接受 → 搜索 → 改写 → 撤销

---

## Self-Review（M1b）

**覆盖性：** FR-LLM-01/02/03/05/08/09、FR-MODEL-01/02/03/04 全覆盖。FR-LLM-04（图谱问答）→ M1c。FR-LLM-06（行动项）/07（每日回顾）→ M2。

**占位符：** T3 OpenAI 实现、T6 队列 CRUD、T7 generate_suggestions、T9 rewrite 的具体 prompt 给了模板但非逐行——这些是中等复杂度实现，计划给了**结构 + 签名 + 测试要求 + 关键算法**，执行时按测试驱动填充。这是合理的计划粒度（非纯机械任务不强制逐行代码）。

**类型一致性：** `Suggestion`/`SuggestionRecord`/`SuggestionStatus`、`Task`/`Routing`/`Registry`、`GuardDecision` 跨任务签名一致。`Registry` 双 map 方案在 T4 内部自我修正。

**依赖顺序：** T1→T2→T3（trait→实现）、T4→T7（routing→indexer）、T5 全局护栏在 T7 前就绪、T6→T8（队列→UI）、T9/T10 独立。顺序合理。
