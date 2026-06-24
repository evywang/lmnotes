//! 上下文拼装：把 top-K RetrievedChunk 拼成带编号引用的 context 段，控制 token 预算。

use super::retriever::RetrievedChunk;

/// 引用编号引用。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CitationRef {
    pub index: usize,
    pub concept_id: String,
    pub path: String,
}

/// 拼装 context 段，每段带编号引用 [1][2]...。
/// 超过 max_chars 时截断（保持完整段落）。
/// 返回 (context 文本, 引用列表)。
pub fn build_context(chunks: &[RetrievedChunk], max_chars: usize) -> (String, Vec<CitationRef>) {
    let mut sections = Vec::new();
    let mut citations = Vec::new();
    let mut total = 0;

    for (i, c) in chunks.iter().enumerate() {
        let title = c.title.as_deref().unwrap_or(&c.path);
        let snippet = if c.snippet.is_empty() {
            "(无正文片段)"
        } else {
            &c.snippet
        };
        let section = format!("[{}] {} ({})\n{}\n", i + 1, title, c.path, snippet);

        if total + section.len() > max_chars {
            break;
        }
        total += section.len();
        citations.push(CitationRef {
            index: i + 1,
            concept_id: c.concept_id.clone(),
            path: c.path.clone(),
        });
        sections.push(section);
    }

    (sections.join("\n"), citations)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &str, path: &str, snippet: &str) -> RetrievedChunk {
        RetrievedChunk {
            concept_id: id.into(),
            path: path.into(),
            title: Some(format!("Title-{id}")),
            snippet: snippet.into(),
            score: 1.0,
        }
    }

    #[test]
    fn builds_numbered_context() {
        let chunks = vec![
            chunk("nt_1", "notes/a.md", "内容A"),
            chunk("nt_2", "notes/b.md", "内容B"),
        ];
        let (ctx, cites) = build_context(&chunks, 10000);
        assert!(ctx.contains("[1] Title-nt_1 (notes/a.md)"));
        assert!(ctx.contains("[2] Title-nt_2 (notes/b.md)"));
        assert!(ctx.contains("内容A"));
        assert_eq!(cites.len(), 2);
        assert_eq!(cites[0].index, 1);
        assert_eq!(cites[0].concept_id, "nt_1");
    }

    #[test]
    fn truncates_on_budget() {
        let chunks = vec![
            chunk("nt_1", "a.md", &"x".repeat(100)),
            chunk("nt_2", "b.md", &"y".repeat(100)),
        ];
        let (ctx, cites) = build_context(&chunks, 150);
        assert_eq!(cites.len(), 1, "only first chunk fits in budget");
        assert!(!ctx.contains("y".repeat(10).as_str()));
    }

    #[test]
    fn empty_snippet_handled() {
        let chunks = vec![chunk("nt_1", "a.md", "")];
        let (ctx, _) = build_context(&chunks, 10000);
        assert!(ctx.contains("(无正文片段)"));
    }
}
