//! RAG 检索器：向量 + 全文混合（RRF 融合，ADR-0003）。

use crate::backend::IndexBackend;
use crate::index::sqlite::SqliteIndex;
use crate::index::tantivy::TantivyIndex;
use crate::llm::EmbedCap;
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct RetrievedChunk {
    pub concept_id: String,
    pub path: String,
    pub title: Option<String>,
    pub snippet: String,
    pub score: f64,
}

pub struct Retriever {
    pub meta: Arc<dyn IndexBackend>,
    pub fulltext: Arc<TantivyIndex>,
    pub sqlite_index: Arc<SqliteIndex>,
    pub embedder: Arc<dyn EmbedCap>,
    pub embed_model: String,
}

impl Retriever {
    pub fn new(
        meta: Arc<dyn IndexBackend>,
        fulltext: Arc<TantivyIndex>,
        sqlite_index: Arc<SqliteIndex>,
        embedder: Arc<dyn EmbedCap>,
        embed_model: String,
    ) -> Self {
        Self {
            meta,
            fulltext,
            sqlite_index,
            embedder,
            embed_model,
        }
    }

    /// 混合检索：向量 KNN + 全文 BM25，RRF 融合。
    /// 返回 top-K 富化后的 RetrievedChunk。
    pub async fn retrieve(&self, query: &str, k: usize) -> Result<Vec<RetrievedChunk>> {
        let rrf_k = 60; // 标准 RRF 常数

        // 1. 向量召回：embed query → KNN top-2K
        let vec_hits = match self
            .embedder
            .embed(&self.embed_model, &[query.into()])
            .await
        {
            Ok(vectors) => {
                if let Some(qvec) = vectors.into_iter().next() {
                    self.sqlite_index.vector_search(&qvec, k * 2)?
                } else {
                    Vec::new()
                }
            }
            Err(e) => {
                eprintln!("retriever embed query fail: {e}");
                Vec::new()
            }
        };

        // 2. 全文召回：BM25 top-2K
        let ft_hits = match self.fulltext.search(query, k * 2) {
            Ok(hits) => hits,
            Err(e) => {
                eprintln!("retriever fulltext fail: {e}");
                Vec::new()
            }
        };

        // 3. RRF 融合：按 concept_id 聚合得分
        let mut scores: HashMap<String, f64> = HashMap::new();
        for (rank, (id, _dist)) in vec_hits.iter().enumerate() {
            *scores.entry(id.clone()).or_insert(0.0) += 1.0 / (rrf_k as f64 + rank as f64 + 1.0);
        }
        for (rank, h) in ft_hits.iter().enumerate() {
            *scores.entry(h.id.clone()).or_insert(0.0) += 1.0 / (rrf_k as f64 + rank as f64 + 1.0);
        }

        // 排序取 top-K
        let mut merged: Vec<(String, f64)> = scores.into_iter().collect();
        merged.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        merged.truncate(k);

        // 4. 富化：从 SQLite 取 meta + 从 Tantivy 取 snippet
        // Tantivy body_snippet 在 ft_hits 中已有，建索引供快速查找
        let ft_snippets: HashMap<String, String> = ft_hits
            .iter()
            .map(|h| (h.id.clone(), h.body_snippet.clone()))
            .collect();

        let mut out = Vec::with_capacity(merged.len());
        for (id, score) in merged {
            if let Some(row) = self.meta.get_concept(&id)? {
                let snippet = ft_snippets.get(&id).cloned().unwrap_or_default();
                out.push(RetrievedChunk {
                    concept_id: id,
                    path: row.path,
                    title: row.title,
                    snippet,
                    score,
                });
            }
        }
        Ok(out)
    }
}
