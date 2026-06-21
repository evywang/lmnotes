//! Vault = OKF Bundle = 一个目录（ADR-0001）。
//!
//! M0 职责：创建骨架、打开、遍历校验合规。

use crate::backend::{fs::FsBackend, StorageBackend};
use crate::okf::validator::{validate_filename, validate_frontmatter, FileKind};
use crate::{CoreError, Result};

pub struct Vault {
    pub backend: Box<dyn StorageBackend>,
}

/// 校验结果：每个文件一条。
#[derive(Debug, Default)]
pub struct VaultReport {
    pub checked: usize,
    pub errors: Vec<(String, String)>, // (path, message)
}

impl VaultReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}

impl Vault {
    /// 打开已存在的 vault 目录。
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let p = path.as_ref();
        if !p.exists() {
            return Err(CoreError::Conformance(format!(
                "vault not found: {}",
                p.display()
            )));
        }
        Ok(Self {
            backend: Box::new(FsBackend::new(p)),
        })
    }

    /// 创建新 vault 骨架（含根 index.md，声明 okf_version）。
    ///
    /// Bootstrap 说明：此处直接调用 `std::fs::create_dir_all`，因为根目录必须先存在
    /// 才能构造 FsBackend（backend 以 root 为锚）。这是 backend 建立前的必要一步，
    /// 不违反 ADR-0002（业务模块经 backend IO）的精神。
    #[allow(clippy::disallowed_methods)]
    pub async fn create(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let p = path.as_ref();
        std::fs::create_dir_all(p)?;
        let vault = Self::open(p)?;
        // 根 index.md（OKF §6 + §11 okf_version 声明）
        vault
            .backend
            .write_file(
                "index.md",
                b"---\nokf_version: \"0.1\"\n---\n\n# Vault Index\n",
            )
            .await?;
        Ok(vault)
    }

    /// 遍历 vault 校验所有 concept 合规（OKF §9）。
    pub async fn validate(&self) -> Result<VaultReport> {
        let mut report = VaultReport::default();
        self.walk("", &mut report).await?;
        Ok(report)
    }

    async fn walk(&self, dir: &str, report: &mut VaultReport) -> Result<()> {
        let entries = self.backend.list_dir(dir).await?;
        for e in entries {
            if e.is_dir {
                // async 递归需 Box::pin 避免无限大小的 future
                Box::pin(self.walk(&e.path, report)).await?;
                continue;
            }
            let name = std::path::Path::new(&e.path)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            match validate_filename(&name) {
                FileKind::Concept => {
                    report.checked += 1;
                    let data = self.backend.read_file(&e.path).await?;
                    let text = std::str::from_utf8(&data)?;
                    let (yaml, _body) = match crate::okf::concept::split_frontmatter(text) {
                        Ok(v) => v,
                        Err(_) => {
                            report
                                .errors
                                .push((e.path.clone(), "no frontmatter (§9 rule 1)".into()));
                            continue;
                        }
                    };
                    match validate_frontmatter(yaml) {
                        Ok(r) if !r.is_conformant() => {
                            for err in r.errors {
                                report.errors.push((e.path.clone(), err));
                            }
                        }
                        Err(err) => {
                            report.errors.push((e.path.clone(), err.to_string()));
                        }
                        _ => {}
                    }
                }
                FileKind::Reserved | FileKind::Other => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_vault_writes_index() {
        let tmp = tempfile::tempdir().unwrap();
        let v = Vault::create(tmp.path()).await.unwrap();
        assert!(v.backend.exists("index.md").await.unwrap());
    }

    #[tokio::test]
    async fn validate_clean_vault_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let v = Vault::create(tmp.path()).await.unwrap();
        v.backend
            .write_file("notes/a.md", b"---\ntype: note\n---\n\nbody\n")
            .await
            .unwrap();
        let report = v.validate().await.unwrap();
        assert!(report.is_ok(), "{:?}", report.errors);
        assert_eq!(report.checked, 1);
    }

    #[tokio::test]
    async fn validate_reports_bad_concept() {
        let tmp = tempfile::tempdir().unwrap();
        let v = Vault::create(tmp.path()).await.unwrap();
        v.backend
            .write_file("notes/bad.md", b"---\ntype: \"\"\n---\n\nbody\n")
            .await
            .unwrap();
        let report = v.validate().await.unwrap();
        assert!(!report.is_ok());
        assert!(report.errors.iter().any(|(p, _)| p.contains("bad.md")));
    }

    #[tokio::test]
    async fn validate_ignores_non_md() {
        let tmp = tempfile::tempdir().unwrap();
        let v = Vault::create(tmp.path()).await.unwrap();
        v.backend.write_file("assets/x.png", b"fake").await.unwrap();
        let report = v.validate().await.unwrap();
        assert!(report.is_ok());
    }

    #[tokio::test]
    async fn validate_skips_reserved_files() {
        let tmp = tempfile::tempdir().unwrap();
        let v = Vault::create(tmp.path()).await.unwrap();
        // log.md 是保留文件，不应作为 concept 校验
        v.backend
            .write_file("log.md", b"# Log\n\n## 2026-06-21\n\n- init\n")
            .await
            .unwrap();
        let report = v.validate().await.unwrap();
        assert!(report.is_ok());
        assert_eq!(report.checked, 0);
    }
}
