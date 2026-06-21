//! Concept 文件 round-trip：写 → 读 → 字段相等。
//! 且验证 ADR-0001 合规自检：删扩展字段后仍合规。

use lmnotes_core::okf::concept::{join_concept, split_frontmatter, Concept};
use lmnotes_core::okf::validator::validate_frontmatter;

const SAMPLE: &str = "\
---
type: note
title: LLM Wiki
tags: [ai]
id: nt_20260621_1430_AB34
language: zh-CN
---

# LLM Wiki

正文，见 [注意力](/notes/ai/attention.md)。
";

#[test]
fn split_separates_frontmatter_and_body() {
    let (yaml, body) = split_frontmatter(SAMPLE).unwrap();
    assert!(yaml.contains("type: note"));
    assert!(body.starts_with("# LLM Wiki"));
}

#[test]
fn parse_extracts_fields() {
    let c = Concept::parse(SAMPLE).unwrap();
    assert_eq!(c.frontmatter.type_, "note");
    assert_eq!(c.frontmatter.title.as_deref(), Some("LLM Wiki"));
    assert_eq!(c.frontmatter.language.as_deref(), Some("zh-CN"));
    assert!(c.body.contains("# LLM Wiki"));
}

#[test]
fn round_trip_preserves_content() {
    let c = Concept::parse(SAMPLE).unwrap();
    let serialized = c.to_string();
    let c2 = Concept::parse(&serialized).unwrap();
    assert_eq!(c.frontmatter, c2.frontmatter);
    assert_eq!(c.body, c2.body);
}

#[test]
fn strip_extensions_remains_conformant() {
    // ADR-0001 合规自检：删扩展字段后仍合规
    let mut c = Concept::parse(SAMPLE).unwrap();
    c.frontmatter.id = None;
    c.frontmatter.language = None;
    c.frontmatter.status = None;
    c.frontmatter.aliases.clear();
    c.frontmatter.created = None;
    let yaml = c.frontmatter.to_yaml().unwrap();
    let report = validate_frontmatter(&yaml).unwrap();
    assert!(report.is_conformant(), "stripped concept must be OKF-conformant");
}

#[test]
fn parse_rejects_missing_frontmatter() {
    let result = Concept::parse("no frontmatter here");
    assert!(result.is_err());
}

#[test]
fn join_round_trips() {
    let (yaml, body) = split_frontmatter(SAMPLE).unwrap();
    let joined = join_concept(yaml, body);
    assert!(joined.starts_with("---\n"));
    assert!(joined.contains("---\n\n# LLM Wiki"));
}
