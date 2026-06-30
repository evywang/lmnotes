//! lmnotes-mcp: 把 LMNotes vault 只读暴露给 AI agent（MCP server）。
//!
//! 设计原则（ADR-0002 一致）：本 crate 只依赖 [`lmnotes-core`] + [`rmcp`]，
//! **transport 无关**——只定义 `ServerHandler` 与工具，不 import 任何 Tauri 类型。
//! 当前挂到 streamable HTTP transport（见 [`server`] 模块）；将来若需补独立 stdio
//! 二进制，仅需 10 行胶水，本文件零改动。
//!
//! 能力范围：**只读**。5 个工具：
//! - `search_notes` —— 全文检索（BM25 + 元数据富化，同桌面 `search` 命令）
//! - `read_note` —— 读单条笔记原文（markdown）
//! - `list_notes` —— 列 vault 目录树（跳过 `.lmnotes/`）
//! - `ask_vault` —— RAG 问答（向量+全文 RRF → 拼 context → LLM，同 `chat_stream`）
//! - `get_note_links` —— 反向链接图（谁链接到该笔记）
//!
//! `ask_vault` 沿用桌面 chat 的护栏（`cloud_allowed` / `sensitive_patterns`），
//! 并把流式输出聚合成一次性返回（MCP tool 一次 call 一个 result）。

pub mod server;

use std::sync::Arc;

use lmnotes_core::backend::fs::FsBackend;
use lmnotes_core::backend::{DirEntry, IndexBackend, StorageBackend};
use lmnotes_core::index::sqlite::SqliteIndex;
use lmnotes_core::index::tantivy::TantivyIndex;
use lmnotes_core::llm::guard::{check as guard_check, GuardConfig, GuardDecision};
use lmnotes_core::llm::routing::{Registry, Routing, Task};
use lmnotes_core::llm::{ChatMessage, ChatRequest, ChatRole};
use lmnotes_core::qa::context::build_context;
use lmnotes_core::qa::prompt::SYSTEM;
use lmnotes_core::qa::retriever::Retriever;
use lmnotes_core::search::SearchEngine;
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::{tool, tool_handler, tool_router, Json, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// 请求/响应 DTO（自带 JSON Schema，rmcp 据此生成工具 schema 暴露给 agent）
// ============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchNotesRequest {
    /// 搜索关键词（支持中文分词）。
    pub query: String,
    /// 返回条数上限，默认 20。
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct NoteHit {
    /// vault 相对路径，如 "notes/ai/attention.md"。
    pub path: String,
    /// 笔记标题（取自 frontmatter，可能为空）。
    pub title: Option<String>,
    /// 相关度得分（越高越相关）。
    pub score: f64,
}

/// `search_notes` 响应（MCP 要求 outputSchema 根为 object，故包一层）。
#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchNotesResponse {
    pub hits: Vec<NoteHit>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadNoteRequest {
    /// vault 相对路径，如 "notes/ai/attention.md"。
    pub path: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct NoteContent {
    /// 笔记原文（含 YAML frontmatter + markdown 正文）。
    pub text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListNotesRequest {
    /// 起始子目录（vault 相对）；缺省为 vault 根。
    #[serde(default)]
    pub rel_path: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct NoteTree {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<NoteTree>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AskVaultRequest {
    /// 向 vault 提出的问题。
    pub query: String,
    /// 可选的多轮对话历史（最近 20 条参与上下文）。
    #[serde(default)]
    pub history: Vec<HistoryMsg>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct HistoryMsg {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AskVaultResponse {
    /// LLM 基于检索到的笔记给出的回答。
    pub answer: String,
    /// 回答所依据的笔记引用（编号见 answer 中的 [n]）。
    pub citations: Vec<Citation>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Citation {
    pub index: usize,
    pub path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetNoteLinksRequest {
    /// 目标笔记的 vault 相对路径，如 "notes/ai/attention.md"。
    pub path: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Backlink {
    /// 链接到目标笔记的源笔记路径。
    pub src_path: String,
    /// 链接的显示文本（可能为空）。
    pub link_text: Option<String>,
}

/// `get_note_links` 响应（MCP 要求 outputSchema 根为 object，故包一层）。
#[derive(Debug, Serialize, JsonSchema)]
pub struct GetNoteLinksResponse {
    pub backlinks: Vec<Backlink>,
}

// ============================================================================
// Server：持有桌面进程已构造的同一组 Arc 资源（零拷贝共享，无跨进程锁）
// ============================================================================

/// LMNotes 的 MCP 服务（transport 无关）。
///
/// 字段全部是 `Arc`，可与桌面进程共享同一份已打开的 SQLite / Tantivy 句柄，
/// 避免跨进程并发写导致的锁竞争（ADR-0003）。
#[derive(Clone)]
pub struct LmnotesMcpServer {
    vault_root: Arc<PathBuf>,
    engine: Arc<SearchEngine>,
    meta: Arc<dyn IndexBackend>,
    sqlite: Arc<SqliteIndex>,
    fulltext: Arc<TantivyIndex>,
    registry: Arc<Registry>,
    routing: Arc<Routing>,
    guard_cfg: Arc<GuardConfig>,
    tool_router: ToolRouter<Self>,
}

impl LmnotesMcpServer {
    /// 用桌面已构造的资源组装 MCP 服务。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        vault_root: PathBuf,
        engine: Arc<SearchEngine>,
        meta: Arc<dyn IndexBackend>,
        sqlite: Arc<SqliteIndex>,
        fulltext: Arc<TantivyIndex>,
        registry: Arc<Registry>,
        routing: Arc<Routing>,
        guard_cfg: Arc<GuardConfig>,
    ) -> Self {
        Self {
            vault_root: Arc::new(vault_root),
            engine,
            meta,
            sqlite,
            fulltext,
            registry,
            routing,
            guard_cfg,
            tool_router: Self::tool_router(),
        }
    }

    fn backend(&self) -> FsBackend {
        FsBackend::new(self.vault_root.as_path())
    }
}

// ----------------------------------------------------------------------------
// 工具实现（全部只读）
// ----------------------------------------------------------------------------

#[tool_router(router = tool_router)]
impl LmnotesMcpServer {
    /// 全文检索笔记（BM25，支持中文分词）。
    #[tool(
        name = "search_notes",
        description = "全文检索笔记。返回匹配笔记的路径、标题与相关度。"
    )]
    pub async fn search_notes(
        &self,
        Parameters(req): Parameters<SearchNotesRequest>,
    ) -> Result<Json<SearchNotesResponse>, String> {
        let limit = req.limit.unwrap_or(20).min(200);
        let hits = self
            .engine
            .search(&req.query, limit)
            .map_err(|e| e.to_string())?;
        Ok(Json(SearchNotesResponse {
            hits: hits
                .into_iter()
                .map(|h| NoteHit {
                    path: h.path,
                    title: h.title,
                    score: h.score,
                })
                .collect(),
        }))
    }

    /// 读取单条笔记原文（含 frontmatter + markdown 正文）。
    #[tool(
        name = "read_note",
        description = "按 vault 相对路径读取一条笔记的完整原文（markdown）。"
    )]
    pub async fn read_note(
        &self,
        Parameters(req): Parameters<ReadNoteRequest>,
    ) -> Result<Json<NoteContent>, String> {
        let bytes = self
            .backend()
            .read_file(&req.path)
            .await
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        Ok(Json(NoteContent { text }))
    }

    /// 列出 vault 目录树（递归，跳过 `.lmnotes/` 与隐藏项）。
    #[tool(
        name = "list_notes",
        description = "列出 vault 目录树（递归）。跳过 .lmnotes/ 与隐藏文件，仅含 .md 笔记。"
    )]
    pub async fn list_notes(
        &self,
        Parameters(req): Parameters<ListNotesRequest>,
    ) -> Result<Json<NoteTree>, String> {
        let rel = req.rel_path.unwrap_or_else(|| ".".into());
        let tree = self
            .list_dir_recursive(&rel)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Json(tree))
    }

    /// 基于检索到的笔记回答问题（RAG）。
    ///
    /// 复刻桌面 `chat_stream`：向量 KNN + 全文 BM25（RRF 融合）→ 拼 context
    /// （6000 字预算）→ 护栏检查 → LLM 流式输出聚合为整段返回。
    /// 流式 chunk 聚合为一次性返回（MCP tool 一次 call 一个 result）。
    #[tool(
        name = "ask_vault",
        description = "基于检索到的笔记回答问题（RAG）。返回回答正文与引用的笔记。需要已配置可用的 LLM provider。"
    )]
    pub async fn ask_vault(
        &self,
        Parameters(req): Parameters<AskVaultRequest>,
    ) -> Result<Json<AskVaultResponse>, String> {
        use futures_util::StreamExt;

        // 存用户消息到历史（与桌面 chat 一致）
        let _ = self.sqlite.append_chat_history("user", &req.query, None);

        // 1. 取 embed provider + 检索
        let (embedder, embed_model) = self
            .registry
            .embed_for(&self.routing, Task::Embed)
            .map_err(|e| e.to_string())?;
        let retriever = Retriever::new(
            self.meta.clone(),
            self.fulltext.clone(),
            self.sqlite.clone(),
            embedder,
            embed_model,
        );
        let chunks = retriever
            .retrieve(&req.query, 5)
            .await
            .map_err(|e| e.to_string())?;
        let (ctx, citations) = build_context(&chunks, 6000);

        // 2. 取 chat provider
        let (chat, model) = self
            .registry
            .chat_for(&self.routing, Task::Chat)
            .map_err(|e| e.to_string())?;

        // 3. 护栏检查（与 chat_stream 一致）
        let full_input = format!("{SYSTEM}\n\n【上下文】\n{ctx}\n\n【问题】\n{}", req.query);
        match guard_check(&self.guard_cfg, chat.kind(), &full_input, false) {
            GuardDecision::Allow => {}
            GuardDecision::Deny(reason) => return Err(reason),
        }

        // 4. 构建 messages：system（含 context）+ 历史（最近 20 条）+ 当前 query
        let mut messages = vec![ChatMessage {
            role: ChatRole::System,
            content: format!("{SYSTEM}\n\n【上下文】\n{ctx}"),
        }];
        for h in req.history.iter().rev().take(20).rev() {
            messages.push(ChatMessage {
                role: match h.role.as_str() {
                    "user" => ChatRole::User,
                    "assistant" => ChatRole::Assistant,
                    _ => ChatRole::User,
                },
                content: h.content.clone(),
            });
        }
        let chat_req = ChatRequest {
            model,
            messages,
            temperature: Some(0.4),
        };

        // 5. 流式 chat → 聚合为整段（MCP tool 一次返回）
        let mut stream = chat
            .chat_stream(chat_req)
            .await
            .map_err(|e| e.to_string())?;
        let mut full_answer = String::new();
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(text) => full_answer.push_str(&text),
                Err(e) => eprintln!("ask_vault stream error: {e}"),
            }
        }

        // 6. 存回答到历史 + 返回引用
        let cite_json = if citations.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&citations).unwrap_or_default())
        };
        let _ = self
            .sqlite
            .append_chat_history("assistant", &full_answer, cite_json.as_deref());

        Ok(Json(AskVaultResponse {
            answer: full_answer,
            citations: citations
                .into_iter()
                .map(|c| Citation {
                    index: c.index,
                    path: c.path,
                })
                .collect(),
        }))
    }

    /// 反向链接：哪些笔记链接到了目标笔记。
    #[tool(
        name = "get_note_links",
        description = "查询反向链接：哪些笔记链接到了目标笔记。便于遍历知识图谱。"
    )]
    pub async fn get_note_links(
        &self,
        Parameters(req): Parameters<GetNoteLinksRequest>,
    ) -> Result<Json<GetNoteLinksResponse>, String> {
        // path → concept id（id 优先取 frontmatter.id，缺省为 path 本身）
        let id = self
            .meta
            .get_concept_by_path(&req.path)
            .map_err(|e| e.to_string())?
            .map(|row| row.id)
            .unwrap_or(req.path);
        let edges = self.meta.backrefs(&id).map_err(|e| e.to_string())?;
        // src_id → src_path 富化
        let mut backlinks = Vec::with_capacity(edges.len());
        for e in edges {
            let src_path = self
                .meta
                .get_concept(&e.src_id)
                .ok()
                .flatten()
                .map(|r| r.path)
                .unwrap_or(e.src_id);
            backlinks.push(Backlink {
                src_path,
                link_text: e.link_text,
            });
        }
        Ok(Json(GetNoteLinksResponse { backlinks }))
    }

    // ---- list_notes 的目录递归实现（复刻 commands::list_dir_recursive 规则）----
    async fn list_dir_recursive(&self, rel: &str) -> Result<NoteTree, lmnotes_core::CoreError> {
        let backend = self.backend();
        let entries = backend.list_dir(rel).await?;
        let children = build_tree(&backend, &entries).await;
        // 顶层虚拟根：name 取最后一段（或 "/"），path 取归一化 rel
        let root_name = rel
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty() && *s != ".")
            .unwrap_or("/")
            .to_string();
        Ok(NoteTree {
            name: root_name,
            path: rel.trim_end_matches('/').to_string(),
            is_dir: true,
            children,
        })
    }
}

/// 递归构建目录树（跳过 `.lmnotes/` 与隐藏项；仅保留 .md；目录在前）。
///
/// 因 async fn 不能直接递归（future 大小未知），用 `Box::pin` 引入间接层。
fn build_tree<'a>(
    backend: &'a FsBackend,
    entries: &'a [DirEntry],
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<NoteTree>> + Send + 'a>> {
    Box::pin(async move {
        let mut nodes = Vec::with_capacity(entries.len());
        for e in entries {
            let name = e.path.rsplit('/').next().unwrap_or(&e.path).to_string();
            if name.starts_with('.') {
                continue; // 跳过隐藏项 + .lmnotes
            }
            if e.is_dir {
                let child_entries = match backend.list_dir(&e.path).await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let children = build_tree(backend, &child_entries).await;
                nodes.push(NoteTree {
                    name,
                    path: e.path.clone(),
                    is_dir: true,
                    children,
                });
            } else if name.ends_with(".md") {
                nodes.push(NoteTree {
                    name,
                    path: e.path.clone(),
                    is_dir: false,
                    children: vec![],
                });
            }
        }
        // 目录在前，文件在后；同类按名排序（与 commands 一致）
        nodes.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        nodes
    })
}

// ----------------------------------------------------------------------------
// ServerHandler：声明能力 + 委托 tool_router
// ----------------------------------------------------------------------------

#[tool_handler(router = self.tool_router)]
impl ServerHandler for LmnotesMcpServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        use rmcp::model::*;
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                "lmnotes-mcp",
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "只读访问 LMNotes vault（笔记知识库）。可用工具：search_notes 全文检索、\
             read_note 读单条笔记原文、list_notes 列目录树、ask_vault 基于 RAG 问答、\
             get_note_links 查反向链接。所有工具均为只读，不会修改笔记。"
                    .to_string(),
            )
    }
}

#[cfg(test)]
mod tests {
    //! 非破坏性集成测试：用 tempdir vault + 真实 SQLite/Tantivy 索引，
    //! 验证 4 个只读工具（不含 ask_vault，后者依赖 LLM provider，已在桌面 chat_stream 覆盖）。

    use super::*;
    use lmnotes_core::indexer::Indexer;
    use lmnotes_core::llm::guard::GuardConfig;
    use lmnotes_core::llm::routing::{Registry, Routing};
    use lmnotes_core::okf::concept::Concept;
    use std::sync::Arc;

    /// 构造测试 server：temp vault + 已索引的笔记 + 空 Registry（非 LLM 工具不需要）。
    async fn make_server(files: &[(&str, &str)]) -> (LmnotesMcpServer, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let lmnotes_dir = root.join(".lmnotes");
        std::fs::create_dir_all(&lmnotes_dir).unwrap();

        // 一个 Arc<SqliteIndex> 同时充当 Arc<dyn IndexBackend>（trait 强转）与 Arc<SqliteIndex>。
        let sqlite = Arc::new(SqliteIndex::open(lmnotes_dir.join("index.sqlite")).unwrap());
        sqlite.init_schema_with_vec_dim(768).await.unwrap();
        let fulltext = Arc::new(TantivyIndex::open(lmnotes_dir.join("tantivy")).unwrap());
        let meta: Arc<dyn IndexBackend> = sqlite.clone();
        let indexer = Indexer::new(meta.clone(), fulltext.clone());

        let backend = FsBackend::new(&root);
        for (rel, text) in files {
            // 用 FsBackend 写笔记（与 server 同款沙箱写入，避免触发 std::fs 禁用规则）
            backend.write_file(rel, text.as_bytes()).await.unwrap();
            if let Ok(c) = Concept::parse(text) {
                indexer.index_concept(rel, text, &c).await.unwrap();
            }
        }

        let engine = Arc::new(SearchEngine::new(meta.clone(), fulltext.clone()));
        let registry = Arc::new(Registry::new());
        let routing = Arc::new(Routing::default());
        let guard = Arc::new(GuardConfig::default());

        let server = LmnotesMcpServer::new(
            root.clone(),
            engine,
            meta,
            sqlite,
            fulltext,
            registry,
            routing,
            guard,
        );
        (server, tmp)
    }

    #[tokio::test]
    async fn search_notes_returns_hits() {
        let (server, _tmp) = make_server(&[
            (
                "notes/a.md",
                "---\ntype: note\nid: nt_1\ntitle: 注意力机制\n---\n\n注意力是稀缺资源\n",
            ),
            (
                "notes/b.md",
                "---\ntype: note\nid: nt_2\ntitle: 无关内容\n---\n\n天气不错\n",
            ),
        ])
        .await;

        let Json(resp) = server
            .search_notes(Parameters(SearchNotesRequest {
                query: "注意力".into(),
                limit: Some(10),
            }))
            .await
            .unwrap();
        assert!(resp.hits.iter().any(|h| h.path == "notes/a.md"));
    }

    #[tokio::test]
    async fn read_note_returns_text() {
        let (server, _tmp) = make_server(&[(
            "notes/x.md",
            "---\ntype: note\nid: nt_x\ntitle: T\n---\n\n正文内容\n",
        )])
        .await;

        let Json(content) = server
            .read_note(Parameters(ReadNoteRequest {
                path: "notes/x.md".into(),
            }))
            .await
            .unwrap();
        assert!(content.text.contains("正文内容"));
        assert!(content.text.contains("type: note"));
    }

    #[tokio::test]
    async fn list_notes_skips_hidden_and_lmnotes() {
        let (server, _tmp) = make_server(&[
            ("notes/a.md", "---\ntype: note\n---\n\na\n"),
            ("notes/sub/b.md", "---\ntype: note\n---\n\nb\n"),
            (".secret.md", "---\ntype: note\n---\n\nsecret\n"),
        ])
        .await;

        let Json(tree) = server
            .list_notes(Parameters(ListNotesRequest { rel_path: None }))
            .await
            .unwrap();
        // 收集所有路径
        fn collect(t: &NoteTree, out: &mut Vec<String>) {
            out.push(t.path.clone());
            for c in &t.children {
                collect(c, out);
            }
        }
        let mut paths = Vec::new();
        collect(&tree, &mut paths);
        assert!(paths.iter().any(|p| p.contains("a.md")));
        assert!(paths.iter().any(|p| p.contains("b.md")));
        assert!(!paths.iter().any(|p| p.contains(".secret")));
        assert!(!paths.iter().any(|p| p.contains(".lmnotes")));
    }

    #[tokio::test]
    async fn read_note_rejects_path_traversal() {
        let (server, _tmp) = make_server(&[("a.md", "---\ntype: note\n---\n\na\n")]).await;
        let res = server
            .read_note(Parameters(ReadNoteRequest {
                path: "../../etc/passwd".into(),
            }))
            .await;
        assert!(res.is_err(), "path traversal must be rejected");
    }
}
