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
const [activePath, setActivePath] = createSignal<string | null>(null);

export function useVault() {
  return { query, setQuery, results, searching, activePath, setActivePath };
}

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
