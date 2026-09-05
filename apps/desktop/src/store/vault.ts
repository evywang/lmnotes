import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export interface SearchHit {
  id: string;
  path: string;
  title: string | null;
  score: number;
}

const [query, setQuery] = createSignal("");
const [results, setResults] = createSignal<SearchHit[]>([]);
const [searching, setSearching] = createSignal(false);
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
  return { query, setQuery, results, searching, activePath, setActivePath };
}
export { recentPaths };

export async function runSearch(q: string) {
  if (!q.trim()) {
    setResults([]);
    return;
  }
  setSearching(true);
  try {
    const r = await invoke<SearchHit[]>("search", { query: q, limit: 50 });
    setResults(r);
  } catch (e) {
    console.error("search failed", e);
    setResults([]);
  } finally {
    setSearching(false);
  }
}
