/**
 * 主区顶栏（v1.0 spec §3.3）：面包屑 + 保存状态 ｜ ✦ AI 菜单 · 历史 · 预览。
 * 纯展示组件——动作经 onAction 回调由 Editor 实现（CodeMirror 逻辑不外泄）。
 */
import { createSignal, For, Show } from "solid-js";
import { Icon } from "./icons";
import { t, type MessageKey } from "../i18n";

export type TopBarAction =
  | "polish"
  | "expand"
  | "translate"
  | "summarize"
  | "extract"
  | "daily-review"
  | "weekly-review"
  | "history"
  | "preview";

interface Props {
  path: string;
  dirty: boolean;
  saving: boolean;
  mediaBusy: boolean;
  queuedHint: boolean;
  canExtract: boolean;
  /** LLM 动作进行中（改写/抽取/回顾）。 */
  busy: boolean;
  previewing: boolean;
  onAction: (a: TopBarAction) => void;
}

const REWRITE_ITEMS: { id: "polish" | "expand" | "translate" | "summarize"; label: MessageKey }[] = [
  { id: "polish", label: "rewrite.polish" },
  { id: "expand", label: "rewrite.expand" },
  { id: "translate", label: "rewrite.translate" },
  { id: "summarize", label: "rewrite.summarize" },
];

export function TopBar(props: Props) {
  const [menuOpen, setMenuOpen] = createSignal(false);
  const crumbs = () => props.path.replace(/\.md$/, "").split("/");

  return (
    <header class="topbar">
      <div class="topbar-crumb" title={props.path}>
        <For each={crumbs()}>
          {(seg, i) => (
            <Show when={i() > 0} fallback={<span class="crumb-current">{seg}</span>}>
              <span class="crumb-sep">›</span>
              <span class="crumb-parent">{seg}</span>
            </Show>
          )}
        </For>
      </div>
      <div class="topbar-status">
        <Show when={props.mediaBusy}>
          <span class="status-chip warn">🎙 {t("editor.mediaTranscribing")}</span>
        </Show>
        <Show when={props.queuedHint}>
          <span class="status-chip warn">⏳ {t("editor.mediaQueued")}</span>
        </Show>
        <Show
          when={props.dirty}
          fallback={
            <Show when={props.saving}>
              <span class="status-chip">{t("topbar.saving")}</span>
            </Show>
          }
        >
          <span class="status-chip accent">● {t("topbar.unsaved")}</span>
        </Show>
      </div>
      <div class="topbar-actions">
        <div class="topbar-menu-wrap">
          <button
            class={`tb-btn ${props.busy ? "busy" : ""}`}
            onClick={() => setMenuOpen((v) => !v)}
            title={t("topbar.aiMenu")}
          >
            <Show when={!props.busy} fallback={<span class="spinner" />}>
              <Icon name="spark" size={14} />
            </Show>
            {t("topbar.aiMenu")}
            <Icon name="chevron-down" size={12} />
          </button>
          <Show when={menuOpen()}>
            <div class="tb-menu-overlay" onClick={() => setMenuOpen(false)} />
            <div class="tb-menu">
              <For each={REWRITE_ITEMS}>
                {(it) => (
                  <button
                    class="tb-menu-item"
                    disabled={props.busy}
                    onClick={() => {
                      setMenuOpen(false);
                      props.onAction(it.id);
                    }}
                  >
                    {t(it.label)}
                  </button>
                )}
              </For>
              <Show when={props.canExtract}>
                <div class="tb-menu-sep" />
                <button
                  class="tb-menu-item"
                  disabled={props.busy}
                  onClick={() => {
                    setMenuOpen(false);
                    props.onAction("extract");
                  }}
                >
                  {t("editor.extractActions")}
                </button>
              </Show>
              <div class="tb-menu-sep" />
              <button
                class="tb-menu-item"
                disabled={props.busy}
                onClick={() => {
                  setMenuOpen(false);
                  props.onAction("daily-review");
                }}
              >
                {t("palette.dailyReview")}
              </button>
              <button
                class="tb-menu-item"
                disabled={props.busy}
                onClick={() => {
                  setMenuOpen(false);
                  props.onAction("weekly-review");
                }}
              >
                {t("palette.weeklyReview")}
              </button>
            </div>
          </Show>
        </div>
        <button class="tb-btn" onClick={() => props.onAction("history")} title={t("editor.historyTooltip")}>
          <Icon name="history" size={14} /> {t("editor.history")}
        </button>
        <button
          class={`tb-btn ${props.previewing ? "active" : ""}`}
          onClick={() => props.onAction("preview")}
          title={t("editor.toggleTooltip")}
        >
          <Icon name={props.previewing ? "pencil" : "eye"} size={14} />
          {props.previewing ? t("editor.edit") : t("editor.preview")}
        </button>
      </div>
    </header>
  );
}
