//! SQLite 元数据索引 + sqlite-vec 向量表实现。

use super::schema::{CREATE_CONCEPTS, CREATE_EDGES, CREATE_VEC, ConceptRow, EdgeRow};
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
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// 内存库（测试用）。
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        ensure_vec_extension();
        let conn = Connection::open_in_memory()?;
        Ok(Self { conn: Mutex::new(conn) })
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
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(&format!("{CREATE_CONCEPTS}\n{CREATE_EDGES}\n{CREATE_VEC}"))?;
        Ok(())
    }

    async fn upsert_concept(&self, row: ConceptRow) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO concepts (id, path, type_, title, mtime, content_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![row.id, row.path, row.type_, row.title, row.mtime, row.content_hash],
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
                stmt.execute(rusqlite::params![e.src_id, e.dst_id, e.dst_path, e.link_text])?;
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
        let mut stmt =
            conn.prepare("SELECT src_id, dst_id, dst_path, link_text FROM edges WHERE dst_id = ?1")?;
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
}
