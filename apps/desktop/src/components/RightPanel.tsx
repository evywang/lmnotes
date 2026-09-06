/**
 * 右面板（v1.0 spec §3.4）：按需滑入的审阅面板。P1 仅「建议」标签页。
 * 开合由 ui store 驱动——Welcome/命令面板（P2 顶栏 chip）都能唤起。
 */
import { Show } from "solid-js";
import { useUi } from "../store/ui";
import { useSuggestions } from "../store/llm";
import { SuggestionCenter } from "../suggestions/SuggestionCenter";
import { Icon } from "./icons";
import { t } from "../i18n";

export function RightPanel() {
  const { panelOpen, setPanelOpen } = useUi();
  const { suggestions } = useSuggestions();
  return (
    <aside class={`right-panel ${panelOpen() ? "open" : ""}`} aria-hidden={!panelOpen()}>
      <header class="rp-head">
        <span class="rp-title">
          <Icon name="spark" size={14} /> {t("panel.suggestions")}
          <Show when={suggestions().length > 0}> ({suggestions().length})</Show>
        </span>
        <button class="rp-close" onClick={() => setPanelOpen(false)} aria-label={t("panel.close")}>
          <Icon name="x" size={14} />
        </button>
      </header>
      <div class="rp-body">
        <SuggestionCenter />
      </div>
    </aside>
  );
}
