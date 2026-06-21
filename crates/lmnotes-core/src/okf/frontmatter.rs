//! OKF frontmatter 解析与序列化（官方 §4.1）。
//!
//! 关键：未知字段必须保留（round-trip 安全）。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 已知的 frontmatter 字段（官方 §4.1 + LMNotes 扩展）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frontmatter {
    /// OKF §4.1 REQUIRED
    #[serde(rename = "type")]
    pub type_: String,

    /// OKF §4.1 recommended
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,

    /// LMNotes 扩展
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<chrono::DateTime<chrono::Utc>>,

    /// 未知字段全部保留（官方 §4.1 round-trip 要求）
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

impl Frontmatter {
    /// 从 YAML 文本解析。
    pub fn parse(yaml: &str) -> crate::Result<Self> {
        serde_yaml::from_str(yaml).map_err(|e| crate::CoreError::Yaml(e.to_string()))
    }

    /// 序列化为 YAML 文本（不含 `---` 分隔符）。
    pub fn to_yaml(&self) -> crate::Result<String> {
        serde_yaml::to_string(self).map_err(|e| crate::CoreError::Yaml(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_type_only() {
        let fm = Frontmatter::parse("type: note").unwrap();
        assert_eq!(fm.type_, "note");
        assert!(fm.title.is_none());
    }

    #[test]
    fn parses_all_known_fields() {
        let yaml = "\
type: note
title: 测试笔记
description: 一个测试
tags: [ai, 知识]
id: nt_20260621_1430_AB34
language: zh-CN
";
        let fm = Frontmatter::parse(yaml).unwrap();
        assert_eq!(fm.title.as_deref(), Some("测试笔记"));
        assert_eq!(fm.tags, vec!["ai", "知识"]);
        assert_eq!(fm.id.as_deref(), Some("nt_20260621_1430_AB34"));
        assert_eq!(fm.language.as_deref(), Some("zh-CN"));
    }

    #[test]
    fn preserves_unknown_keys() {
        let yaml = "\
type: note
custom_field: hello
nested:
  a: 1
  b: 2
";
        let fm = Frontmatter::parse(yaml).unwrap();
        assert!(fm.extra.contains_key("custom_field"));
        assert!(fm.extra.contains_key("nested"));
    }

    #[test]
    fn round_trip_preserves_unknown_keys() {
        let yaml = "\
type: note
custom: value
";
        let fm = Frontmatter::parse(yaml).unwrap();
        let out = fm.to_yaml().unwrap();
        let fm2 = Frontmatter::parse(&out).unwrap();
        assert_eq!(
            fm2.extra.get("custom").and_then(|v| v.as_str()),
            Some("value")
        );
    }

    #[test]
    fn missing_type_is_error() {
        // 注意：serde 不直接报"缺 type"——由 Validator 层处理。
        // 这里验证结构体层：type 是必需字段，无 default，缺它会反序列化失败。
        let result = Frontmatter::parse("title: no type here");
        assert!(result.is_err());
    }

    #[test]
    fn serializes_skip_none_and_empty() {
        let fm = Frontmatter {
            type_: "note".into(),
            title: Some("T".into()),
            description: None,
            resource: None,
            tags: vec![],
            timestamp: None,
            id: None,
            aliases: vec![],
            status: None,
            language: None,
            created: None,
            extra: BTreeMap::new(),
        };
        let out = fm.to_yaml().unwrap();
        assert!(out.contains("type: note"));
        assert!(out.contains("title: T"));
        assert!(!out.contains("description"));
        assert!(!out.contains("tags:"));
    }
}
