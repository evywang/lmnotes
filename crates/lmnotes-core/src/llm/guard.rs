//! 三层隐私护栏（ADR-0005 §4）。
//!
//! GuardConfig 需满足 Send + Sync（Tauri State 要求）。其字段 `cloud_allowed: bool`
//! 与 `sensitive_patterns: Vec<String>` 均自动 Send + Sync，故 derive(Default, Clone)
//! 即满足，无需手动标记。

use super::provider::ProviderKind;

#[derive(Debug, Clone, Default)]
pub struct GuardConfig {
    pub cloud_allowed: bool,
    pub sensitive_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    Allow,
    Deny(String),
}

pub fn check(
    cfg: &GuardConfig,
    provider_kind: ProviderKind,
    content: &str,
    local_only: bool,
) -> GuardDecision {
    if provider_kind == ProviderKind::Local {
        return GuardDecision::Allow;
    }
    // Cloud provider：逐层检查
    if local_only {
        return GuardDecision::Deny("concept marked local_only, cloud not allowed".into());
    }
    for pat in &cfg.sensitive_patterns {
        if content.contains(pat.as_str()) {
            return GuardDecision::Deny(format!("sensitive pattern matched: {pat}"));
        }
    }
    if !cfg.cloud_allowed {
        return GuardDecision::Deny("cloud not globally authorized".into());
    }
    GuardDecision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cfg() -> GuardConfig {
        GuardConfig {
            cloud_allowed: true,
            sensitive_patterns: vec!["密码".into()],
        }
    }

    #[test]
    fn local_always_allowed() {
        assert_eq!(
            check(&cfg(), ProviderKind::Local, "密码", true),
            GuardDecision::Allow
        );
    }
    #[test]
    fn local_only_blocks_cloud() {
        assert!(matches!(
            check(&GuardConfig::default(), ProviderKind::Cloud, "x", true),
            GuardDecision::Deny(_)
        ));
    }
    #[test]
    fn sensitive_blocks_cloud() {
        assert!(matches!(
            check(&cfg(), ProviderKind::Cloud, "我的密码", false),
            GuardDecision::Deny(_)
        ));
    }
    #[test]
    fn cloud_unauthorized_blocks() {
        let mut c = cfg();
        c.cloud_allowed = false;
        c.sensitive_patterns.clear();
        assert!(matches!(
            check(&c, ProviderKind::Cloud, "x", false),
            GuardDecision::Deny(_)
        ));
    }
    #[test]
    fn cloud_clean_content_allowed() {
        assert_eq!(
            check(&cfg(), ProviderKind::Cloud, "hello", false),
            GuardDecision::Allow
        );
    }

    // ── 语音转录路径（ADR-0006）──────────────────────────────────────────
    // 音频 bytes 不可字符串扫描，create_voice_note 传空 content + local_only=false，
    // 仅依赖 cloud_allowed 闸门。这几条固化该契约。

    #[test]
    fn voice_path_blocked_when_cloud_not_allowed() {
        // 默认配置（local-first）：云端 Whisper 应被拒。
        let guard = GuardConfig::default(); // cloud_allowed = false
        assert!(matches!(
            check(&guard, ProviderKind::Cloud, "", false),
            GuardDecision::Deny(_)
        ));
    }

    #[test]
    fn voice_path_allowed_when_cloud_authorized() {
        // 用户显式开启 cloud_allowed：转录放行。
        let guard = GuardConfig {
            cloud_allowed: true,
            sensitive_patterns: vec![],
        };
        assert_eq!(
            check(&guard, ProviderKind::Cloud, "", false),
            GuardDecision::Allow
        );
    }

    #[test]
    fn voice_path_sensitive_pattern_does_not_block_empty_content() {
        // 空内容不会误命中敏感词（即便配了敏感词表）——语音护栏的已知简化。
        let guard = GuardConfig {
            cloud_allowed: true,
            sensitive_patterns: vec!["密码".into()],
        };
        assert_eq!(
            check(&guard, ProviderKind::Cloud, "", false),
            GuardDecision::Allow
        );
    }
}
