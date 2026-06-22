//! Tantivy 全文索引，中文用 tantivy-jieba tokenizer（社区维护适配器）。
//! 更新语义（ADR-0003 F4）：delete_by_term(id) + add。
//! text 字段加 STORED（M1c RAG 需取回 body snippet）。
//!
//! 选型说明：jieba-rs 官方 README 推荐两个 tantivy 适配器（cang-jie / tantivy-jieba）。
//! 采用 `tantivy-jieba`（jiegec 维护），其正确处理 tokenizer 的 position/offset/短语查询，
//! 比手写适配器更稳健。

use crate::Result;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, TextFieldIndexing, TextOptions, Value};
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};
use tantivy_tokenizer_api::{Token, TokenStream, Tokenizer};

/// 单条检索命中。
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: String,
    pub score: f32,
    pub body_snippet: String,
}

// ============ jieba → Tantivy Tokenizer 适配器（手写） ============
//
// 注：jieba-rs 官方推荐的适配器 tantivy-jieba 0.20 需要 tantivy 0.26+，
// 与本项目用的 tantivy 0.22 不兼容（tokenizer_api 版本 0.3 vs 0.7 冲突）。
// 升级 tantivy 至 0.26+ 后可切换到 tantivy-jieba 删除本适配器。

/// jieba 分词适配 Tantivy Tokenizer。
///
/// 用 `TokenizeMode::Default`（非重叠切分）：Search 模式会产生重叠 token，
/// 导致 QueryParser 构造的 PhraseQuery（slop=0）失配。Default 模式给出严格
/// 不重叠 token 序列，位置严格递增，短语查询正确工作。
#[derive(Clone)]
pub struct JiebaTokenizer {
    jieba: Arc<jieba_rs::Jieba>,
}

impl JiebaTokenizer {
    pub fn new() -> Self {
        Self {
            jieba: Arc::new(jieba_rs::Jieba::default()),
        }
    }
}

impl Default for JiebaTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Tokenizer for JiebaTokenizer {
    type TokenStream<'a> = JiebaTokenStream;

    fn token_stream(&mut self, text: &str) -> Self::TokenStream<'_> {
        // jieba Token.start/end 是 Unicode 字符序号；Tantivy 需要 UTF-8 字节 offset。
        // 建一次「字符序号 → 字节起点」映射。
        let mut char_to_byte: Vec<usize> = Vec::with_capacity(text.chars().count() + 1);
        for (byte, _) in text.char_indices() {
            char_to_byte.push(byte);
        }
        char_to_byte.push(text.len());

        let jtokens = self
            .jieba
            .tokenize(text, jieba_rs::TokenizeMode::Default, true);
        let tokens = jtokens
            .into_iter()
            .enumerate()
            .map(|(position, jt)| {
                let byte_from = *char_to_byte.get(jt.start).unwrap_or(&0);
                let byte_to = *char_to_byte.get(jt.end).unwrap_or(&text.len());
                Token {
                    offset_from: byte_from,
                    offset_to: byte_to,
                    position,
                    position_length: 1,
                    text: jt.word.to_lowercase(),
                }
            })
            .collect();
        JiebaTokenStream { tokens, index: 0 }
    }
}

pub struct JiebaTokenStream {
    tokens: Vec<Token>,
    index: usize,
}

impl TokenStream for JiebaTokenStream {
    fn advance(&mut self) -> bool {
        if self.index < self.tokens.len() {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn token(&self) -> &Token {
        let idx = if self.index == 0 { 0 } else { self.index - 1 };
        &self.tokens[idx]
    }

    fn token_mut(&mut self) -> &mut Token {
        let idx = if self.index == 0 { 0 } else { self.index - 1 };
        &mut self.tokens[idx]
    }
}

// ============ TantivyIndex ============

pub struct TantivyIndex {
    index: Index,
    writer: Mutex<IndexWriter>,
    reader: IndexReader,
    id_field: tantivy::schema::Field,
    text_field: tantivy::schema::Field,
}

impl TantivyIndex {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        std::fs::create_dir_all(&path).map_err(crate::CoreError::Io)?;
        let schema = Self::build_schema();
        let index = match Index::open_in_dir(&path) {
            Ok(idx) => idx,
            Err(_) => Index::create_in_dir(&path, schema.clone())?,
        };
        Self::finish_init(index)
    }

    /// 内存索引（测试用）。
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        let schema = Self::build_schema();
        let index = Index::create_in_ram(schema);
        Self::finish_init(index)
    }

    fn finish_init(index: Index) -> Result<Self> {
        // 注册手写 jieba 适配器（tantivy-jieba 0.20 与 tantivy 0.22 不兼容，见模块注释）
        index.tokenizers().register("jieba", JiebaTokenizer::new());
        let id_field = index.schema().get_field("id").unwrap();
        let text_field = index.schema().get_field("text").unwrap();
        let writer = index.writer(15_000_000)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        Ok(Self {
            index,
            writer: Mutex::new(writer),
            reader,
            id_field,
            text_field,
        })
    }

    fn build_schema() -> Schema {
        let mut schema = Schema::builder();
        // id：不分词、不存储，用于 delete_by_term（raw tokenizer，整体作 term）
        // id：raw 分词（整体作 term，用于 delete_by_term）+ 存储（search 取回）
        let id_opts = TextOptions::default().set_stored().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("raw")
                .set_index_option(tantivy::schema::IndexRecordOption::Basic),
        );
        schema.add_text_field("id", id_opts);
        // text：jieba 分词 + 存储（M1c RAG 取 snippet）
        let text_opts = TextOptions::default().set_stored().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("jieba")
                .set_index_option(tantivy::schema::IndexRecordOption::WithFreqsAndPositions),
        );
        schema.add_text_field("text", text_opts);
        schema.build()
    }

    /// 新增/更新文档（更新 = 先删后增，ADR-0003 F4）。
    pub fn upsert(&self, id: &str, text: &str) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.delete_term(Term::from_field_text(self.id_field, id));
        writer.add_document(doc!(self.id_field => id, self.text_field => text))?;
        writer.commit()?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.delete_term(Term::from_field_text(self.id_field, id));
        writer.commit()?;
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        // 确保读到最新提交（OnCommitWithDelay 有延迟，立即查询前主动 reload）
        let _ = self.reader.reload();
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.text_field]);
        let parsed = parser
            .parse_query(query)
            .map_err(|e| crate::CoreError::Conformance(format!("query parse: {e}")))?;
        let hits = searcher.search(&parsed, &TopDocs::with_limit(limit))?;
        let mut out = Vec::with_capacity(hits.len());
        for (score, doc_addr) in hits {
            let doc: TantivyDocument = searcher.doc(doc_addr)?;
            let id = doc
                .get_first(self.id_field)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let body = doc
                .get_first(self.text_field)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            out.push(SearchHit {
                id,
                score,
                body_snippet: body,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_then_search_chinese() {
        let idx = TantivyIndex::in_memory().unwrap();
        idx.upsert("nt_1", "注意力机制是 Transformer 的核心")
            .unwrap();
        idx.upsert("nt_2", "Transformer 用了自注意力").unwrap();
        let hits = idx.search("注意力", 10).unwrap();
        assert!(hits.iter().any(|h| h.id == "nt_1"));
        assert!(hits.iter().any(|h| h.id == "nt_2"));
    }

    #[test]
    fn update_is_delete_then_add() {
        let idx = TantivyIndex::in_memory().unwrap();
        idx.upsert("nt_1", "内容一").unwrap();
        idx.upsert("nt_1", "内容二 完全不同").unwrap();
        let hits = idx.search("内容", 10).unwrap();
        let count = hits.iter().filter(|h| h.id == "nt_1").count();
        assert_eq!(count, 1, "update should not duplicate");
    }

    #[test]
    fn delete_removes_doc() {
        let idx = TantivyIndex::in_memory().unwrap();
        idx.upsert("nt_1", "唯一关键词 独角兽").unwrap();
        assert!(!idx.search("独角兽", 10).unwrap().is_empty());
        idx.delete("nt_1").unwrap();
        assert!(idx.search("独角兽", 10).unwrap().is_empty());
    }

    #[test]
    fn jieba_segments_chinese_words() {
        let idx = TantivyIndex::in_memory().unwrap();
        idx.upsert("nt_1", "知识图谱是结构化的知识库").unwrap();
        let hits = idx.search("知识", 10).unwrap();
        assert!(hits.iter().any(|h| h.id == "nt_1"));
    }

    #[test]
    fn body_snippet_returned() {
        let idx = TantivyIndex::in_memory().unwrap();
        idx.upsert("nt_1", "这是完整正文内容").unwrap();
        let hits = idx.search("正文", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].body_snippet.contains("正文"));
    }
}
