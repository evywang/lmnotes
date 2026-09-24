//! 跨 SQLite + Tantivy 的混合检索（向量层 M1c 补）。

pub mod rrf;

use crate::backend::IndexBackend;
use crate::index::tantivy::{SearchHit as TantivyHit, TantivyIndex};
use crate::Result;
use std::sync::Arc;

/// 一条混合检索命中（DTO 前身，M1c 接向量后含 source 标记）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchHit {
    pub id: String,
    pub path: String,
    pub title: Option<String>,
    pub score: f64,
}

pub struct SearchEngine {
    pub meta: Arc<dyn IndexBackend>,
    pub fulltext: Arc<TantivyIndex>,
}

impl SearchEngine {
    pub fn new(meta: Arc<dyn IndexBackend>, fulltext: Arc<TantivyIndex>) -> Self {
        Self { meta, fulltext }
    }

    /// 全文检索 + 元数据富化（向量层 M1c 补）。
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let hits: Vec<TantivyHit> = self.fulltext.search(query, limit)?;
        let mut out = Vec::with_capacity(hits.len());
        for h in hits {
            if let Some(row) = self.meta.get_concept(&h.id)? {
                out.push(SearchHit {
                    id: row.id,
                    path: row.path,
                    title: row.title,
                    score: h.score as f64,
                });
            }
        }
        Ok(out)
    }
}

/// 笔记级混合命中（v0.9 侧栏语义搜索）：两路来源标记 + 融合分。
#[derive(Debug, Clone, PartialEq)]
pub struct FusedHit {
    pub id: String,
    pub score: f64,
    pub in_bm25: bool,
    pub in_vector: bool,
}

/// BM25 命中 + 向量命中（按 rank 顺序的 concept id）→ RRF 融合（k=60，
/// 与 qa/retriever 一致）。双路命中者最高；同分时 BM25 路靠前（更可解释）。
pub fn hybrid_fuse(bm25: &[SearchHit], vector_ids: &[String], limit: usize) -> Vec<FusedHit> {
    const K: f64 = 60.0;
    use std::collections::HashMap;
    let mut acc: HashMap<String, FusedHit> = HashMap::new();
    for (i, h) in bm25.iter().enumerate() {
        let rank = (i + 1) as f64;
        let e = acc.entry(h.id.clone()).or_insert(FusedHit {
            id: h.id.clone(),
            score: 0.0,
            in_bm25: false,
            in_vector: false,
        });
        e.score += 1.0 / (K + rank);
        e.in_bm25 = true;
    }
    for (i, id) in vector_ids.iter().enumerate() {
        let rank = (i + 1) as f64;
        let e = acc.entry(id.clone()).or_insert(FusedHit {
            id: id.clone(),
            score: 0.0,
            in_bm25: false,
            in_vector: false,
        });
        e.score += 1.0 / (K + rank);
        e.in_vector = true;
    }
    let mut out: Vec<FusedHit> = acc.into_values().collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.in_bm25.cmp(&a.in_bm25))
            .then_with(|| a.id.cmp(&b.id))
    });
    out.truncate(limit);
    out
}

/// 搜索片段截取（纯文本，前端负责高亮）：定位 query（整词 → 分词 fallback）
/// 首次出现位置，向两侧各取约 1/4·max 作上下文，超界补省略号。
pub fn make_snippet(body: &str, query: &str, max_chars: usize) -> String {
    let body_trim = body.trim();
    if body_trim.is_empty() || max_chars == 0 {
        return String::new();
    }
    let lower_body = body_trim.to_lowercase();
    let candidates: Vec<String> = {
        let q = query.trim().to_lowercase();
        let mut v = vec![q.clone()];
        v.extend(
            q.split_whitespace()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        );
        v.retain(|s| !s.is_empty());
        v
    };
    let hit_pos = candidates
        .iter()
        .filter_map(|c| lower_body.find(c.as_str()))
        .min()
        .unwrap_or(0);
    // 字符级窗口（不切多字节字符）
    let chars: Vec<char> = body_trim.chars().collect();
    let hit_char = lower_body
        .char_indices()
        .take_while(|(i, _)| *i <= hit_pos)
        .count()
        - 1;
    let start = hit_char.saturating_sub(max_chars / 4);
    let end = (start + max_chars).min(chars.len());
    let mut s: String = chars[start..end].iter().collect();
    if start > 0 {
        s.insert(0, '…');
    }
    if end < chars.len() {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::sqlite::SqliteIndex;
    use crate::indexer::Indexer;
    use crate::okf::concept::Concept;

    #[tokio::test]
    async fn search_returns_enriched_hits() {
        let meta = Arc::new(SqliteIndex::in_memory().unwrap());
        meta.init_schema().await.unwrap();
        let ft = Arc::new(TantivyIndex::in_memory().unwrap());
        let indexer = Indexer::new(meta.clone(), ft.clone());
        let c =
            Concept::parse("---\ntype: note\nid: nt_1\ntitle: 知识图谱\n---\n\n知识图谱连接概念\n")
                .unwrap();
        indexer
            .index_concept("notes/kg.md", "raw", &c)
            .await
            .unwrap();
        let engine = SearchEngine::new(meta, ft);
        let hits = engine.search("知识", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "notes/kg.md");
        assert_eq!(hits[0].title.as_deref(), Some("知识图谱"));
    }

    // ── make_snippet（v0.9 搜索片段）────────────────────────────────────

    #[test]
    fn snippet_windows_around_chinese_hit() {
        let body = "前奏".repeat(50).as_str().to_string()
            + "注意力机制是深度学习的关键"
            + &"后缀".repeat(50);
        let s = make_snippet(&body, "注意力", 40);
        assert!(s.contains("注意力"), "{s}");
        assert!(s.starts_with('…'), "起始被截应有省略号：{s}");
        assert!(s.chars().count() <= 44, "长度受限：{}", s.chars().count());
    }

    #[test]
    fn snippet_falls_back_to_token_then_head() {
        // 整词未命中 → 分词 fallback；仍未命中 → 从头截
        let body = "the attention mechanism works well in transformers".to_string();
        let s = make_snippet(&body, "attention mechanism", 30);
        assert!(s.contains("attention") || s.contains("mechanism"), "{s}");
        let s2 = make_snippet(&body, "完全不相关", 20);
        assert!(s2.starts_with("the attention"), "{s2}");
    }

    #[test]
    fn snippet_empty_body() {
        assert_eq!(make_snippet("", "x", 40), "");
        assert_eq!(make_snippet("   ", "x", 40).trim(), "");
    }

    // ── hybrid_fuse（v0.9 笔记级 RRF）───────────────────────────────────

    fn hit(id: &str) -> SearchHit {
        SearchHit {
            id: id.into(),
            path: format!("{id}.md"),
            title: None,
            score: 1.0,
        }
    }

    #[test]
    fn fuse_both_sources_rank_highest() {
        let bm25 = vec![hit("a"), hit("b")];
        let vec_ids = vec!["b".to_string(), "c".to_string()];
        let fused = hybrid_fuse(&bm25, &vec_ids, 10);
        assert_eq!(fused[0].id, "b", "双路命中应最高：{fused:?}");
        assert!(fused[0].in_bm25 && fused[0].in_vector);
        let a = fused.iter().find(|f| f.id == "a").unwrap();
        assert!(a.in_bm25 && !a.in_vector);
        let c = fused.iter().find(|f| f.id == "c").unwrap();
        assert!(!c.in_bm25 && c.in_vector);
    }

    #[test]
    fn fuse_respects_limit_and_empty_vector() {
        let bm25 = vec![hit("a"), hit("b"), hit("c")];
        let fused = hybrid_fuse(&bm25, &[], 2);
        assert_eq!(fused.len(), 2);
        assert_eq!(fused[0].id, "a", "纯 BM25 保持原序");
        let v_only = hybrid_fuse(&[], &["z".to_string()], 5);
        assert_eq!(v_only.len(), 1);
        assert!(v_only[0].in_vector);
    }
}
