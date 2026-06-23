//! 增量索引器：协调 SQLite 元数据 + Tantivy 全文（向量层 M1b 补）。
//! 监听 concept 变更，事务化更新三层。增量：按 content_hash 跳过未变。

use crate::backend::IndexBackend;
use crate::index::schema::{ConceptRow, EdgeRow};
use crate::index::tantivy::TantivyIndex;
use crate::okf::concept::Concept;
use crate::Result;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub struct Indexer {
    pub meta: Arc<dyn IndexBackend>,
    pub fulltext: Arc<TantivyIndex>,
}

impl Indexer {
    pub fn new(meta: Arc<dyn IndexBackend>, fulltext: Arc<TantivyIndex>) -> Self {
        Self { meta, fulltext }
    }

    /// 索引一个 concept（增量：hash 未变则跳过）。
    /// 返回 true 表示确实更新了索引。
    pub async fn index_concept(
        &self,
        rel_path: &str,
        text: &str,
        concept: &Concept,
    ) -> Result<bool> {
        let id = concept
            .frontmatter
            .id
            .clone()
            .unwrap_or_else(|| rel_path.to_string());
        let content_hash = hex_hash(text);
        // 增量检查
        if let Some(existing) = self.meta.get_concept(&id)? {
            if existing.content_hash == content_hash && existing.path == rel_path {
                return Ok(false);
            }
        }
        // 抽取 body 中的 markdown link 作为出边
        let edges = extract_edges(&concept.body);
        let row = ConceptRow {
            id: id.clone(),
            path: rel_path.to_string(),
            type_: concept.frontmatter.type_.clone(),
            title: concept.frontmatter.title.clone(),
            mtime: now_secs(),
            content_hash: content_hash.clone(),
        };
        // 更新 SQLite
        self.meta.upsert_concept(row).await?;
        // 解析出边中的 dst_id：尝试按 path 反查
        let mut resolved: Vec<EdgeRow> = Vec::with_capacity(edges.len());
        for e in edges {
            let dst_id = self.resolve_dst_id(&e.dst_path)?;
            resolved.push(EdgeRow {
                src_id: id.clone(),
                dst_id,
                dst_path: e.dst_path,
                link_text: e.link_text,
            });
        }
        self.meta.replace_edges(&id, resolved).await?;
        // 更新 Tantivy 全文
        self.fulltext.upsert(&id, &concept.body)?;
        Ok(true)
    }

    /// 删除一个 concept 的全部索引数据。
    pub async fn unindex(&self, id: &str) -> Result<()> {
        self.meta.delete_concept(id).await?;
        self.fulltext.delete(id)?;
        Ok(())
    }

    fn resolve_dst_id(&self, dst_path: &str) -> Result<Option<String>> {
        // bundle-relative 路径去 .md 后缀对齐 concept.path（OKF §5.1 绝对链接）
        let normalized = dst_path.trim_start_matches('/').trim_end_matches(".md");
        // 先精确匹配 path，再尝试 path+.md
        if let Some(r) = self.meta.get_concept_by_path(normalized)? {
            return Ok(Some(r.id));
        }
        if let Some(r) = self.meta.get_concept_by_path(&format!("{normalized}.md"))? {
            return Ok(Some(r.id));
        }
        Ok(None)
    }
}

fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn hex_hash(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex::encode(h.finalize())
}

struct RawEdge {
    dst_path: String,
    link_text: Option<String>,
}

/// 仅当 edge 尚无 link_text 且文本非空时填充。
fn set_link_text_if_needed(edge: &mut RawEdge, text: &str) {
    if edge.link_text.is_none() && !text.is_empty() {
        edge.link_text = Some(text.to_string());
    }
}

/// 抽取 body 中的 markdown link（OKF §5）。
/// 仅 bundle-relative（/开头）算内部链接（OKF §5.1）。
fn extract_edges(body: &str) -> Vec<RawEdge> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(body, opts);
    let mut edges = Vec::new();
    let mut in_link = false;
    let mut current_link_text = String::new();
    for event in parser {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                let dest = dest_url.into_string();
                if dest.starts_with('/') {
                    in_link = true;
                    current_link_text.clear();
                    edges.push(RawEdge {
                        dst_path: dest,
                        link_text: None,
                    });
                }
            }
            Event::Text(t) => {
                if !in_link {
                    continue;
                }
                current_link_text.push_str(t.as_ref());
            }
            Event::End(TagEnd::Link) => {
                if !in_link {
                    continue;
                }
                if let Some(last) = edges.last_mut() {
                    set_link_text_if_needed(last, &current_link_text);
                }
                in_link = false;
            }
            _ => {}
        }
    }
    edges
}

// ============ LLM 建议生成（M1b）============

/// 对一个 concept 生成 LLM 建议（摘要 + 标签），写入 suggestion store。
/// 同时把 concept 文本 embed 写入 vec_concepts（为 M1c 链接建议/RAG 服务）。
///
/// 纯异步，由 save_concept 在索引完成后 spawn 调用，不阻塞编辑器。
/// 护栏：每路调用前经 guard::check（concept local_only + 敏感词 + 云端授权）。
/// 错误吞咽（eprintln），不向上传播——单条建议失败不应阻断其他建议。
pub async fn generate_suggestions(
    concept: &Concept,
    concept_path: &str,
    sqlite: &crate::index::SqliteIndex,
    registry: &crate::llm::routing::Registry,
    routing: &crate::llm::routing::Routing,
    guard_cfg: &crate::llm::guard::GuardConfig,
    concept_text: &str,
) -> crate::Result<()> {
    use crate::llm::guard::{check, GuardDecision};
    use crate::llm::routing::Task;
    use crate::llm::suggestion::Suggestion;
    use crate::llm::{ChatMessage, ChatRequest, ChatRole};

    let concept_id = concept
        .frontmatter
        .id
        .clone()
        .unwrap_or_else(|| concept_path.to_string());
    let local_only = concept
        .frontmatter
        .extra
        .get("llm_local_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // 1. 摘要（chat provider）
    if let Ok((chat, model)) = registry.chat_for(routing, Task::Summarize) {
        let kind = chat.kind();
        match check(guard_cfg, kind, concept_text, local_only) {
            GuardDecision::Allow => {
                let req = ChatRequest {
                    model,
                    messages: vec![
                        ChatMessage {
                            role: ChatRole::System,
                            content:
                                "用一句话（≤50字）总结这段笔记的核心内容。只输出总结，不加前缀。"
                                    .into(),
                        },
                        ChatMessage {
                            role: ChatRole::User,
                            content: concept_text.to_string(),
                        },
                    ],
                    temperature: Some(0.3),
                };
                if let Ok(summary) = chat.chat(req).await {
                    let trimmed = summary.trim();
                    if !trimmed.is_empty() {
                        let sid = format!("sg_{}_sum", concept_id);
                        if let Err(e) = sqlite.insert_suggestion(
                            &sid,
                            &concept_id,
                            &Suggestion::Summary {
                                text: trimmed.into(),
                            },
                        ) {
                            eprintln!("summary insert fail {concept_id}: {e}");
                        }
                    }
                }
            }
            GuardDecision::Deny(reason) => {
                eprintln!("guard deny summary for {concept_id}: {reason}")
            }
        }
    }

    // 2. 标签（chat provider）
    if let Ok((chat, model)) = registry.chat_for(routing, Task::LinkSuggest) {
        let kind = chat.kind();
        if matches!(
            check(guard_cfg, kind, concept_text, local_only),
            GuardDecision::Allow
        ) {
            let req = ChatRequest {
                model,
                messages: vec![
                    ChatMessage {
                        role: ChatRole::System,
                        content: "提取这段笔记的 3-5 个标签，每行一个，不加序号或符号。".into(),
                    },
                    ChatMessage {
                        role: ChatRole::User,
                        content: concept_text.to_string(),
                    },
                ],
                temperature: Some(0.3),
            };
            if let Ok(tags_text) = chat.chat(req).await {
                for tag in tags_text
                    .lines()
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .take(5)
                {
                    let sid = format!("sg_{}_tag_{}", concept_id, sanitize(tag));
                    if let Err(e) = sqlite.insert_suggestion(
                        &sid,
                        &concept_id,
                        &Suggestion::Tag { tag: tag.into() },
                    ) {
                        eprintln!("tag insert fail {concept_id}: {e}");
                    }
                }
            }
        }
    }

    // 3. 向量 embed + 写 vec_concepts
    if let Ok((embedder, model)) = registry.embed_for(routing, Task::Embed) {
        let kind = embedder.kind();
        if matches!(
            check(guard_cfg, kind, concept_text, local_only),
            GuardDecision::Allow
        ) {
            match embedder.embed(&model, &[concept_text.to_string()]).await {
                Ok(vectors) => {
                    if let Some(v) = vectors.into_iter().next() {
                        if let Err(e) = sqlite.upsert_vector(&concept_id, &v) {
                            eprintln!("vec insert fail {concept_id}: {e}");
                        }
                    }
                }
                Err(e) => eprintln!("embed fail {concept_id}: {e}"),
            }
        }
    }

    Ok(())
}

/// 标签文本 → 安全的 suggestion id 后缀（字母数字 + _ -，限 20 字符）。
fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .take(20)
        .collect()
}

/// 遍历 vault 目录，对每个合规 concept 增量索引。
/// 用于启动时全量重建（若索引为空）或外部编辑感知重索引。
/// 跳过 parse 失败的文件（Vault::validate 会报告），返回 (已检查数, 已索引数)。
pub async fn walk_and_index(
    indexer: &Indexer,
    backend: &dyn crate::backend::StorageBackend,
    root: &std::path::Path,
) -> (usize, usize) {
    let mut checked = 0usize;
    let mut indexed = 0usize;
    let _ = walk_dir(indexer, backend, root, root, &mut checked, &mut indexed).await;
    (checked, indexed)
}

async fn walk_dir(
    indexer: &Indexer,
    backend: &dyn crate::backend::StorageBackend,
    root: &std::path::Path,
    dir: &std::path::Path,
    checked: &mut usize,
    indexed: &mut usize,
) -> Result<()> {
    use crate::okf::validator::{validate_filename, FileKind};
    let entries = backend.list_dir(&rel(root, dir)).await?;
    for e in entries {
        let full = dir.join(&e.path);
        let name = full
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if e.is_dir {
            // e.path 是相对 vault 根的路径
            let sub = root.join(&e.path);
            // async 递归需 Box::pin
            Box::pin(walk_dir(indexer, backend, root, &sub, checked, indexed)).await?;
            continue;
        }
        if validate_filename(&name) != FileKind::Concept {
            continue;
        }
        *checked += 1;
        let rel = e.path.clone();
        // 读文件
        match backend.read_file(&rel).await {
            Ok(data) => {
                let text = match std::str::from_utf8(&data) {
                    Ok(t) => t,
                    Err(_) => continue,
                };
                match Concept::parse(text) {
                    Ok(c) => match indexer.index_concept(&rel, text, &c).await {
                        Ok(true) => *indexed += 1,
                        Ok(false) => {}
                        Err(err) => eprintln!("index fail {rel}: {err}"),
                    },
                    Err(err) => eprintln!("parse skip {rel}: {err}"),
                }
            }
            Err(err) => eprintln!("read skip {rel}: {err}"),
        }
    }
    Ok(())
}

fn rel(root: &std::path::Path, dir: &std::path::Path) -> String {
    dir.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::sqlite::SqliteIndex;

    async fn setup() -> Indexer {
        let meta = Arc::new(SqliteIndex::in_memory().unwrap());
        meta.init_schema().await.unwrap();
        let ft = Arc::new(TantivyIndex::in_memory().unwrap());
        Indexer::new(meta, ft)
    }

    #[tokio::test]
    async fn index_then_search_finds_it() {
        let idx = setup().await;
        let c = Concept::parse(
            "---\ntype: note\ntitle: 注意力\nid: nt_1\n---\n\n# 注意力\n\n这是关于注意力的内容。\n",
        )
        .unwrap();
        let changed = idx.index_concept("notes/a.md", "raw", &c).await.unwrap();
        assert!(changed);
        let hits = idx.fulltext.search("注意力", 10).unwrap();
        assert!(hits.iter().any(|h| h.id == "nt_1"));
    }

    #[tokio::test]
    async fn incremental_skips_unchanged() {
        let idx = setup().await;
        let c = Concept::parse("---\ntype: note\nid: nt_1\n---\n\nbody\n").unwrap();
        idx.index_concept("a.md", "raw", &c).await.unwrap();
        let changed = idx.index_concept("a.md", "raw", &c).await.unwrap();
        assert!(!changed, "re-index same content should be no-op");
    }

    #[tokio::test]
    async fn links_become_edges() {
        let idx = setup().await;
        // 先索引目标
        let target = Concept::parse("---\ntype: note\nid: nt_2\n---\n\n目标\n").unwrap();
        idx.index_concept("notes/b.md", "raw", &target)
            .await
            .unwrap();
        // 索引含链接的源
        let src =
            Concept::parse("---\ntype: note\nid: nt_1\n---\n\n见 [/notes/b.md](/notes/b.md)\n")
                .unwrap();
        idx.index_concept("notes/a.md", "raw", &src).await.unwrap();
        let backrefs = idx.meta.backrefs("nt_2").unwrap();
        assert_eq!(backrefs.len(), 1);
        assert_eq!(backrefs[0].src_id, "nt_1");
    }

    #[tokio::test]
    async fn unindex_removes_everywhere() {
        let idx = setup().await;
        let c = Concept::parse("---\ntype: note\nid: nt_1\ntitle: 唯一\n---\n\n独角兽\n").unwrap();
        idx.index_concept("a.md", "raw", &c).await.unwrap();
        idx.unindex("nt_1").await.unwrap();
        assert!(idx.fulltext.search("独角兽", 10).unwrap().is_empty());
        assert!(idx.meta.get_concept("nt_1").unwrap().is_none());
    }

    #[test]
    fn extract_edges_finds_bundle_relative_only() {
        let body = "见 [内部](/notes/a.md) 和 [外部](https://example.com) 还有 [相对](./b.md)";
        let edges = extract_edges(body);
        assert_eq!(edges.len(), 1, "only /-prefixed links are internal");
        assert_eq!(edges[0].dst_path, "/notes/a.md");
        assert_eq!(edges[0].link_text.as_deref(), Some("内部"));
    }

    // ============ generate_suggestions 测试（M1b）============

    use crate::llm::guard::GuardConfig;
    use crate::llm::provider::{
        Capabilities, ChatCap, ChatRequest, EmbedCap, LlmProvider, ProviderKind,
    };
    use crate::llm::routing::{ProviderRef, Registry, Routing, Task};
    use async_trait::async_trait;
    use futures_util::Stream;

    /// 固定返回摘要文本的 mock chat provider。
    struct FakeChat;
    #[async_trait]
    impl LlmProvider for FakeChat {
        fn id(&self) -> &str {
            "fake"
        }
        fn kind(&self) -> ProviderKind {
            ProviderKind::Local
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::CHAT
        }
        async fn health(&self) -> crate::Result<bool> {
            Ok(true)
        }
    }
    #[async_trait]
    impl ChatCap for FakeChat {
        async fn chat_stream(
            &self,
            _: ChatRequest,
        ) -> crate::Result<Box<dyn Stream<Item = crate::Result<String>> + Send + Unpin>> {
            Ok(Box::new(futures_util::stream::iter(vec![Ok(
                "这是一条摘要".into(),
            )])))
        }
    }

    /// 固定返回 768 维向量的 mock embed provider。
    struct FakeEmbed;
    #[async_trait]
    impl LlmProvider for FakeEmbed {
        fn id(&self) -> &str {
            "fake"
        }
        fn kind(&self) -> ProviderKind {
            ProviderKind::Local
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities::EMBED
        }
        async fn health(&self) -> crate::Result<bool> {
            Ok(true)
        }
    }
    #[async_trait]
    impl EmbedCap for FakeEmbed {
        async fn embed(&self, _: &str, _: &[String]) -> crate::Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1; 768]])
        }
    }

    fn v768(lead: &[f32]) -> Vec<f32> {
        let mut v = vec![0.0; 768];
        for (i, x) in lead.iter().enumerate() {
            v[i] = *x;
        }
        v
    }

    #[tokio::test]
    async fn generate_suggestions_writes_summary_tag_vector() {
        let sqlite = Arc::new(crate::index::SqliteIndex::in_memory().unwrap());
        sqlite.init_schema().await.unwrap();

        // 注册 FakeChat（摘要+标签用同一 provider）+ FakeEmbed
        let mut reg = Registry::new();
        reg.register_chat(FakeChat);
        reg.register_embed(FakeEmbed);

        // 路由：Summarize/LinkSuggest/Embed 都指向 "fake"
        let routing = Routing {
            map: [
                (
                    Task::Summarize,
                    (
                        ProviderRef {
                            provider_id: "fake".into(),
                            model: "m".into(),
                        },
                        vec![],
                    ),
                ),
                (
                    Task::LinkSuggest,
                    (
                        ProviderRef {
                            provider_id: "fake".into(),
                            model: "m".into(),
                        },
                        vec![],
                    ),
                ),
                (
                    Task::Embed,
                    (
                        ProviderRef {
                            provider_id: "fake".into(),
                            model: "m".into(),
                        },
                        vec![],
                    ),
                ),
            ]
            .into_iter()
            .collect(),
        };
        let guard = GuardConfig::default(); // cloud_allowed=false，但 provider 是 Local 所以放行

        let concept =
            Concept::parse("---\ntype: note\nid: nt_test\n---\n\n# 测试\n\n这是一段测试笔记。\n")
                .unwrap();
        let text = "# 测试\n\n这是一段测试笔记。";

        generate_suggestions(
            &concept,
            "notes/test.md",
            &sqlite,
            &reg,
            &routing,
            &guard,
            text,
        )
        .await
        .unwrap();

        // 应有 pending 建议（摘要 + 标签——FakeChat 固定返回"这是一条摘要"，
        // 因不含换行，标签解析为 0 个。所以只有摘要建议）
        let pending = sqlite.list_pending_suggestions().unwrap();
        assert!(
            !pending.is_empty(),
            "should have at least summary suggestion"
        );
        assert!(pending.iter().any(|s| matches!(
            &s.suggestion,
            crate::llm::suggestion::Suggestion::Summary { text } if text == "这是一条摘要"
        )));

        // 向量应写入 vec_concepts
        let neighbors = sqlite.vector_search(&v768(&[0.1, 0.1]), 5).unwrap();
        assert!(
            neighbors.iter().any(|(id, _)| id == "nt_test"),
            "concept vector should be searchable"
        );
    }

    #[tokio::test]
    async fn generate_suggestions_guard_blocks_cloud_local_only() {
        let sqlite = Arc::new(crate::index::SqliteIndex::in_memory().unwrap());
        sqlite.init_schema().await.unwrap();
        // Cloud provider 但 concept 标 local_only → 护栏应拒绝
        struct CloudChat;
        #[async_trait]
        impl LlmProvider for CloudChat {
            fn id(&self) -> &str {
                "cloud"
            }
            fn kind(&self) -> ProviderKind {
                ProviderKind::Cloud
            }
            fn capabilities(&self) -> Capabilities {
                Capabilities::CHAT
            }
            async fn health(&self) -> crate::Result<bool> {
                Ok(true)
            }
        }
        #[async_trait]
        impl ChatCap for CloudChat {
            async fn chat_stream(
                &self,
                _: ChatRequest,
            ) -> crate::Result<Box<dyn Stream<Item = crate::Result<String>> + Send + Unpin>>
            {
                Ok(Box::new(futures_util::stream::iter(vec![Ok(
                    "不应被调用".into()
                )])))
            }
        }
        let mut reg = Registry::new();
        reg.register_chat(CloudChat);
        let routing = Routing {
            map: [(
                Task::Summarize,
                (
                    ProviderRef {
                        provider_id: "cloud".into(),
                        model: "m".into(),
                    },
                    vec![],
                ),
            )]
            .into_iter()
            .collect(),
        };
        let guard = GuardConfig {
            cloud_allowed: true,
            sensitive_patterns: vec![],
        };

        // concept 标 llm_local_only: true
        let concept = Concept::parse(
            "---\ntype: note\nid: nt_secret\nllm_local_only: true\n---\n\n机密内容\n",
        )
        .unwrap();
        generate_suggestions(
            &concept,
            "secret.md",
            &sqlite,
            &reg,
            &routing,
            &guard,
            "机密内容",
        )
        .await
        .unwrap();

        // 护栏拒绝 → 不应有建议
        assert!(
            sqlite.list_pending_suggestions().unwrap().is_empty(),
            "guard should block cloud for local_only concept"
        );
    }
}
