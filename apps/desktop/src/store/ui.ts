/**
 * 主窗口 UI 状态（v1.0 spec §3）：上下文栏视图、标签过滤、右面板开合。
 * 会话级（不持久化——重开应用回到默认「笔记」视图，符合克制原则）。
 */
import { createSignal } from "solid-js";

export type ContextView = "notes" | "tags" | "files";

const [contextView, setContextView] = createSignal<ContextView>("notes");
const [tagFilter, setTagFilter] = createSignal<string | null>(null);
const [panelOpen, setPanelOpen] = createSignal(false);

export function useUi() {
  return { contextView, setContextView, tagFilter, setTagFilter, panelOpen, setPanelOpen };
}
export { contextView, setContextView, tagFilter, setTagFilter, panelOpen, setPanelOpen };
