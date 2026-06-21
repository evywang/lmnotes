//! 官方 OKF v0.1 §9 Conformance 合规测试。
//! 规则原文见 docs/okf/SPEC.v0.1.md §9。

use lmnotes_core::okf::validator::{validate_filename, validate_frontmatter, FileKind};

#[test]
fn conformant_concept_passes() {
    let fm_yaml = "type: note\ntitle: ok\n";
    let report = validate_frontmatter(fm_yaml).unwrap();
    assert!(report.is_conformant(), "should be conformant: {report:?}");
    assert!(report.errors.is_empty());
}

#[test]
fn missing_type_fails_rule_2() {
    let fm_yaml = "title: no type\n";
    // 缺 type：frontmatter 层就解析失败（serde 必需字段）
    let report = validate_frontmatter(fm_yaml);
    assert!(report.is_err() || !report.unwrap().is_conformant());
}

#[test]
fn empty_type_fails_rule_2() {
    let fm_yaml = "type: \"\"\n";
    let report = validate_frontmatter(fm_yaml).unwrap();
    assert!(!report.is_conformant());
    assert!(report.errors.iter().any(|e| e.contains("type")));
}

#[test]
fn unparseable_yaml_fails_rule_1() {
    let bad_yaml = "type: [unterminated\n";
    let report = validate_frontmatter(bad_yaml);
    assert!(report.is_err());
}

#[test]
fn reserved_filename_index_is_not_concept() {
    assert_eq!(validate_filename("index.md"), FileKind::Reserved);
}

#[test]
fn reserved_filename_log_is_not_concept() {
    assert_eq!(validate_filename("log.md"), FileKind::Reserved);
}

#[test]
fn normal_md_is_concept() {
    assert_eq!(validate_filename("my-note.md"), FileKind::Concept);
}

#[test]
fn non_md_is_ignored() {
    assert_eq!(validate_filename("image.png"), FileKind::Other);
}
