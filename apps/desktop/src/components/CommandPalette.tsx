/**
 * 命令面板（FR-SEARCH-01，v0.7）：Ctrl+K 唤起的全局浮层。
 *
 * 两类条目：
 * - 命令：由 App 注入（新建/捕获/语音/Chat/图谱/时间线/今日笔记/设置/任务中心），
 *   按 label 子串匹配过滤；
 * - 笔记：复用后端 list_note_titles（title/alias/路径过滤内核 FR-CAP-03），
 *   120ms 防抖；空查询显示命令 + 最近打开（localStorage，上限 8）。
 *
 * 键盘：↑↓ 选择（环绕）、Enter 执行、Esc 关闭。渲染行 = 分区头 + 可执行项，
 * 选择索引只落在可执行项上（header 的 itemIndex = -1 不参与）。
 */
import { createEffect, createSignal, For, Show, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";
import { recentPaths } from "../store/vault";

export interface PaletteAction {
  id: string;
  label: string;
  icon: string;
  run: () => void;
}

interface NoteTitleHit {
  path: string;
  title: string | null;
}

interface Props {
  open: boolean;
  onClose: () => void;
  onOpenNote: (path: string) => void;
  actions: () => PaletteAction[];
}

function baseName(path: string): string {
  const p = path.replace(/\.md$/, "");
  const i = p.lastIndexOf("/");
  return i === -1 ? p : p.slice(i + 1);
}

type RenderRow =
  | { kind: "header"; label: string; itemIndex: -1 }
  | { kind: "item"; icon: string; label: string; meta: string; run: () => void; itemIndex: number };

export function CommandPalette(props: Props) {
  const [query, setQuery] = createSignal("");
  const [notes, setNotes] = createSignal<NoteTitleHit[]>([]);
  const [sel, setSel] = createSignal(0);
  let inputRef: HTMLInputElement | undefined;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  const renderRows = (): { rows: RenderRow[]; itemCount: number } => {
    const rows: RenderRow[] = [];
    let n = 0;
    const pushItem = (icon: string, label: string, meta: string, run: () => void) => {
      rows.push({ kind: "item", icon, label, meta, run, itemIndex: n++ });
    };

    const q = query().trim().toLowerCase();
    const acts = q
      ? props.actions().filter((a) => a.label.toLowerCase().includes(q))
      : props.actions();
    if (acts.length > 0) {
      rows.push({ kind: "header", label: t("palette.sectionCommands"), itemIndex: -1 });
      for (const a of acts) pushItem(a.icon, a.label, "", a.run);
    }
    if (!query().trim()) {
      const rec = recentPaths().filter((p) => p !== "");
      if (rec.length > 0) {
        rows.push({ kind: "header", label: t("palette.sectionRecent"), itemIndex: -1 });
        for (const p of rec) {
          pushItem("🕘", baseName(p), p, () => props.onOpenNote(p));
        }
      }
    } else if (notes().length > 0) {
      rows.push({ kind: "header", label: t("palette.sectionNotes"), itemIndex: -1 });
      for (const hit of notes()) {
        pushItem("📄", hit.title || baseName(hit.path), hit.path, () => props.onOpenNote(hit.path));
      }
    }
    return { rows, itemCount: n };
  };

  // 打开时重置状态并聚焦输入框
  createEffect(() => {
    if (props.open) {
      setQuery("");
      setNotes([]);
      setSel(0);
      queueMicrotask(() => inputRef?.focus());
    }
  });

  // 防抖检索笔记
  createEffect(() => {
    const q = query().trim();
    if (debounceTimer) clearTimeout(debounceTimer);
    if (!q || !props.open) {
      setNotes([]);
      return;
    }
    debounceTimer = setTimeout(async () => {
      try {
        const hits = await invoke<NoteTitleHit[]>("list_note_titles", {
          query: q,
          limit: 12,
        });
        setNotes(hits);
      } catch (e) {
        console.error("palette search", e);
        setNotes([]);
      }
    }, 120);
  });
  onCleanup(() => debounceTimer && clearTimeout(debounceTimer));

  // 条目数变化时选中项回到合法范围
  createEffect(() => {
    if (sel() >= renderRows().itemCount) setSel(0);
  });

  const move = (d: number) => {
    const n = renderRows().itemCount;
    if (n === 0) return;
    setSel((s) => (s + d + n) % n);
  };

  const execute = (i: number) => {
    const row = renderRows().rows.find(
      (r): r is Extract<RenderRow, { kind: "item" }> => r.kind === "item" && r.itemIndex === i,
    );
    if (!row) return;
    props.onClose();
    row.run();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      move(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      move(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      execute(sel());
    } else if (e.key === "Escape") {
      e.preventDefault();
      props.onClose();
    }
  };

  return (
    <Show when={props.open}>
      <div class="palette-overlay" onClick={() => props.onClose()}>
        <div class="palette" onClick={(e) => e.stopPropagation()}>
          <input
            ref={inputRef}
            class="palette-input"
            placeholder={t("palette.placeholder")}
            value={query()}
            onInput={(e) => {
              setQuery(e.currentTarget.value);
              setSel(0);
            }}
            onKeyDown={onKeyDown}
          />
          <div class="palette-list">
            <Show
              when={renderRows().itemCount > 0}
              fallback={<p class="palette-empty">{t("palette.noResults")}</p>}
            >
              <For each={renderRows().rows}>
                {(row) =>
                  row.kind === "header" ? (
                    <div class="palette-section">{row.label}</div>
                  ) : (
                    <button
                      class={`palette-item ${row.itemIndex === sel() ? "selected" : ""}`}
                      onMouseEnter={() => setSel(row.itemIndex)}
                      onClick={() => execute(row.itemIndex)}
                    >
                      <span class="palette-item-icon">{row.icon}</span>
                      <span class="palette-item-label">{row.label}</span>
                      <span class="palette-item-meta">{row.meta}</span>
                    </button>
                  )
                }
              </For>
            </Show>
          </div>
        </div>
      </div>
    </Show>
  );
}
