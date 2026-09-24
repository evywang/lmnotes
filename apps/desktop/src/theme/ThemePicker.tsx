/**
 * 设置·外观：主题卡片网格（spec §4.3）。选中即 setTheme（即时生效）；
 * 用户主题解析失败在网格下方警示，可手动重扫。
 */
import { For, Show } from "solid-js";
import { t } from "../i18n";
import {
  allThemes, currentThemeId, loadUserThemes, resolveColors, setTheme, themeWarnings,
} from "./index";
import { Icon } from "../components/icons";

export function ThemePicker() {
  const dots = (id: string) => {
    const theme = allThemes().find((x) => x.id === id)!;
    const c = resolveColors(theme);
    return [c["surface-0"], c["surface-1"], c["surface-2"], c["text-1"], c["accent"]];
  };

  return (
    <div class="theme-picker">
      <div class="theme-cards">
        <button
          class={`theme-card ${currentThemeId() === "auto" ? "selected" : ""}`}
          onClick={() => setTheme("auto")}
        >
          <span class="theme-card-name">{t("settings.themeAuto")}</span>
          <span class="theme-dots">
            <Icon name="swatch" size={14} />
          </span>
        </button>
        <For each={allThemes()}>
          {(theme) => (
            <button
              class={`theme-card ${currentThemeId() === theme.id ? "selected" : ""}`}
              onClick={() => setTheme(theme.id)}
            >
              <span class="theme-card-name">
                {theme.builtin ? t(theme.nameKey!) : theme.name}
              </span>
              <span class="theme-dots">
                <For each={dots(theme.id)}>
                  {(color) => <span class="theme-dot" style={{ background: color }} />}
                </For>
              </span>
            </button>
          )}
        </For>
      </div>
      <Show when={themeWarnings().length > 0}>
        <p class="theme-warning">
          {t("settings.themeInvalid", { files: themeWarnings().join(", ") })}
        </p>
      </Show>
      <button type="button" class="btn-secondary btn-small" onClick={() => void loadUserThemes()}>
        {t("settings.themeRescan")}
      </button>
    </div>
  );
}
