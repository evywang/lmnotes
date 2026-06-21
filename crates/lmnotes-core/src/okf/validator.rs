//! OKF v0.1 §9 Conformance Validator。
//!
//! 三条硬约束：
//! 1. 每个非保留名 `.md` 含可解析 frontmatter。
//! 2. 每个 frontmatter 含非空 `type`。
//! 3. index.md/log.md（若存在）符合 §6/§7。
//!
//! 与社区 openknowledgeformat.com/validator 规则一致。

use crate::okf::Frontmatter;
use crate::Result;

/// 保留文件名（OKF §3.1）。
pub const RESERVED: &[&str] = &["index.md", "log.md"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileKind {
    /// 普通概念文档（需校验 frontmatter）
    Concept,
    /// 保留文件名（index.md / log.md）
    Reserved,
    /// 非 markdown 文件（资源等，不在 §9 校验范围）
    Other,
}

/// 根据文件名判断类型。
pub fn validate_filename(name: &str) -> FileKind {
    if RESERVED.contains(&name) {
        FileKind::Reserved
    } else if name.ends_with(".md") {
        FileKind::Concept
    } else {
        FileKind::Other
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConformanceReport {
    pub errors: Vec<String>,
}

impl ConformanceReport {
    pub fn is_conformant(&self) -> bool {
        self.errors.is_empty()
    }
}

/// 校验单个 concept 文件的 frontmatter（规则 1 + 2）。
/// 返回 `Err` 表示 YAML 不可解析（规则 1 失败）。
/// 返回 `Ok(report)` 时检查 `report.errors` 判断规则 2。
pub fn validate_frontmatter(yaml: &str) -> Result<ConformanceReport> {
    let fm: Frontmatter = Frontmatter::parse(yaml)?;
    let mut errors = Vec::new();
    if fm.type_.trim().is_empty() {
        errors.push("frontmatter field `type` is empty (OKF §9 rule 2)".into());
    }
    Ok(ConformanceReport { errors })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conformant() {
        let r = validate_frontmatter("type: note").unwrap();
        assert!(r.is_conformant());
    }

    #[test]
    fn empty_type() {
        let r = validate_frontmatter("type: '   '").unwrap();
        assert!(!r.is_conformant());
    }
}
