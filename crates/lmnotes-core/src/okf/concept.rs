//! Concept 文件 = YAML frontmatter（含 --- 分隔）+ markdown body。

use crate::okf::Frontmatter;
use crate::Result;

/// 一个 OKF concept 文件的内存表示。
#[derive(Debug, Clone, PartialEq)]
pub struct Concept {
    pub frontmatter: Frontmatter,
    pub body: String,
}

impl Concept {
    /// 解析完整 concept 文件文本。
    pub fn parse(text: &str) -> Result<Self> {
        let (yaml, body) = split_frontmatter(text)?;
        let frontmatter = Frontmatter::parse(yaml)?;
        Ok(Self { frontmatter, body: body.to_string() })
    }

    /// 序列化为完整 concept 文件文本。
    pub fn to_string(&self) -> String {
        join_concept_yaml(&self.frontmatter.to_yaml().unwrap_or_default(), &self.body)
    }
}

impl std::fmt::Display for Concept {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_string())
    }
}

/// 拆分 frontmatter 与 body。
/// 要求文本以 `---\n` 开头，到下一个独占一行的 `---` 为 frontmatter。
pub fn split_frontmatter(text: &str) -> Result<(&str, &str)> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .ok_or_else(|| crate::CoreError::Conformance("missing leading `---` delimiter".into()))?;

    // 找结束分隔符：独占一行的 `---`（允许 `\r\n`）
    let end = find_closing_delimiter(rest)
        .ok_or_else(|| crate::CoreError::Conformance("missing closing `---` delimiter".into()))?;

    let yaml = &rest[..end];
    // 跳过结束分隔符行及其后的换行
    let after = &rest[end..];
    let after = after
        .trim_start_matches(['-', '\r', '\n'])
        .trim_start_matches('\n');
    Ok((yaml, after))
}

fn find_closing_delimiter(s: &str) -> Option<usize> {
    for (idx, line) in s.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            // 返回该行起始字节偏移
            let prefix_len: usize = s.split_inclusive('\n').take(idx).map(|l| l.len()).sum();
            return Some(prefix_len);
        }
    }
    None
}

/// 用原始 yaml 文本与 body 重新拼接（测试用，保留原始格式）。
pub fn join_concept(yaml: &str, body: &str) -> String {
    join_concept_yaml(yaml, body)
}

fn join_concept_yaml(yaml: &str, body: &str) -> String {
    let yaml = yaml.trim_end_matches('\n');
    if body.is_empty() {
        format!("---\n{yaml}\n---\n")
    } else {
        format!("---\n{yaml}\n---\n\n{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_body_ok() {
        let c = Concept {
            frontmatter: Frontmatter::parse("type: note").unwrap(),
            body: String::new(),
        };
        let s = c.to_string();
        assert!(Concept::parse(&s).is_ok());
    }
}
