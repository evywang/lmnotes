//! 存储后端抽象（ADR-0002）。
//!
//! 核心库业务模块禁止直接 std::fs，必须经 StorageBackend。
//! 桌面用 FsBackend，Web（M5）用 OpfsBackend。

pub mod fs;

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
