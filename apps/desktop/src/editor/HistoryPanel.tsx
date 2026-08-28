/**
 * 历史版本面板（FR-LLM-09）。
 *
 * 列出当前笔记的 llm 快照（save_snapshot 落盘于 .lmnotes/llm/snapshots/），
 * 点击预览、可一键恢复。恢复 = ① 先存当前全文为新快照（自救）
 * → ② view.dispatch 全文替换（进 CodeMirror history，Ctrl+Z 可撤销恢复本身）
 * → 防抖 save_concept 自动落盘。
 */
import { createSignal, For, Show, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import type { EditorView } from "@codemirror/view";
import { t } from "../i18n";

interface SnapshotInfo {
  ts: number;
  rel_path: string;
  size_bytes: number;
}

export function HistoryPanel(props: {
  conceptPath: string;
  view: () => EditorView | undefined;
  onClose: () => void;
}) {
  const [snaps, setSnaps] = createSignal<SnapshotInfo[]>([]);
  const [preview, setPreview] = createSignal<{ info: SnapshotInfo; text: string } | null>(null);
  const [restoring, setRestoring] = createSignal(false);
  const [loading, setLoading] = createSignal(true);

  onMount(async () => {
    try {
      const list = await invoke<SnapshotInfo[]>("list_snapshots", {
        conceptPath: props.conceptPath,
      });
      setSnaps(list);
    } catch (e) {
      console.error("list_snapshots failed", e);
    } finally {
      setLoading(false);
    }
  });

  const openPreview = async (info: SnapshotInfo) => {
    try {
      const text = await invoke<string>("read_snapshot", { relPath: info.rel_path });
      setPreview({ info, text });
    } catch (e) {
      console.error("read_snapshot failed", e);
    }
  };

  const fmtTime = (ts: number) =>
    new Date(ts * 1000).toLocaleString(undefined, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });

  const fmtSize = (bytes: number) =>
    bytes >= 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${bytes} B`;

  const restore = async () => {
    const view = props.view();
    const p = preview();
    if (!view || !p || restoring()) return;
    setRestoring(true);
    try {
      // ① 自救：先存当前内容
      const current = view.state.doc.toString();
      await invoke("save_snapshot", { conceptPath: props.conceptPath, text: current });
      // ② 全文替换（进 CM history；防抖 save_concept 随后落盘）
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: p.text },
        selection: { anchor: 0 },
      });
      props.onClose();
    } catch (e) {
      console.error("restore failed", e);
    } finally {
      setRestoring(false);
    }
  };

  return (
    <>
      <div class="rewrite-overlay" onClick={props.onClose} />
      <div class="history-panel">
        <div class="history-header">
          <h3>{t("editor.history")}</h3>
          <button class="history-close" title={t("history.close")} onClick={props.onClose}>
            ✕
          </button>
        </div>

        <Show when={!loading()} fallback={<p class="muted small">{t("editor.loading")}</p>}>
          <Show
            when={snaps().length > 0}
            fallback={<p class="muted small">{t("history.empty")}</p>}
          >
            <ul class="history-list">
              <For each={snaps()}>
                {(s) => (
                  <li>
                    <button
                      class={`history-item ${preview()?.info.rel_path === s.rel_path ? "active" : ""}`}
                      onClick={() => openPreview(s)}
                    >
                      <span>{fmtTime(s.ts)}</span>
                      <span class="muted small">{fmtSize(s.size_bytes)}</span>
                    </button>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </Show>

        <Show when={preview()}>
          {(p) => (
            <div class="history-preview">
              <p class="muted small">
                {t("history.previewTitle", {
                  time: fmtTime(p().info.ts),
                })}
              </p>
              <pre>{p().text}</pre>
              <button class="btn-primary" disabled={restoring()} onClick={restore}>
                {restoring() ? t("history.restoring") : t("history.restore")}
              </button>
            </div>
          )}
        </Show>
      </div>
    </>
  );
}
