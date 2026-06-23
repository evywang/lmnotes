//! SQLite 元数据索引 + sqlite-vec 向量表实现。

use super::schema::{
    create_vec_sql, ConceptRow, EdgeRow, CREATE_CONCEPTS, CREATE_EDGES, CREATE_SUGGESTIONS,
};
use crate::backend::IndexBackend;
use crate::Result;
use async_trait::async_trait;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::{Mutex, Once};

/// sqlite-vec 扩展注册（进程级一次）。
static VEC_INIT: Once = Once::new();

// sqlite-vec 的 init 入口签名是 C ABI 函数指针；rusqlite::ffi::sqlite3_auto_extension
// 需要特定 fn 类型，跨类型转换必须用 transmute，加显式类型注解满足 clippy。
#[allow(clippy::missing_transmute_annotations)]
fn ensure_vec_extension() {
    VEC_INIT.call_once(|| unsafe {
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
            sqlite_vec::sqlite3_vec_init as *const (),
        )));
    });
}

pub struct SqliteIndex {
    conn: Mutex<Connection>,
}

impl SqliteIndex {
    /// 打开/创建索引文件。
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(crate::CoreError::Io)?;
        }
        ensure_vec_extension();
        let conn = Connection::open(path)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 内存库（测试用）。
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        ensure_vec_extension();
        let conn = Connection::open_in_memory()?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

fn row_from_h(rs: &mut rusqlite::Rows<'_>) -> Result<Option<ConceptRow>> {
    match rs.next()? {
        Some(r) => Ok(Some(ConceptRow {
            id: r.get(0)?,
            path: r.get(1)?,
            type_: r.get(2)?,
            title: r.get(3)?,
            mtime: r.get(4)?,
            content_hash: r.get(5)?,
        })),
        None => Ok(None),
    }
}

#[async_trait]
impl IndexBackend for SqliteIndex {
    async fn init_schema(&self) -> Result<()> {
        // 默认维度 768（Ollama nomic-embed-text）。云端 Provider（如 GLM 1024）应改调
        // init_schema_with_vec_dim。保留此默认实现以兼容 M1a 测试。
        self.init_schema_with_vec_dim(768).await
    }

    async fn upsert_concept(&self, row: ConceptRow) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO concepts (id, path, type_, title, mtime, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                row.id,
                row.path,
                row.type_,
                row.title,
                row.mtime,
                row.content_hash
            ],
        )?;
        Ok(())
    }

    async fn delete_concept(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM concepts WHERE id = ?1", [id])?;
        conn.execute("DELETE FROM edges WHERE src_id = ?1", [id])?;
        Ok(())
    }

    async fn replace_edges(&self, src_id: &str, edges: Vec<EdgeRow>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM edges WHERE src_id = ?1", [src_id])?;
        {
            let mut stmt = conn.prepare(
                "INSERT INTO edges (src_id, dst_id, dst_path, link_text) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for e in &edges {
                stmt.execute(rusqlite::params![
                    e.src_id,
                    e.dst_id,
                    e.dst_path,
                    e.link_text
                ])?;
            }
        }
        Ok(())
    }

    fn get_concept(&self, id: &str) -> Result<Option<ConceptRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, type_, title, mtime, content_hash FROM concepts WHERE id = ?1",
        )?;
        let mut rs = stmt.query([id])?;
        row_from_h(&mut rs)
    }

    fn get_concept_by_path(&self, path: &str) -> Result<Option<ConceptRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, type_, title, mtime, content_hash FROM concepts WHERE path = ?1",
        )?;
        let mut rs = stmt.query([path])?;
        row_from_h(&mut rs)
    }

    fn backrefs(&self, dst_id: &str) -> Result<Vec<EdgeRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT src_id, dst_id, dst_path, link_text FROM edges WHERE dst_id = ?1")?;
        let rows = stmt.query_map([dst_id], |r| {
            Ok(EdgeRow {
                src_id: r.get(0)?,
                dst_id: r.get(1)?,
                dst_path: r.get(2)?,
                link_text: r.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

// ============ Suggestion Store（M1b）============
//
// 注意：这些是 SqliteIndex 的 inherent 方法（不在 IndexBackend trait），
// 独立 impl 块，避免 "method not a member of trait" 错误。
impl SqliteIndex {
    /// sqlite.rs 内的 now_secs 局部副本（indexer::now_secs 是私有的，不能跨模块用）。
    fn now_secs() -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    pub fn list_pending_suggestions(
        &self,
    ) -> crate::Result<Vec<crate::llm::suggestion::SuggestionRecord>> {
        use crate::llm::suggestion::{Suggestion, SuggestionRecord, SuggestionStatus};
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, concept_id, payload, status FROM suggestions WHERE status='pending' ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            let id: String = r.get(0)?;
            let concept_id: String = r.get(1)?;
            let payload: String = r.get(2)?;
            let status: String = r.get(3)?;
            let suggestion: Suggestion = serde_json::from_str(&payload).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    2,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(SuggestionRecord {
                id,
                concept_id,
                suggestion,
                status: SuggestionStatus::parse(&status),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_suggestions_for(
        &self,
        concept_id: &str,
    ) -> crate::Result<Vec<crate::llm::suggestion::SuggestionRecord>> {
        use crate::llm::suggestion::{Suggestion, SuggestionRecord, SuggestionStatus};
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, payload, status FROM suggestions WHERE concept_id=?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([concept_id], |r| {
            let id: String = r.get(0)?;
            let payload: String = r.get(1)?;
            let status: String = r.get(2)?;
            let suggestion: Suggestion = serde_json::from_str(&payload).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    1,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?;
            Ok(SuggestionRecord {
                id,
                concept_id: concept_id.to_string(),
                suggestion,
                status: SuggestionStatus::parse(&status),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn insert_suggestion(
        &self,
        id: &str,
        concept_id: &str,
        suggestion: &crate::llm::suggestion::Suggestion,
    ) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        let payload =
            serde_json::to_string(suggestion).map_err(|e| crate::CoreError::Yaml(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO suggestions (id, concept_id, kind, payload, status, created_at) VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
            rusqlite::params![id, concept_id, suggestion.kind_str(), payload, Self::now_secs()],
        )?;
        Ok(())
    }

    pub fn set_suggestion_status(
        &self,
        id: &str,
        status: crate::llm::suggestion::SuggestionStatus,
    ) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        let applied = if matches!(
            status,
            crate::llm::suggestion::SuggestionStatus::Accepted
                | crate::llm::suggestion::SuggestionStatus::Rejected
        ) {
            Some(Self::now_secs())
        } else {
            None
        };
        conn.execute(
            "UPDATE suggestions SET status=?1, applied_at=COALESCE(?2, applied_at) WHERE id=?3",
            rusqlite::params![status.as_str(), applied, id],
        )?;
        Ok(())
    }

    // ============ 向量层（M1b：embed 写入 sqlite-vec）============

    /// 用指定 embedding 维度初始化 schema。
    /// 若 vec_concepts 表已存在但维度不匹配（切 Provider 场景），drop + recreate
    /// （清空向量，下次启动全量 re-embed 由 indexer 触发）。
    pub async fn init_schema_with_vec_dim(&self, dim: usize) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(&format!(
            "{CREATE_CONCEPTS}\n{CREATE_EDGES}\n{CREATE_SUGGESTIONS}"
        ))?;
        // 检测现有 vec_concepts 维度是否匹配
        let need_recreate = Self::detect_vec_dim_mismatch(&conn, dim).unwrap_or(true);
        if need_recreate {
            // 维度变化或表不存在 → drop（忽略错误，表可能不存在）+ create
            let _ = conn.execute("DROP TABLE IF EXISTS vec_concepts", []);
            conn.execute_batch(&create_vec_sql(dim))?;
        }
        Ok(())
    }

    /// 检测现有 vec_concepts 表的 embedding 维度是否与目标 dim 匹配。
    /// 返回 true 表示需要（重新）创建（维度不符或表不存在）。
    fn detect_vec_dim_mismatch(conn: &Connection, dim: usize) -> Result<bool> {
        // sqlite-vec 的 vec0 表 schema 不易直接查维度，用 PRAGMA 或试探。
        // 简化：尝试插入一个 dim 维向量，失败说明维度不符 → 需重建。
        // 但插入会污染数据——改用 sqlite_master 查建表 SQL 解析维度。
        let sql: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='vec_concepts'",
                [],
                |r| r.get(0),
            )
            .ok();
        match sql {
            None => Ok(true), // 表不存在
            Some(create_sql) => {
                // 解析 "float[NNN]"
                let current = (create_sql.match_indices("float[").next())
                    .and_then(|(i, _)| create_sql[i..].split('[').nth(1))
                    .and_then(|s| s.split(']').next())
                    .and_then(|s| s.parse::<usize>().ok());
                match current {
                    Some(d) => Ok(d != dim),
                    None => Ok(true), // 解析失败，保险起见重建
                }
            }
        }
    }

    /// 写入 concept 向量到 vec_concepts（sqlite-vec）。
    pub fn upsert_vector(&self, id: &str, embedding: &[f32]) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        let ser: String = format!(
            "[{}]",
            embedding
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        conn.execute(
            "INSERT OR REPLACE INTO vec_concepts (id, embedding) VALUES (?1, ?2)",
            rusqlite::params![id, ser],
        )?;
        Ok(())
    }

    /// KNN 向量检索，返回 (id, distance) 列表。
    pub fn vector_search(&self, q: &[f32], k: usize) -> crate::Result<Vec<(String, f32)>> {
        let conn = self.conn.lock().unwrap();
        let ser = format!(
            "[{}]",
            q.iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        let sql = format!(
            "SELECT id, distance FROM vec_concepts WHERE embedding MATCH ?1 ORDER BY distance LIMIT {k}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([&ser], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, f32>(1)?))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, path: &str) -> ConceptRow {
        ConceptRow {
            id: id.into(),
            path: path.into(),
            type_: "note".into(),
            title: Some("T".into()),
            mtime: 1000,
            content_hash: "abc".into(),
        }
    }

    #[tokio::test]
    async fn init_then_upsert_get() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "notes/a.md")).await.unwrap();
        let got = idx.get_concept("nt_1").unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().path, "notes/a.md");
    }

    #[tokio::test]
    async fn upsert_replaces() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "notes/a.md")).await.unwrap();
        let mut r = row("nt_1", "notes/a.md");
        r.title = Some("Updated".into());
        idx.upsert_concept(r).await.unwrap();
        assert_eq!(
            idx.get_concept("nt_1").unwrap().unwrap().title,
            Some("Updated".into())
        );
    }

    #[tokio::test]
    async fn delete_cascades_edges() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "a.md")).await.unwrap();
        idx.upsert_concept(row("nt_2", "b.md")).await.unwrap();
        idx.replace_edges(
            "nt_1",
            vec![EdgeRow {
                src_id: "nt_1".into(),
                dst_id: Some("nt_2".into()),
                dst_path: "/b.md".into(),
                link_text: Some("b".into()),
            }],
        )
        .await
        .unwrap();
        assert_eq!(idx.backrefs("nt_2").unwrap().len(), 1);
        idx.delete_concept("nt_1").await.unwrap();
        assert!(idx.backrefs("nt_2").unwrap().is_empty());
    }

    #[tokio::test]
    async fn replace_edges_is_incremental() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "a.md")).await.unwrap();
        idx.upsert_concept(row("nt_2", "b.md")).await.unwrap();
        idx.upsert_concept(row("nt_3", "c.md")).await.unwrap();
        idx.replace_edges(
            "nt_1",
            vec![EdgeRow {
                src_id: "nt_1".into(),
                dst_id: Some("nt_2".into()),
                dst_path: "/b.md".into(),
                link_text: None,
            }],
        )
        .await
        .unwrap();
        idx.replace_edges(
            "nt_3",
            vec![EdgeRow {
                src_id: "nt_3".into(),
                dst_id: Some("nt_2".into()),
                dst_path: "/b.md".into(),
                link_text: None,
            }],
        )
        .await
        .unwrap();
        // 替换 nt_1 出边，不应影响 nt_3 的出边
        idx.replace_edges("nt_1", vec![]).await.unwrap();
        assert_eq!(idx.backrefs("nt_2").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn get_by_path_works() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "notes/a.md")).await.unwrap();
        let got = idx.get_concept_by_path("notes/a.md").unwrap();
        assert_eq!(got.unwrap().id, "nt_1");
    }

    #[tokio::test]
    async fn suggestion_round_trip() {
        use crate::llm::suggestion::{Suggestion, SuggestionStatus};
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap(); // 含 CREATE_SUGGESTIONS

        let s = Suggestion::Summary {
            text: "测试摘要".into(),
        };
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
        idx.set_suggestion_status("sg_1", SuggestionStatus::Accepted)
            .unwrap();
        assert!(idx.list_pending_suggestions().unwrap().is_empty());

        // list_suggestions_for 仍能看到（不限 status）
        let all = idx.list_suggestions_for("nt_1").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, SuggestionStatus::Accepted);

        // tag/link 类型 round-trip
        idx.insert_suggestion("sg_2", "nt_1", &Suggestion::Tag { tag: "ai".into() })
            .unwrap();
        idx.insert_suggestion(
            "sg_3",
            "nt_1",
            &Suggestion::Link {
                dst_path: "/notes/x.md".into(),
                link_text: "x".into(),
            },
        )
        .unwrap();
        let all2 = idx.list_suggestions_for("nt_1").unwrap();
        assert_eq!(all2.len(), 3, "should have summary+tag+link");
    }
}
