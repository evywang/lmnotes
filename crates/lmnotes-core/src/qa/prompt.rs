//! Prompt 模板 + 引用解析。

/// System prompt：指示 LLM 基于 context 回答，每条论点用 [n] 引用。
pub const SYSTEM: &str = "\
你是一个知识库助手。根据下方【上下文】回答用户问题。
规则：
1. 每条论点末尾用 [n] 引用对应上下文编号（如 [1]）。
2. 仅基于上下文回答；上下文不足时明确说明\"我的笔记中暂无相关信息\"。
3. 回答简洁，用要点列表。";

/// 从 LLM 回答中提取引用编号 [n]，去重保序。
pub fn extract_citations(text: &str) -> Vec<usize> {
    let mut nums = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            // 找匹配的 ]
            if let Some(end) = bytes[i + 1..].iter().position(|&b| b == b']') {
                let inner = &text[i + 1..i + 1 + end];
                if let Ok(n) = inner.trim().parse::<usize>() {
                    if n > 0 && !nums.contains(&n) {
                        nums.push(n);
                    }
                }
                i += end + 2;
                continue;
            }
        }
        i += 1;
    }
    nums
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_unique_ordered() {
        let text = "注意力是核心 [1]，机制包含 QKV [2]。参考 [1] 再次说明。";
        let cites = extract_citations(text);
        assert_eq!(cites, vec![1, 2]);
    }

    #[test]
    fn ignores_non_numeric() {
        let text = "见 [abc] 和 [note] 但 [3] 有效";
        let cites = extract_citations(text);
        assert_eq!(cites, vec![3]);
    }

    #[test]
    fn empty_when_no_brackets() {
        assert!(extract_citations("no citations here").is_empty());
    }
}
