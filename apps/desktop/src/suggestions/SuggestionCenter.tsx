import { For, Show, onMount } from "solid-js";
import {
  acceptSuggestion,
  loadSuggestions,
  rejectSuggestion,
  useSuggestions,
} from "../store/llm";
import { t } from "../i18n";

export function SuggestionCenter() {
  const { suggestions } = useSuggestions();
  onMount(() => loadSuggestions());

  return (
    <div class="suggestion-list">
      <Show when={suggestions().length === 0}>
        <p class="muted small">{t("suggestion.empty")}</p>
      </Show>
      <For each={suggestions()}>
        {(s) => (
          <div class="suggestion-item">
            <div class="suggestion-kind" data-kind={s.suggestion.kind}>
              {s.suggestion.kind}
            </div>
            <div class="suggestion-body">
              <Show when={s.suggestion.kind === "summary"}>
                <span>{(s.suggestion as { text: string }).text}</span>
              </Show>
              <Show when={s.suggestion.kind === "tag"}>
                <code>#{(s.suggestion as { tag: string }).tag}</code>
              </Show>
              <Show when={s.suggestion.kind === "link"}>
                <code>
                  [[{(s.suggestion as { link_text: string }).link_text}]]
                </code>
              </Show>
            </div>
            <div class="suggestion-actions">
              <button
                class="btn-accept"
                title={t("suggestion.acceptTooltip")}
                onClick={() => acceptSuggestion(s.id)}
              >
                ✓
              </button>
              <button
                class="btn-reject"
                title={t("suggestion.rejectTooltip")}
                onClick={() => rejectSuggestion(s.id)}
              >
                ✕
              </button>
            </div>
          </div>
        )}
      </For>
    </div>
  );
}
