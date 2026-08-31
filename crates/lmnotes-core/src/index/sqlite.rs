//! SQLite 元数据索引 + sqlite-vec 向量表实现。

use super::schema::{
    create_vec_sql, ConceptRow, EdgeRow, MediaTask, ALTER_CONCEPTS_ALIASES, CREATE_CHAT_HISTORY,
    CREATE_CONCEPTS, CREATE_EDGES, CREATE_MEDIA_TASKS, CREATE_SUGGESTIONS,
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
            aliases: parse_aliases(&r.get::<_, String>(6)?),
        })),
        None => Ok(None),
    }
}

/// 解析 aliases JSON 列；坏数据（非 JSON / 类型不符）容错为空表。
fn parse_aliases(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// 把 sqlite-vec vec0 表存储的 embedding blob（小端 f32 序列）解码为 Vec<f32>。
fn decode_f32_blob(blob: &[u8]) -> crate::Result<Vec<f32>> {
    if !blob.len().is_multiple_of(4) {
        return Err(crate::CoreError::Other(format!(
            "embedding blob length {} is not a multiple of 4",
            blob.len()
        )));
    }
    // as_chunks（Rust 1.88+）：常量分块由类型保证，替代 chunks_exact(4)+try_into
    //（CI clippy 1.98 的 chunks_exact_to_as_chunks lint 要求）。
    // 上方已校验 len 是 4 的倍数，rest 恒为空。
    let (chunks, _rest) = blob.as_chunks::<4>();
    chunks
        .iter()
        .map(|chunk| Ok(f32::from_le_bytes(*chunk)))
        .collect()
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
            "INSERT OR REPLACE INTO concepts (id, path, type_, title, mtime, content_hash, aliases)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                row.id,
                row.path,
                row.type_,
                row.title,
                row.mtime,
                row.content_hash,
                serde_json::to_string(&row.aliases).unwrap_or_else(|_| "[]".into())
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
            "SELECT id, path, type_, title, mtime, content_hash, aliases FROM concepts WHERE id = ?1",
        )?;
        let mut rs = stmt.query([id])?;
        row_from_h(&mut rs)
    }

    fn get_concept_by_path(&self, path: &str) -> Result<Option<ConceptRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, type_, title, mtime, content_hash, aliases FROM concepts WHERE path = ?1",
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

    fn forward_edges(&self, src_id: &str) -> Result<Vec<EdgeRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT src_id, dst_id, dst_path, link_text FROM edges WHERE src_id = ?1")?;
        let rows = stmt.query_map([src_id], |r| {
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

    fn all_concepts(&self) -> Result<Vec<ConceptRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, type_, title, mtime, content_hash, aliases FROM concepts ORDER BY path",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ConceptRow {
                id: r.get(0)?,
                path: r.get(1)?,
                type_: r.get(2)?,
                title: r.get(3)?,
                mtime: r.get(4)?,
                content_hash: r.get(5)?,
                aliases: parse_aliases(&r.get::<_, String>(6)?),
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    fn all_edges(&self) -> Result<Vec<EdgeRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT src_id, dst_id, dst_path, link_text FROM edges ORDER BY src_id")?;
        let rows = stmt.query_map([], |r| {
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
            "{CREATE_CONCEPTS}\n{CREATE_EDGES}\n{CREATE_SUGGESTIONS}\n{CREATE_CHAT_HISTORY}\n{CREATE_MEDIA_TASKS}"
        ))?;
        // 老库迁移：补 aliases 列（v0.3 FR-CAP-03）。已有该列时 ALTER 报
        // duplicate column，容忍跳过（新库 CREATE 已含，同样会走到这里）。
        if let Err(e) = conn.execute_batch(ALTER_CONCEPTS_ALIASES) {
            let msg = e.to_string();
            if !msg.contains("duplicate column") {
                return Err(e.into());
            }
        }
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

    /// 读取一个 concept 的 embedding 向量（用于查它的语义近邻）。
    /// sqlite-vec 的 vec0 虚拟表把向量存为原始二进制 blob（小端 f32 序列），
    /// 这里按字节解码回 Vec<f32>。若该 concept 尚未被 embed（无向量行），返回 None。
    pub fn concept_embedding(&self, id: &str) -> crate::Result<Option<Vec<f32>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT embedding FROM vec_concepts WHERE id = ?1")?;
        let mut rows = stmt.query_map([id], |r| {
            let blob: Vec<u8> = r.get(0)?;
            Ok(blob)
        })?;
        match rows.next() {
            None => Ok(None),
            Some(Err(e)) => Err(e.into()),
            Some(Ok(blob)) => decode_f32_blob(&blob).map(Some),
        }
    }

    // ============ Chat History（M1c 增强：多轮对话持久化）============

    pub fn append_chat_history(
        &self,
        role: &str,
        content: &str,
        citations: Option<&str>,
    ) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chat_history (role, content, citations, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![role, content, citations, Self::now_secs()],
        )?;
        Ok(())
    }

    pub fn load_chat_history(&self) -> crate::Result<Vec<ChatHistoryRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, role, content, citations FROM chat_history ORDER BY id ASC")?;
        let rows = stmt.query_map([], |r| {
            Ok(ChatHistoryRow {
                id: r.get(0)?,
                role: r.get(1)?,
                content: r.get(2)?,
                citations: r.get(3)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn clear_chat_history(&self) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM chat_history", [])?;
        Ok(())
    }

    // ── 媒体任务（FR-MEDIA-04，v0.5）────────────────────────────────────

    pub fn insert_media_task(&self, t: &MediaTask) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO media_tasks (id, kind, asset_rel, mime, duration_ms, language, status, error, result_path, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![
                t.id, t.kind, t.asset_rel, t.mime, t.duration_ms, t.language, t.status,
                t.error, t.result_path, t.created_at, t.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn update_media_task_status(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
        result_path: Option<&str>,
    ) -> crate::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE media_tasks SET status=?2, error=?3, result_path=?4, updated_at=?5 WHERE id=?1",
            rusqlite::params![
                id,
                status,
                error,
                result_path,
                chrono::Utc::now().timestamp()
            ],
        )?;
        Ok(())
    }

    fn media_task_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<MediaTask> {
        Ok(MediaTask {
            id: row.get(0)?,
            kind: row.get(1)?,
            asset_rel: row.get(2)?,
            mime: row.get(3)?,
            duration_ms: row.get(4)?,
            language: row.get(5)?,
            status: row.get(6)?,
            error: row.get(7)?,
            result_path: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }

    const MEDIA_TASK_COLS: &str =
        "id, kind, asset_rel, mime, duration_ms, language, status, error, result_path, created_at, updated_at";

    /// 列任务；status 为 None 列全部（新→旧）。
    pub fn list_media_tasks(&self, status: Option<&str>) -> crate::Result<Vec<MediaTask>> {
        let conn = self.conn.lock().unwrap();
        let sql = match status {
            Some(_) => format!(
                "SELECT {0} FROM media_tasks WHERE status=?1 ORDER BY created_at DESC, id DESC",
                Self::MEDIA_TASK_COLS
            ),
            None => format!(
                "SELECT {0} FROM media_tasks ORDER BY created_at DESC, id DESC",
                Self::MEDIA_TASK_COLS
            ),
        };
        let mut stmt = conn.prepare(&sql)?;
        let rows = match status {
            Some(st) => stmt
                .query_map([st], Self::media_task_from)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            None => stmt
                .query_map([], Self::media_task_from)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        };
        Ok(rows)
    }

    /// 崩溃/退出恢复：所有 running 重置为 pending（worker 启动兜底）。
    pub fn reset_running_media_tasks(&self) -> crate::Result<u64> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "UPDATE media_tasks SET status='pending', updated_at=?1 WHERE status='running'",
            rusqlite::params![chrono::Utc::now().timestamp()],
        )?;
        Ok(n as u64)
    }

    /// 取消排队中的任务（条件 UPDATE，rows=0 表示已非 pending——防 worker 拉取竞态）。
    pub fn cancel_pending_media_task(&self, id: &str) -> crate::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "UPDATE media_tasks SET status='cancelled', error='cancelled by user', updated_at=?2
             WHERE id=?1 AND status='pending'",
            rusqlite::params![id, chrono::Utc::now().timestamp()],
        )?;
        Ok(n > 0)
    }

    /// 收尾 running 任务（done/failed/cancelled）。条件 UPDATE 仅在仍为 running 时生效——
    /// 以 rows 裁决完成与取消的竞态（先到者赢，后到者不改写）。返回是否写成功。
    pub fn finish_running_media_task(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
        result_path: Option<&str>,
    ) -> crate::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "UPDATE media_tasks SET status=?2, error=?3, result_path=?4, updated_at=?5
             WHERE id=?1 AND status='running'",
            rusqlite::params![
                id,
                status,
                error,
                result_path,
                chrono::Utc::now().timestamp()
            ],
        )?;
        Ok(n > 0)
    }

    /// 拉取待处理任务（FIFO：created_at 升序），limit 条。
    pub fn pending_media_tasks(&self, limit: usize) -> crate::Result<Vec<MediaTask>> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT {0} FROM media_tasks WHERE status='pending' ORDER BY created_at ASC, id ASC LIMIT ?1",
            Self::MEDIA_TASK_COLS
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt
            .query_map([limit as i64], Self::media_task_from)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatHistoryRow {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub citations: Option<String>,
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
            aliases: vec![],
        }
    }

    #[tokio::test]
    async fn aliases_round_trip_through_upsert_and_all_concepts() {
        // v0.3 双链补全：aliases 进索引(FR-CAP-03)。写入 → 读回一致。
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        let mut r = row("nt_1", "notes/ai/attention.md");
        r.title = Some("注意力机制".into());
        r.aliases = vec!["Attention".into(), "注意力".into()];
        idx.upsert_concept(r).await.unwrap();

        let got = idx.get_concept("nt_1").unwrap().unwrap();
        assert_eq!(
            got.aliases,
            vec!["Attention".to_string(), "注意力".to_string()]
        );

        let all = idx.all_concepts().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].aliases.len(), 2);
        assert_eq!(all[0].aliases[1], "注意力");
    }

    #[tokio::test]
    async fn aliases_default_empty_when_null() {
        // 老 schema 迁移后 DEFAULT '[]'：无别名记录读回应为空 vec(而非报错)。
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_2", "notes/b.md")).await.unwrap();
        let got = idx.get_concept("nt_2").unwrap().unwrap();
        assert!(got.aliases.is_empty());
    }

    #[tokio::test]
    async fn migration_adds_aliases_to_legacy_schema() {
        // 老库(无 aliases 列)升到新代码：init_schema 应补列且不丢数据。
        let idx = SqliteIndex::in_memory().unwrap();
        {
            let conn = idx.conn.lock().unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS concepts (
                    id          TEXT PRIMARY KEY,
                    path        TEXT NOT NULL UNIQUE,
                    type_       TEXT NOT NULL,
                    title       TEXT,
                    mtime       INTEGER NOT NULL,
                    content_hash TEXT NOT NULL
                );
                INSERT INTO concepts VALUES ('nt_old', 'old.md', 'note', 'Old', 1, 'h');",
            )
            .unwrap();
        }
        idx.init_schema().await.unwrap();
        let got = idx.get_concept("nt_old").unwrap().unwrap();
        assert_eq!(got.title.as_deref(), Some("Old"));
        assert!(
            got.aliases.is_empty(),
            "legacy row should read empty aliases"
        );
    }

    // ── filter_titles（补全候选匹配内核，FR-CAP-03）──────────────────────

    use crate::index::schema::filter_titles;

    fn sample_rows() -> Vec<ConceptRow> {
        vec![
            ConceptRow {
                id: "nt_1".into(),
                path: "notes/ai/attention.md".into(),
                type_: "note".into(),
                title: Some("注意力机制".into()),
                mtime: 0,
                content_hash: "h".into(),
                aliases: vec!["Attention".into()],
            },
            ConceptRow {
                id: "nt_2".into(),
                path: "notes/llm-wiki.md".into(),
                type_: "note".into(),
                title: Some("LLM Wiki".into()),
                mtime: 0,
                content_hash: "h".into(),
                aliases: vec![],
            },
            ConceptRow {
                id: "nt_3".into(),
                path: "notes/daily/2026-08-26.md".into(),
                type_: "daily".into(),
                title: None, // 无标题 → 回退文件名
                mtime: 0,
                content_hash: "h".into(),
                aliases: vec![],
            },
        ]
    }

    #[test]
    fn filter_titles_empty_query_returns_first_limit() {
        let hits = filter_titles(&sample_rows(), "", 2);
        assert_eq!(hits.len(), 2);
        // all_concepts 按 path 排序传入 → 保持顺序
        assert_eq!(hits[0].path, "notes/ai/attention.md");
    }

    #[test]
    fn filter_titles_matches_title_substring() {
        let hits = filter_titles(&sample_rows(), "注意力", 20);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "注意力机制");
        assert!(hits[0].matched_alias.is_none());
    }

    #[test]
    fn filter_titles_matches_alias_with_matched_alias_set() {
        let hits = filter_titles(&sample_rows(), "atten", 20);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched_alias.as_deref(), Some("Attention"));
    }

    #[test]
    fn filter_titles_is_case_insensitive_on_title() {
        let hits = filter_titles(&sample_rows(), "llm wiki", 20);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "notes/llm-wiki.md");
    }

    #[test]
    fn filter_titles_falls_back_to_filename_and_matches_path() {
        // 无标题笔记按文件名参与匹配
        assert_eq!(filter_titles(&sample_rows(), "2026-08", 20).len(), 1);
        // path 子串命中
        let hits = filter_titles(&sample_rows(), "daily", 20);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "nt_3");
    }

    fn media_task(id: &str, status: &str, created: i64) -> MediaTask {
        MediaTask {
            id: id.into(),
            kind: "transcribe".into(),
            asset_rel: "/assets/audio/aa/hash.webm".into(),
            mime: "audio/webm".into(),
            duration_ms: Some(30_000),
            language: None,
            status: status.into(),
            error: None,
            result_path: None,
            created_at: created,
            updated_at: created,
        }
    }

    #[tokio::test]
    async fn media_task_crud_round_trip() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.insert_media_task(&media_task("mt_1", "pending", 100))
            .unwrap();

        // 状态迁移 + result_path
        idx.update_media_task_status("mt_1", "running", None, None)
            .unwrap();
        idx.update_media_task_status("mt_1", "done", None, Some("transcripts/x.md"))
            .unwrap();

        let all = idx.list_media_tasks(None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, "done");
        assert_eq!(all[0].result_path.as_deref(), Some("transcripts/x.md"));
        assert!(all[0].updated_at >= all[0].created_at);
    }

    #[tokio::test]
    async fn media_task_list_filters_by_status_and_orders() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.insert_media_task(&media_task("mt_a", "pending", 300))
            .unwrap();
        idx.insert_media_task(&media_task("mt_b", "pending", 100))
            .unwrap();
        idx.insert_media_task(&media_task("mt_c", "done", 200))
            .unwrap();

        // pending-only：2 条
        let pending = idx.list_media_tasks(Some("pending")).unwrap();
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().all(|t| t.status == "pending"));
        // 全量按 created_at 降序：mt_a(300) → mt_c(200) → mt_b(100)
        let all = idx.list_media_tasks(None).unwrap();
        assert_eq!(
            all.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["mt_a", "mt_c", "mt_b"]
        );
        // pending FIFO（升序）：mt_b → mt_a
        let fifo = idx.pending_media_tasks(10).unwrap();
        assert_eq!(
            fifo.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["mt_b", "mt_a"]
        );
        // limit 生效
        assert_eq!(idx.pending_media_tasks(1).unwrap().len(), 1);
    }

    #[tokio::test]
    async fn cancel_pending_is_conditional_and_finish_running_is_conditional() {
        // v0.5.1 取消语义：条件 UPDATE 以 rows 裁决，杜绝完成/取消竞态说谎。
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.insert_media_task(&media_task("mt_c1", "pending", 100))
            .unwrap();
        idx.insert_media_task(&media_task("mt_c2", "pending", 101))
            .unwrap();

        // pending 取消：命中
        assert!(idx.cancel_pending_media_task("mt_c1").unwrap());
        // 再取消：已非 pending → false（幂等）
        assert!(!idx.cancel_pending_media_task("mt_c1").unwrap());

        // worker 拉起 mt_c2 → running；此时 pending 取消不再命中
        idx.update_media_task_status("mt_c2", "running", None, None)
            .unwrap();
        assert!(!idx.cancel_pending_media_task("mt_c2").unwrap());

        // finish_running：done 写入成功；重复 finish 不覆盖（已非 running）
        assert!(idx
            .finish_running_media_task("mt_c2", "done", None, Some("t.md"))
            .unwrap());
        assert!(!idx
            .finish_running_media_task("mt_c2", "cancelled", Some("late"), None)
            .unwrap());
        let got = idx
            .list_media_tasks(None)
            .unwrap()
            .into_iter()
            .find(|t| t.id == "mt_c2")
            .unwrap();
        assert_eq!(got.status, "done", "late cancel must NOT overwrite done");
        assert_eq!(got.result_path.as_deref(), Some("t.md"));
    }

    #[test]
    fn filter_titles_respects_limit() {
        assert_eq!(filter_titles(&sample_rows(), "", 1).len(), 1);
        // "o" 命中全部 3 行（attention/llm-wiki/daily 均含 o 或其文件名含 o），limit=2 截断
        assert_eq!(filter_titles(&sample_rows(), "o", 2).len(), 2);
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
    async fn forward_edges_returns_outgoing() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "a.md")).await.unwrap();
        idx.upsert_concept(row("nt_2", "b.md")).await.unwrap();
        idx.upsert_concept(row("nt_3", "c.md")).await.unwrap();
        idx.replace_edges(
            "nt_1",
            vec![
                EdgeRow {
                    src_id: "nt_1".into(),
                    dst_id: Some("nt_2".into()),
                    dst_path: "/b.md".into(),
                    link_text: None,
                },
                EdgeRow {
                    src_id: "nt_1".into(),
                    dst_id: Some("nt_3".into()),
                    dst_path: "/c.md".into(),
                    link_text: None,
                },
            ],
        )
        .await
        .unwrap();
        let out = idx.forward_edges("nt_1").unwrap();
        assert_eq!(out.len(), 2, "nt_1 应有 2 条出链");
        // 对称性：backrefs(nt_2) 也应看到这条边
        assert_eq!(idx.backrefs("nt_2").unwrap().len(), 1);
    }

    #[tokio::test]
    async fn all_concepts_returns_every_row() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema().await.unwrap();
        idx.upsert_concept(row("nt_1", "a.md")).await.unwrap();
        idx.upsert_concept(row("nt_2", "notes/b.md")).await.unwrap();
        let all = idx.all_concepts().unwrap();
        assert_eq!(all.len(), 2);
        // 按 path 排序，notes/b.md 排在 a.md 之后
        assert_eq!(all[0].path, "a.md");
        assert_eq!(all[1].path, "notes/b.md");
    }

    #[tokio::test]
    async fn all_edges_returns_full_adjacency() {
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
                link_text: Some("see b".into()),
            }],
        )
        .await
        .unwrap();
        let edges = idx.all_edges().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].link_text.as_deref(), Some("see b"));
    }

    #[tokio::test]
    async fn concept_embedding_round_trip() {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema_with_vec_dim(3).await.unwrap();
        idx.upsert_concept(row("nt_1", "a.md")).await.unwrap();
        // 未 embed 时返回 None
        assert!(idx.concept_embedding("nt_1").unwrap().is_none());
        // 写入向量后能读回
        let v = vec![0.1, 0.2, 0.3];
        idx.upsert_vector("nt_1", &v).unwrap();
        let got = idx.concept_embedding("nt_1").unwrap().unwrap();
        assert_eq!(got.len(), 3);
        assert!((got[0] - 0.1).abs() < 1e-6);
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
