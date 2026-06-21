//! 文件系统后端（桌面/Tauri 用）。
//!
//! 本模块是核心库内唯一允许直接调用 `std::fs` 的地方（通过 `#![allow(clippy::disallowed_methods)]`
//! 局部豁免 ADR-0002 的 clippy.toml 约束）。业务模块必须经 `StorageBackend` trait。

#![allow(clippy::disallowed_methods)]

use super::{DirEntry, StorageBackend};
use crate::{CoreError, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct FsBackend {
    root: Arc<PathBuf>,
}

impl FsBackend {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: Arc::new(root.into()) }
    }

    fn resolve(&self, rel: &str) -> Result<PathBuf> {
        let joined = self.root.join(rel);
        // 词法归一化 `.` 与 `..`（不用 canonicalize：路径可能尚不存在，写操作会失败）。
        let normalized = lexical_normalize(&joined);
        // 防路径穿越（§10 沙箱）：归一化后必须仍在 vault 根之下。
        if !normalized.starts_with(self.root.as_path()) {
            return Err(CoreError::Conformance(format!("path escapes vault: {rel}")));
        }
        Ok(normalized)
    }
}

/// 词法归一化路径：解析 `.`/`..` 而不访问文件系统。
/// 不处理符号链接（vault 内不应有越界软链，由部署侧保证）。
fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[async_trait]
impl StorageBackend for FsBackend {
    async fn read_file(&self, rel_path: &str) -> Result<Vec<u8>> {
        let p = self.resolve(rel_path)?;
        std::fs::read(&p).map_err(Into::into)
    }

    async fn write_file(&self, rel_path: &str, data: &[u8]) -> Result<()> {
        let p = self.resolve(rel_path)?;
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // 原子写（ADR-0001 数据完整性）
        let tmp = p.with_extension("md.tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    async fn list_dir(&self, rel_path: &str) -> Result<Vec<DirEntry>> {
        let p = self.resolve(rel_path)?;
        let mut entries = Vec::new();
        for e in std::fs::read_dir(&p)? {
            let e = e?;
            let full = e.path();
            let stripped = full.strip_prefix(self.root.as_path()).unwrap_or(&full);
            let rel = stripped.to_string_lossy().replace('\\', "/");
            entries.push(DirEntry { path: rel, is_dir: e.file_type()?.is_dir() });
        }
        Ok(entries)
    }

    async fn rename(&self, from: &str, to: &str) -> Result<()> {
        let pf = self.resolve(from)?;
        let pt = self.resolve(to)?;
        std::fs::rename(pf, pt)?;
        Ok(())
    }

    async fn exists(&self, rel_path: &str) -> Result<bool> {
        Ok(self.resolve(rel_path)?.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_then_read_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let be = FsBackend::new(tmp.path());
        be.write_file("notes/a.md", b"hello").await.unwrap();
        let data = be.read_file("notes/a.md").await.unwrap();
        assert_eq!(data, b"hello");
    }

    #[tokio::test]
    async fn rejects_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let be = FsBackend::new(tmp.path());
        let r = be.read_file("../../etc/passwd").await;
        assert!(matches!(r, Err(CoreError::Conformance(_))));
    }

    #[tokio::test]
    async fn list_dir_returns_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let be = FsBackend::new(tmp.path());
        be.write_file("a.md", b"x").await.unwrap();
        be.write_file("sub/b.md", b"y").await.unwrap();
        let entries = be.list_dir(".").await.unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(names.contains(&"a.md"));
        assert!(names.iter().any(|n| n.contains("sub")));
    }

    #[tokio::test]
    async fn write_is_atomic() {
        let tmp = tempfile::tempdir().unwrap();
        let be = FsBackend::new(tmp.path());
        be.write_file("f.md", b"v1").await.unwrap();
        be.write_file("f.md", b"v2").await.unwrap();
        // 不应残留 .tmp 文件
        assert!(!tmp.path().join("f.md.tmp").exists());
        assert_eq!(be.read_file("f.md").await.unwrap(), b"v2");
    }
}
