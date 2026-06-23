import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

export type Suggestion =
  | { kind: "summary"; text: string }
  | { kind: "tag"; tag: string }
  | { kind: "link"; dst_path: string; link_text: string };

export interface SuggestionRecord {
  id: string;
  concept_id: string;
  suggestion: Suggestion;
  status: "pending" | "accepted" | "rejected";
}

const [suggestions, setSuggestions] = createSignal<SuggestionRecord[]>([]);

export function useSuggestions() {
  return { suggestions, setSuggestions };
}

export async function loadSuggestions() {
  try {
    const r = await invoke<SuggestionRecord[]>("list_suggestions");
    setSuggestions(r);
  } catch (e) {
    console.error("load suggestions", e);
  }
}

export async function acceptSuggestion(id: string) {
  try {
    await invoke("accept_suggestion", { id });
    setSuggestions((prev) => prev.filter((s) => s.id !== id));
  } catch (e) {
    console.error("accept suggestion", e);
  }
}

export async function rejectSuggestion(id: string) {
  try {
    await invoke("reject_suggestion", { id });
    setSuggestions((prev) => prev.filter((s) => s.id !== id));
  } catch (e) {
    console.error("reject suggestion", e);
  }
}
