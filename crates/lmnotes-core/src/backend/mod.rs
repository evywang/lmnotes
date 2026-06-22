//! 存储后端抽象（ADR-0002）。
//!
//! 核心库业务模块禁止直接 std::fs，必须经 StorageBackend。
//! 桌面用 FsBackend，Web（M5）用 OpfsBackend。

pub mod fs;

use crate::index::schema::{ConceptRow, EdgeRow};
use crate::Result;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub path: String, // 相对 vault 根
    pub is_dir: bool,
}

/// 文件存储后端抽象。
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn read_file(&self, rel_path: &str) -> Result<Vec<u8>>;
    async fn write_file(&self, rel_path: &str, data: &[u8]) -> Result<()>;
    async fn list_dir(&self, rel_path: &str) -> Result<Vec<DirEntry>>;
    async fn rename(&self, from: &str, to: &str) -> Result<()>;
    async fn exists(&self, rel_path: &str) -> Result<bool>;
}

/// 索引后端抽象（ADR-0002）。SQLite 元数据层。
///
/// 写入方法异步（便于未来接入远程后端），查询方法同步（rusqlite 本同步，
/// 同步 search 入口无需 block_on）。T6 修正预先应用。
#[async_trait]
pub trait IndexBackend: Send + Sync {
    /// 初始化 schema（幂等）。
    async fn init_schema(&self) -> Result<()>;

    /// UPSERT concept 元数据。
    async fn upsert_concept(&self, row: ConceptRow) -> Result<()>;

    /// 删除 concept（含其出边）。
    async fn delete_concept(&self, id: &str) -> Result<()>;

    /// 替换 concept 的出边（先删后插，增量，见 ADR-0003 F5）。
    async fn replace_edges(&self, src_id: &str, edges: Vec<EdgeRow>) -> Result<()>;

    /// 按 id 查 concept（同步）。
    fn get_concept(&self, id: &str) -> Result<Option<ConceptRow>>;

    /// 按 path 查 concept（同步，改名检测用）。
    fn get_concept_by_path(&self, path: &str) -> Result<Option<ConceptRow>>;

    /// 反向链接查询：谁链接到了 dst_id（同步）。
    fn backrefs(&self, dst_id: &str) -> Result<Vec<EdgeRow>>;
}
