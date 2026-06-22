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
}
