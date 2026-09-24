/**
 * 上下文栏（v1.0 spec §3.2）：找与收。
 * 搜索（300ms 防抖实时搜）/ 捕获行（＋新笔记·模板▾·语音·导入）/
 * 分段视图（笔记|标签|文件）/ 底部库状态条。
 * 笔记视图默认 = list_timeline（mtime 倒序）；有查询 = 混合搜索结果（首行可问 LMNotes）；
 * 标签过滤 = list_notes_with_tag。
 */
import { createEffect, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { useVault, runSearch } from "../store/vault";
import { useUi, type ContextView } from "../store/ui";
import { FileTree } from "./FileTree";
import { TagCloud } from "./TagCloud";
import { HighlightText, termsOf } from "./HighlightText";
import { Icon } from "./icons";
import { t } from "../i18n";

interface TimelineEntryDto {
  path: string;
  title: string | null;
  type: string;
  mtime: number;
}

interface Props {
  /** App 的刷新计数（新建/导入/库切换/浮窗保存后 +1）。 */
  refreshKey: number;
  onOpenNote: (path: string) => void;
  onAsk: (q: string) => void;
  onVoice: () => void;
  onImport: () => void;
  onNewNote: () => void;
  onNewFromTemplate: (templatePath: string) => void;
}

/** 模板清单懒加载缓存（原 App.createNote 逻辑迁此）。 */
let templatesCache: { name: string; path: string }[] | null = null;

const VIEW_LABEL: Record<ContextView, () => string> = {
  notes: () => t("ctx.view_notes"),
  tags: () => t("ctx.view_tags"),
  files: () => t("ctx.view_files"),
};

export function ContextColumn(props: Props) {
  const { query, setQuery, results, searching, semantic, activePath } = useVault();
  const { contextView, setContextView, tagFilter, setTagFilter } = useUi();
  const [entries, setEntries] = createSignal<TimelineEntryDto[]>([]);
  const [vaultName, setVaultName] = createSignal<string | null>(null);
  const [tplOpen, setTplOpen] = createSignal(false);
  const [tplList, setTplList] = createSignal<{ name: string; path: string }[]>([]);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  const loadEntries = async (tag: string | null) => {
    try {
      const list = tag
        ? await invoke<TimelineEntryDto[]>("list_notes_with_tag", { tag })
        : await invoke<TimelineEntryDto[]>("list_timeline", { limit: 100 });
      setEntries(list);
    } catch (e) {
      console.error("notes list load", e);
      setEntries([]);
    }
  };

  // 默认列表：refreshKey（新建/导入/库切换）或标签过滤变化时重载
  createEffect(() => {
    void props.refreshKey;
    const tag = tagFilter();
    if (!query().trim()) void loadEntries(tag);
  });

  // 实时搜索（spec §3.2：输入即搜，不再要求回车）
  createEffect(() => {
    const q = query();
    if (searchTimer) clearTimeout(searchTimer);
    if (!q.trim()) {
      runSearch("");
      return;
    }
    searchTimer = setTimeout(() => void runSearch(q), 300);
  });
  onCleanup(() => searchTimer && clearTimeout(searchTimer));

  const onSearchKey = (e: KeyboardEvent) => {
    if (e.key !== "Enter") return;
    const q = query().trim();
    if (!q) return;
    if (e.metaKey || e.ctrlKey) {
      props.onAsk(q); // ⌘Enter = 直接问 LMNotes
      return;
    }
    const first = results()[0];
    if (first) props.onOpenNote(first.path); // Enter = 打开第一条
  };

  const pickTag = (tag: string) => {
    setTagFilter(tag);
    setContextView("notes");
  };

  const loadTemplates = async () => {
    if (!templatesCache) {
      try {
        templatesCache = await invoke<{ name: string; path: string }[]>("list_templates");
      } catch {
        templatesCache = [];
      }
    }
    setTplList(templatesCache);
  };

  const baseName = (path: string) => {
    const p = path.replace(/\.md$/, "");
    const i = p.lastIndexOf("/");
    return i === -1 ? p : p.slice(i + 1);
  };

  const fmtDay = (ts: number) => {
    const d = new Date(ts * 1000);
    return d.toDateString() === new Date().toDateString()
      ? t("ctx.today")
      : `${d.getMonth() + 1}/${d.getDate()}`;
  };

  onMount(async () => {
    void loadTemplates();
    try {
      const vs = await invoke<{ name: string; current: boolean }[]>("list_vaults");
      setVaultName(vs.find((v) => v.current)?.name ?? null);
    } catch {
      // 静默
    }
  });

  return (
    <aside class="context-col">
      {/* 搜索框：Enter 开首条；⌘Enter 问 LMNotes */}
      <div class="ctx-search">
        <Icon name="search" size={14} class="ctx-search-icon" />
        <input
          class="ctx-search-input"
          placeholder={t("ctx.searchPlaceholder")}
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={onSearchKey}
        />
        <Show when={query()}>
          <button
            class="ctx-search-clear"
            onClick={() => setQuery("")}
            aria-label={t("ctx.clearSearch")}
          >
            <Icon name="x" size={12} />
          </button>
        </Show>
      </div>

      {/* 捕获行：＋ 新笔记（有模板时 ▾ 展开）/ 语音 / 导入 */}
      <div class="ctx-capture">
        <button class="btn-primary-quiet" onClick={props.onNewNote} title={t("ctx.newNoteTooltip")}>
          <Icon name="plus" size={14} /> {t("ctx.newNote")}
        </button>
        <Show when={tplList().length > 0}>
          <button
            class="btn-icon"
            onClick={() => setTplOpen((v) => !v)}
            title={t("ctx.newNoteTooltip")}
            aria-label={t("ctx.newNote")}
          >
            <Icon name="chevron-down" size={14} />
          </button>
        </Show>
        <button class="btn-icon" onClick={props.onVoice} title={t("app.voiceTooltip")} aria-label={t("app.voiceBtn")}>
          <Icon name="mic" size={16} />
        </button>
        <button class="btn-icon" onClick={props.onImport} title={t("app.importTooltip")} aria-label={t("ctx.import")}>
          <Icon name="import" size={16} />
        </button>
        <Show when={tplOpen()}>
          <div class="ctx-tpl-overlay" onClick={() => setTplOpen(false)} />
          <div class="ctx-tpl-popover">
            <button
              class="ctx-tpl-item"
              onClick={() => {
                setTplOpen(false);
                props.onNewNote();
              }}
            >
              {t("ctx.blankNote")}
            </button>
            <For each={tplList()}>
              {(tpl) => (
                <button
                  class="ctx-tpl-item"
                  onClick={() => {
                    setTplOpen(false);
                    props.onNewFromTemplate(tpl.path);
                  }}
                >
                  {tpl.name}
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>

      {/* 分段视图（搜索态隐藏——结果接管列表） */}
      <Show when={!query().trim()}>
        <div class="ctx-seg" role="tablist">
          <For each={["notes", "tags", "files"] as ContextView[]}>
            {(v) => (
              <button
                role="tab"
                aria-selected={contextView() === v}
                class={`ctx-seg-btn ${contextView() === v ? "active" : ""}`}
                onClick={() => setContextView(v)}
              >
                {VIEW_LABEL[v]()}
              </button>
            )}
          </For>
        </div>
      </Show>

      {/* 标签过滤 chip */}
      <Show when={tagFilter() && !query().trim()}>
        <div class="ctx-tag-chip">
          <Icon name="tag" size={12} /> #{tagFilter()}
          <button class="ctx-tag-clear" onClick={() => setTagFilter(null)} aria-label={t("ctx.tagFilterClear")}>
            <Icon name="x" size={12} />
          </button>
        </div>
      </Show>

      {/* 列表区 */}
      <div class="ctx-list">
        <Show
          when={query().trim()}
          fallback={
            <Show
              when={contextView() === "notes"}
              fallback={
                <Show
                  when={contextView() === "tags"}
                  fallback={
                    <div class="ctx-tree">
                      <FileTree
                        onOpen={props.onOpenNote}
                        activePath={activePath}
                        refreshKey={() => props.refreshKey}
                      />
                    </div>
                  }
                >
                  <TagCloud onPick={pickTag} />
                </Show>
              }
            >
              <Show when={entries().length > 0} fallback={<p class="ctx-empty">{t("ctx.emptyVault")}</p>}>
                <ul class="note-list">
                  <For each={entries()}>
                    {(n) => (
                      <li>
                        <button
                          class={`note-row ${activePath() === n.path ? "current" : ""}`}
                          onClick={() => props.onOpenNote(n.path)}
                        >
                          <span class="note-row-title">{n.title || baseName(n.path)}</span>
                          <span class="note-row-meta">{fmtDay(n.mtime)}</span>
                        </button>
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            </Show>
          }
        >
          {/* 搜索结果：首行固定「问 LMNotes」动作行 */}
          <Show
            when={searching()}
            fallback={
              <Show when={results().length > 0} fallback={<p class="ctx-empty">{t("ctx.noResults")}</p>}>
                <button class="ctx-ask-row" onClick={() => props.onAsk(query().trim())}>
                  <Icon name="spark" size={14} /> {t("ctx.askPrefix")}「{query().trim()}」
                </button>
                <Show when={semantic()}>
                  <p class="ctx-semantic-note">{t("search.semanticNote")}</p>
                </Show>
                <ul class="result-list">
                  <For each={results()}>
                    {(r) => (
                      <li>
                        <button
                          class={`result-item ${activePath() === r.path ? "current" : ""}`}
                          onClick={() => props.onOpenNote(r.path)}
                        >
                          <span class="result-title">
                            <HighlightText text={r.title || baseName(r.path)} terms={termsOf(query())} />
                            <Show when={r.sources.includes("semantic")}>
                              <span class="result-badge">{t("search.semanticBadge")}</span>
                            </Show>
                          </span>
                          <Show when={r.snippet}>
                            <span class="result-snippet">
                              <HighlightText text={r.snippet!} terms={termsOf(query())} />
                            </span>
                          </Show>
                          <span class="result-path">{r.path}</span>
                        </button>
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
            }
          >
            <p class="ctx-empty">{t("app.searching")}</p>
          </Show>
        </Show>
      </div>

      {/* 底部状态条：库名 + 搜索 spinner（LLM 健康点 P3 补） */}
      <div class="ctx-status">
        <span class="ctx-status-vault">
          <Icon name="book" size={12} /> {vaultName() ?? ""}
        </span>
        <Show when={searching()}>
          <span class="spinner" />
        </Show>
      </div>
    </aside>
  );
}
