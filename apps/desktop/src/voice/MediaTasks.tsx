/**
 * 媒体任务中心（v0.5 FR-MEDIA-04）。
 *
 * 列表：kind 图标 + 资产名 + 状态徽章（pending 灰/running 转圈/done 绿/failed 红）
 * + 耗时；实时监听 media-task-update 事件 + 30s 兜底轮询。done 点击打开产物；
 * failed 显示错误 + 重试；pending 可取消。挂设置页，侧栏「⏳ 任务」按钮打开。
 */
import { createSignal, For, Show, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { t } from "../i18n";

export interface MediaTaskDto {
  id: string;
  kind: string;
  asset_rel: string;
  mime: string;
  duration_ms: number | null;
  language: string | null;
  status: string; // pending | running | done | failed | cancelled
  error: string | null;
  result_path: string | null;
  created_at: number;
  updated_at: number;
}

const [open, setOpen] = createSignal(false);
const [tasks, setTasks] = createSignal<MediaTaskDto[]>([]);
let unlisten: UnlistenFn | null = null;
let pollTimer: ReturnType<typeof setInterval> | null = null;

/** 程序化打开任务中心（v0.7 命令面板入口）。 */
export function openMediaTasks() {
  setOpen(true);
}

async function refresh() {
  try {
    setTasks(await invoke<MediaTaskDto[]>("list_media_tasks", { status: null }));
  } catch (e) {
    console.error("list_media_tasks failed", e);
  }
}

/** 全局初始化（App onMount 调一次）：事件订阅 + 兜底轮询。 */
export function initMediaTaskFeed() {
  if (unlisten) return;
  listen<MediaTaskDto>("media-task-update", (ev) => {
    const u = ev.payload;
    setTasks((prev) => {
      const idx = prev.findIndex((x) => x.id === u.id);
      if (idx === -1) {
        void refresh(); // 新任务（本地缺字段）直接全量刷
        return prev;
      }
      const next = [...prev];
      next[idx] = { ...next[idx], status: u.status, result_path: u.result_path, error: u.error };
      return next;
    });
  }).then((fn) => (unlisten = fn));
  pollTimer = setInterval(refresh, 30_000);
  void refresh();
  onCleanup(() => {
    unlisten?.();
    if (pollTimer) clearInterval(pollTimer);
  });
}

export function MediaTasksButton() {
  return (
    <button class="chat-btn" onClick={() => setOpen(true)}>
      ⏳ {t("tasks.openBtn")}
    </button>
  );
}

export function MediaTasksPanel() {
  const assetName = (rel: string) => rel.split("/").pop() ?? rel;
  const fmtTime = (ts: number) =>
    new Date(ts * 1000).toLocaleString(undefined, {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });

  const retry = async (id: string) => {
    await invoke("retry_media_task", { id });
    await refresh();
  };
  const cancel = async (id: string) => {
    await invoke("cancel_media_task", { id });
    await refresh();
  };

  const badge = (s: string) => {
    switch (s) {
      case "running":
        return <span class="task-badge running">⏳ {t("tasks.running")}</span>;
      case "done":
        return <span class="task-badge done">✓ {t("tasks.done")}</span>;
      case "failed":
        return <span class="task-badge failed">✕ {t("tasks.failed")}</span>;
      case "cancelled":
        return <span class="task-badge">⊘ {t("tasks.cancelled")}</span>;
      default:
        return <span class="task-badge pending">… {t("tasks.pending")}</span>;
    }
  };

  return (
    <Show when={open()}>
      <div class="capture-overlay" onClick={() => setOpen(false)}>
        <div class="capture-box tasks-box" onClick={(e) => e.stopPropagation()}>
          <div class="history-header">
            <h3>{t("tasks.title")}</h3>
            <button class="history-close" onClick={() => setOpen(false)}>
              ✕
            </button>
          </div>
          <p class="muted small">{t("tasks.cancelRunningHint")}</p>
          <Show
            when={tasks().length > 0}
            fallback={<p class="muted small">{t("tasks.empty")}</p>}
          >
            <ul class="task-list">
              <For each={tasks()}>
                {(task) => (
                  <li class="task-item">
                    <div class="task-main">
                      <span>{task.kind === "describe" ? "🖼" : "🎙"}</span>
                      <span class="task-asset">{assetName(task.asset_rel)}</span>
                      {badge(task.status)}
                    </div>
                    <div class="muted small task-meta">
                      {fmtTime(task.created_at)}
                      <Show when={task.result_path}>
                        {" · "}
                        <button
                          class="link-btn"
                          onClick={async () => {
                            const { useVault } = await import("../store/vault");
                            useVault().setActivePath(task.result_path!);
                            setOpen(false);
                          }}
                        >
                          {t("tasks.openResult")}
                        </button>
                      </Show>
                    </div>
                    <Show when={task.error}>
                      <p class="task-error">{task.error}</p>
                    </Show>
                    <div class="task-actions">
                      <Show when={task.status === "failed"}>
                        <button class="btn-secondary" onClick={() => retry(task.id)}>
                          {t("tasks.retry")}
                        </button>
                      </Show>
                      <Show when={task.status === "pending" || task.status === "running"}>
                        <button class="btn-secondary" onClick={() => cancel(task.id)}>
                          {t("tasks.cancel")}
                        </button>
                      </Show>
                    </div>
                  </li>
                )}
              </For>
            </ul>
          </Show>
        </div>
      </div>
    </Show>
  );
}
