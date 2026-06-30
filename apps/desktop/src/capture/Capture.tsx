import { createSignal, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";

export function Capture(props: { onClose: () => void }) {
  const [text, setText] = createSignal("");
  const [saving, setSaving] = createSignal(false);

  const submit = async () => {
    if (!text().trim()) {
      props.onClose();
      return;
    }
    setSaving(true);
    try {
      await invoke("quick_capture", { text: text() });
      props.onClose();
    } catch (e) {
      console.error("quick_capture failed", e);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="capture-overlay" onClick={props.onClose}>
      <div class="capture-box" onClick={(e) => e.stopPropagation()}>
        <textarea
          autofocus
          class="capture-input"
          placeholder={t("capture.placeholder")}
          value={text()}
          onInput={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") props.onClose();
            if (e.key === "Enter" && e.ctrlKey) submit();
          }}
        />
        <Show when={saving()}>
          <span class="muted small">{t("capture.saving")}</span>
        </Show>
      </div>
    </div>
  );
}
