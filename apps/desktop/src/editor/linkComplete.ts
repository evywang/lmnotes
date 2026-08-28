/**
 * 双链补全源（FR-CAP-03，ADR-0001 §5）。
 *
 * 触发：光标前存在未闭合的 `[`（其后无 `]`）——覆盖 PRD 语义：
 *   键入 `[` 后继续输入 → 弹候选；接受后 `[..` 整段替换为
 *   `[label](/vault/相对路径.md)`（`/` 开头 + `.md` 后缀，仅此格式会被
 *   图谱 extract_edges 记为显式内链）。
 *
 * 候选来自 list_note_titles 命令（title/alias/path 子串匹配，后端 ms 级）；
 * alias 命中时 label 用别名（matched_alias 标记）。
 */
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { invoke } from "@tauri-apps/api/core";

interface NoteTitleHit {
  title: string;
  path: string;
  id: string;
  matched_alias: string | null;
}

export function noteLinkCompletions(
  ctx: CompletionContext,
): Promise<CompletionResult | null> {
  // `[` 之后到光标、中间不含 ] 或 [ 的文本即查询词
  const before = ctx.matchBefore(/\[[^\[\]]*$/);
  if (!before) return Promise.resolve(null);

  const query = ctx.state.sliceDoc(before.from + 1, ctx.pos).trim();
  // 用户已输入 `](/` 之类时不再干扰（query 里出现这些说明补全已被接受过）
  if (query.includes("](")) return Promise.resolve(null);

  return invoke<NoteTitleHit[]>("list_note_titles", { query, limit: 20 }).then(
    (hits) => {
      if (hits.length === 0) return null;
      return {
        from: before.from,
        options: hits.map((h) => ({
          label: h.title,
          detail: h.path,
          type: "note",
          apply: (view: { dispatch: (s: object) => void }, _c: unknown, from: number, to: number) => {
            const path = h.path.startsWith("/") ? h.path : `/${h.path}`;
            const md = h.path.endsWith(".md") ? path : `${path}.md`;
            view.dispatch({
              changes: { from, to },
              selection: { anchor: from + md.length },
            });
          },
        })),
        validFor: /^[^\[\]]*$/,
      } satisfies CompletionResult;
    },
    () => null, // 后端失败静默（不打断输入）
  );
}
