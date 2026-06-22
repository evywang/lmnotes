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
}
