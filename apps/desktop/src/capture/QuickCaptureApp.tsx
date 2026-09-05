/**
 * 全局快捷键浮窗（FR-CAP-01，v0.7）：CmdOrCtrl+Shift+L 唤起的置顶速记小窗。
 *
 * 同一 dist 的 `#quick-capture` 路由渲染（main.tsx 分流，不带主界面）。
 * Ctrl+Enter 保存到当日 daily note（复用 quick_capture 命令，条目带时间戳），
 * 成功后广播 quick-note-saved（主窗口刷新文件树）并隐藏；Esc 直接隐藏。
 */
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { t } from "../i18n";

export function QuickCaptureApp() {
  const [text, setText] = createSignal("");
  const [saving, setSaving] = createSignal(false);
  const [saved, setSaved] = createSignal(false);
  let taRef: HTMLTextAreaElement | undefined;
  let unlistenFocus: (() => void) | null = null;

  const hide = () => void getCurrentWindow().hide();

  onMount(async () => {
    taRef?.focus();
    // 每次热键重新 show+focus 窗口时，焦点回到输入框
    unlistenFocus = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) taRef?.focus();
    });
  });
  onCleanup(() => unlistenFocus?.());

  const save = async () => {
    const v = text().trim();
    if (!v || saving()) return;
    setSaving(true);
    try {
      await invoke<string>("quick_capture", { text: v });
      setText("");
      setSaved(true);
      void emit("quick-note-saved");
      setTimeout(() => {
        setSaved(false);
        hide();
      }, 600);
    } catch (e) {
      console.error("quick capture", e);
    } finally {
      setSaving(false);
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      hide();
    } else if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void save();
    }
  };

  return (
    <div class="quick-capture">
      <div class="quick-capture-titlebar" data-tauri-drag-region>
        <span data-tauri-drag-region>LMNotes</span>
        <button class="link-btn" onClick={hide}>
          ✕
        </button>
      </div>
      <textarea
        ref={taRef}
        class="quick-capture-input"
        placeholder={t("quickCapture.placeholder")}
        value={text()}
        onInput={(e) => setText(e.currentTarget.value)}
        onKeyDown={onKeyDown}
      />
      <div class="quick-capture-footer">
        <Show when={saved()}>
          <span class="quick-capture-saved">{t("quickCapture.saved")}</span>
        </Show>
        <span class="quick-capture-hint">{t("quickCapture.hint")}</span>
      </div>
    </div>
  );
}
