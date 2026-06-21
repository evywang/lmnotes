//! Concept 与资源的稳定 ID 生成。
//!
//! 规则（ADR-0001）：笔记 `nt_YYYYMMDD_HHMM_<4位base32>`。
//! base32 用 Crockford 编码（去除 I/L/O/U 避免歧义）。

use chrono::NaiveDateTime;

const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// 生成笔记 ID，使用给定时间戳（测试可注入）。
pub fn new_note_id(at: NaiveDateTime) -> String {
    let suffix = random_suffix(4);
    format!("nt_{}_{}", at.format("%Y%m%d_%H%M"), suffix)
}

/// 校验 ID 是否符合 LMNotes 格式（前缀 + 时间 + 4位base32）。
///
/// 结构：`nt_<YYYYMMDD>_<HHMM>_<4位Crockford>`。datetime 段本身含一个下划线，
/// 故从右剥除最后一个下划线取得 suffix，余下整段作为 datetime 解析。
pub fn is_valid(id: &str) -> bool {
    let rest = match id.split_once('_') {
        Some(("nt", rest)) => rest,
        _ => return false,
    };
    let (dt_part, suffix) = match rest.rsplit_once('_') {
        Some(v) => v,
        None => return false,
    };
    let dt = NaiveDateTime::parse_from_str(dt_part, "%Y%m%d_%H%M");
    dt.is_ok() && suffix.len() == 4 && suffix.bytes().all(|b| CROCKFORD.contains(&b))
}

fn random_suffix(n: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    // M0 用时间熵的简易 PRNG（非密码学）；生产环境换 rand crate。
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15);
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let idx = (state % CROCKFORD.len() as u64) as usize;
            CROCKFORD[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_id_format_is_correct() {
        let dt = NaiveDateTime::parse_from_str("2026-06-21T14:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let id = new_note_id(dt);
        assert!(id.starts_with("nt_20260621_1430_"), "got {id}");
        assert_eq!(id.len(), "nt_20260621_1430_".len() + 4);
    }

    #[test]
    fn note_id_suffix_is_crockford_base32() {
        let dt = NaiveDateTime::parse_from_str("2026-06-21T14:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let suffix = &new_note_id(dt)["nt_20260621_1430_".len()..];
        assert!(suffix.bytes().all(|b| CROCKFORD.contains(&b)));
        assert!(!suffix.contains('I') && !suffix.contains('L') && !suffix.contains('O') && !suffix.contains('U'));
    }

    #[test]
    fn is_valid_accepts_well_formed() {
        assert!(is_valid("nt_20260621_1430_AB34"));
    }

    #[test]
    fn is_valid_rejects_bad_prefix() {
        assert!(!is_valid("nk_20260621_1430_AB34"));
    }

    #[test]
    fn is_valid_rejects_ambiguous_chars() {
        // I/L/O/U 不在 Crockford 字符集
        assert!(!is_valid("nt_20260621_1430_AI34"));
        assert!(!is_valid("nt_20260621_1430_AB3U"));
    }

    #[test]
    fn is_valid_rejects_bad_date() {
        assert!(!is_valid("nt_20260132_1430_AB34")); // 32 号
    }

    #[test]
    fn is_valid_rejects_wrong_suffix_length() {
        assert!(!is_valid("nt_20260621_1430_ABC"));   // 3 位
        assert!(!is_valid("nt_20260621_1430_ABCDE")); // 5 位
    }
}
