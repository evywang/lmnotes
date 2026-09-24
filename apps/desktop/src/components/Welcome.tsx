/**
 * 主区欢迎页（v1.0 spec §7）：无选中笔记时的引导空态。
 */
import { Icon } from "./icons";
import { useUi } from "../store/ui";
import { t } from "../i18n";

export function Welcome(props: { onNew: () => void; onImport: () => void }) {
  const { setPanelOpen } = useUi();
  return (
    <div class="welcome">
      <h1 class="welcome-title">LMNotes</h1>
      <p class="welcome-sub">{t("welcome.subtitle")}</p>
      <div class="welcome-actions">
        <button class="btn-primary" onClick={props.onNew}>
          <Icon name="plus" size={14} /> {t("ctx.newNote")}
        </button>
        <button class="btn-secondary" onClick={props.onImport}>
          <Icon name="import" size={14} /> {t("ctx.import")}
        </button>
        <button class="btn-secondary" onClick={() => setPanelOpen(true)}>
          <Icon name="spark" size={14} /> {t("panel.suggestions")}
        </button>
      </div>
      <ul class="welcome-keys">
        <li><kbd>⌘K</kbd> {t("welcome.keySearch")}</li>
        <li><kbd>⌘N</kbd> {t("welcome.keyNew")}</li>
        <li><kbd>⌘J</kbd> {t("welcome.keyAsk")}</li>
      </ul>
    </div>
  );
}
