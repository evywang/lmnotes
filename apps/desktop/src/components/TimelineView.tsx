/**
 * 时间线视图（FR-SEARCH-05，v0.7）：按最近变更（索引 mtime）倒序分组展示。
 * tag 非空时切为「按标签过滤」列表（数据源 list_notes_with_tag），
 * 与时间线共用同一展示组件。浮层样式复用命令面板（palette）视觉。
 */
import { createSignal, For, Show, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";

export interface TimelineEntryDto {
  path: string;
  title: string | null;
  type: string;
  mtime: number;
}

interface Props {
  tag: string | null;
  onClose: () => void;
  onOpen: (path: string) => void;
}

function baseName(path: string): string {
  const p = path.replace(/\.md$/, "");
  const i = p.lastIndexOf("/");
  return i === -1 ? p : p.slice(i + 1);
}

function fmtTime(ts: number): string {
  const d = new Date(ts * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

function dayKey(ts: number): string {
  const d = new Date(ts * 1000);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate(),
  ).padStart(2, "0")}`;
}

function shiftDays(ts: number, days: number): number {
  return ts + days * 86400;
}

function dayLabel(ts: number): string {
  const key = dayKey(ts);
  if (key === dayKey(Date.now() / 1000)) return t("timeline.today");
  if (key === dayKey(shiftDays(Date.now() / 1000, -1))) return t("timeline.yesterday");
  return key;
}

interface Group {
  key: string;
  label: string;
  items: TimelineEntryDto[];
}

export function TimelineView(props: Props) {
  const [entries, setEntries] = createSignal<TimelineEntryDto[]>([]);
  const [loading, setLoading] = createSignal(true);

  onMount(async () => {
    try {
      const list = props.tag
        ? await invoke<TimelineEntryDto[]>("list_notes_with_tag", { tag: props.tag })
        : await invoke<TimelineEntryDto[]>("list_timeline", { limit: 200 });
      setEntries(list);
    } catch (e) {
      console.error("timeline load", e);
    } finally {
      setLoading(false);
    }
  });

  // 分组保持 mtime 倒序（数据源已排序，Map 保插入序）
  const groups = (): Group[] => {
    const map = new Map<string, Group>();
    for (const e of entries()) {
      const key = dayKey(e.mtime);
      let g = map.get(key);
      if (!g) {
        g = { key, label: dayLabel(e.mtime), items: [] };
        map.set(key, g);
      }
      g.items.push(e);
    }
    return [...map.values()];
  };

  return (
    <div class="palette-overlay" onClick={() => props.onClose()}>
      <div class="palette timeline-panel" onClick={(e) => e.stopPropagation()}>
        <div class="timeline-header">
          <span>{props.tag ? `${t("timeline.titleTagPrefix")}${props.tag}` : t("timeline.title")}</span>
          <button class="link-btn" onClick={() => props.onClose()}>
            ✕
          </button>
        </div>
        <div class="palette-list">
          <Show when={!loading()} fallback={<p class="palette-empty">…</p>}>
            <Show
              when={groups().length > 0}
              fallback={<p class="palette-empty">{t("timeline.empty")}</p>}
            >
              <For each={groups()}>
                {(g) => (
                  <>
                    <div class="palette-section">{g.label}</div>
                    <For each={g.items}>
                      {(e) => (
                        <button
                          class="palette-item"
                          onClick={() => {
                            props.onOpen(e.path);
                          }}
                        >
                          <span class="palette-item-icon">
                            {e.type === "daily" ? "📅" : e.type === "transcript" ? "🎙" : "📄"}
                          </span>
                          <span class="palette-item-label">{e.title || baseName(e.path)}</span>
                          <span class="palette-item-meta">{fmtTime(e.mtime)}</span>
                        </button>
                      )}
                    </For>
                  </>
                )}
              </For>
            </Show>
          </Show>
        </div>
      </div>
    </div>
  );
}
