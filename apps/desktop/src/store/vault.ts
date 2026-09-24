import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export interface SearchHit {
  path: string;
  title: string | null;
  score: number;
  snippet: string | null;
  /** "keyword" | "semantic" | "keyword+semantic" */
  sources: string;
}

const [query, setQuery] = createSignal("");
const [results, setResults] = createSignal<SearchHit[]>([]);
const [searching, setSearching] = createSignal(false);
/** 最近一次搜索是否走了向量召回（v0.9 语义搜索指示） */
const [semantic, setSemantic] = createSignal(false);
const [activePath, setActivePathRaw] = createSignal<string | null>(null);

// ── 最近打开（v0.7 命令面板 FR-SEARCH-01）──────────────────────────────
// setActivePath 统一记录（文件树/搜索/Chat 引用/面板全走它），localStorage 持久化。
const RECENT_KEY = "lmnotes.recentPaths";
const RECENT_MAX = 8;

function loadRecent(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

const [recentPaths, setRecentPaths] = createSignal<string[]>(loadRecent());

function setActivePath(p: string | null) {
  setActivePathRaw(p);
  if (p) {
    setRecentPaths((prev) => {
      const next = [p, ...prev.filter((x) => x !== p)].slice(0, RECENT_MAX);
      try {
        localStorage.setItem(RECENT_KEY, JSON.stringify(next));
      } catch {
        /* localStorage 不可用时仅内存态 */
      }
      return next;
    });
  }
}

export function useVault() {
  return { query, setQuery, results, searching, semantic, activePath, setActivePath };
}
export { recentPaths };

// 请求序号：实时搜索下用户快速输入/清空时，丢弃过期响应避免旧结果回写
let searchSeq = 0;

export async function runSearch(q: string) {
  const seq = ++searchSeq;
  if (!q.trim()) {
    if (seq === searchSeq) {
      setResults([]);
      setSemantic(false);
    }
    return;
  }
  setSearching(true);
  try {
    // v0.9 语义混合搜索：向量+BM25 RRF；embed 不可用时后端自动降级纯 BM25
    const r = await invoke<{ hits: SearchHit[]; semantic: boolean }>("search_hybrid", {
      query: q,
      limit: 50,
    });
    if (seq !== searchSeq) return; // 已有更新的请求，过期结果丢弃
    setResults(r.hits);
    setSemantic(r.semantic);
  } catch (e) {
    console.error("search failed", e);
    if (seq === searchSeq) {
      setResults([]);
      setSemantic(false);
    }
  } finally {
    if (seq === searchSeq) setSearching(false);
  }
}
