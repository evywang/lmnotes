/**
 * 搜索高亮（v0.9）：把文本按命中词切分为纯文本/<mark> 段渲染（无 innerHTML，
 * 天然防注入）。词表由查询生成：空白分词 + 中文 2-gram（支持子词命中高亮）。
 */
import { For } from "solid-js";

export function termsOf(query: string): string[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const grams = new Set<string>();
  for (const w of q.split(/\s+/).filter(Boolean)) {
    grams.add(w);
    const chars = [...w];
    if (/[\u4e00-\u9fff]/.test(w) && chars.length > 2) {
      for (let i = 0; i + 2 <= chars.length; i++) grams.add(chars.slice(i, i + 2).join(""));
    }
  }
  // 长词优先，避免 2-gram 抢占整词
  return [...grams].sort((a, b) => b.length - a.length);
}

interface Segment {
  text: string;
  mark: boolean;
}

/** 贪心扫描：每个位置命中最长词；返回纯文本与高亮交替的段。 */
export function splitSegments(text: string, terms: string[]): Segment[] {
  if (terms.length === 0) return [{ text, mark: false }];
  const lower = text.toLowerCase();
  const out: Segment[] = [];
  let plain = "";
  let i = 0;
  while (i < lower.length) {
    const hit = terms.find((t) => t.length > 0 && lower.startsWith(t, i));
    if (hit) {
      if (plain) {
        out.push({ text: plain, mark: false });
        plain = "";
      }
      out.push({ text: text.slice(i, i + hit.length), mark: true });
      i += hit.length;
    } else {
      plain += text[i];
      i += 1;
    }
  }
  if (plain) out.push({ text: plain, mark: false });
  return out;
}

export function HighlightText(props: { text: string; terms: string[] }) {
  return (
    <For each={splitSegments(props.text, props.terms)}>
      {(seg) => (seg.mark ? <mark>{seg.text}</mark> : seg.text)}
    </For>
  );
}
