import { createSignal, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

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
          placeholder="快速记一条…（Esc 关闭，Ctrl+Enter 保存）"
          value={text()}
          onInput={(e) => setText(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") props.onClose();
            if (e.key === "Enter" && e.ctrlKey) submit();
          }}
        />
        <Show when={saving()}>
          <span class="muted small">保存中…</span>
        </Show>
      </div>
    </div>
  );
}
