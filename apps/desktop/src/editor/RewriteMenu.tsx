import { createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { EditorView } from "@codemirror/view";

const ACTIONS = [
  { id: "polish", label: "润色" },
  { id: "expand", label: "扩写" },
  { id: "translate", label: "翻译为英文" },
  { id: "summarize", label: "总结要点" },
] as const;

/**
 * 就地改写菜单：右键选中文本时弹出，选择动作后调 LLM，替换选区。
 * 撤销：CodeMirror 原生 history（Ctrl+Z 短期）+ save_snapshot（跨会话，由前端在改写前调用）。
 */
export function RewriteMenu(props: {
  view: () => EditorView | undefined;
  conceptPath: string;
  onSaveSnapshot: (text: string) => Promise<void>;
}) {
  const [menuPos, setMenuPos] = createSignal<{ x: number; y: number } | null>(null);
  const [busy, setBusy] = createSignal(false);

  const perform = async (action: string) => {
    const view = props.view();
    if (!view) return;
    const sel = view.state.selection.main;
    const selection = view.state.sliceDoc(sel.from, sel.to);
    if (!selection) {
      setMenuPos(null);
      return;
    }
    setMenuPos(null);
    setBusy(true);
    try {
      // 先存快照（全文）供撤销
      const fullText = view.state.doc.toString();
      await props.onSaveSnapshot(fullText);
      const result = await invoke<string>("rewrite_selection", {
        action,
        selection,
      });
      // 替换选区（进入 CodeMirror history，Ctrl+Z 可撤销）
      view.dispatch({
        changes: { from: sel.from, to: sel.to, insert: result },
        selection: { anchor: sel.from, head: sel.from + result.length },
      });
    } catch (e) {
      console.error("rewrite failed", e);
    } finally {
      setBusy(false);
    }
  };

  // 暴露给 Editor 的 contextmenu handler
  const onContextMenu = (e: MouseEvent): boolean => {
    const view = props.view();
    if (!view) return false;
    const sel = view.state.selection.main;
    const hasSelection = sel.to > sel.from;
    if (!hasSelection) return false;
    setMenuPos({ x: e.clientX, y: e.clientY });
    return true; // consumed
  };

  // document 级 contextmenu 监听（onMount 确保只注册一次）
  onMount(() => {
    const handler = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      // CodeMirror 渲染在 .cm-host > .cm-editor 内，检查两者
      if (target.closest(".cm-editor") || target.closest(".cm-host")) {
        if (onContextMenu(e)) {
          e.preventDefault();
        }
      }
    };
    document.addEventListener("contextmenu", handler);
    onCleanup(() => document.removeEventListener("contextmenu", handler));
  });

  return (
    <Show when={menuPos()}>
      {(pos) => (
        <>
          <div
            class="rewrite-overlay"
            onClick={() => setMenuPos(null)}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenuPos(null);
            }}
          />
          <div
            class="rewrite-menu"
            style={{ left: `${pos().x}px`, top: `${pos().y}px` }}
          >
            <Show when={busy()}>
              <div class="rewrite-busy">改写中…</div>
            </Show>
            <For each={ACTIONS}>
              {(a) => (
                <button
                  class="rewrite-action"
                  disabled={busy()}
                  onClick={() => perform(a.id)}
                >
                  {a.label}
                </button>
              )}
            </For>
          </div>
        </>
      )}
    </Show>
  );
}
