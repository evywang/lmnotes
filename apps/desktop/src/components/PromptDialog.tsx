/**
 * 带标题的文本输入对话框（替代 window.prompt）。
 *
 * 浏览器原生 window.prompt/alert/confirm 的标题固定显示页面 URL（如
 * localhost:1420 或 tauri://localhost），无法自定义——在打包应用里很不专业。
 * 本组件提供应用名标题（LMNotes）+ 默认值 + Enter/Esc 快捷键的模态输入框。
 *
 * 用法（promise 风格，与 window.prompt 对齐）：
 *   const name = await showPrompt(t("app.noteTitlePrompt"), t("app.newNoteTitle"));
 *   if (!name) return; // 用户取消
 *
 * 需在 App 根部挂一次 <PromptDialogHost />。
 */
import { createSignal, Show } from "solid-js";
import { t } from "../i18n";

/** 应用名（与 tauri.conf.json productName 一致），用作所有内置对话框标题。 */
export const APP_NAME = "LMNotes";

interface PromptRequest {
  message: string;
  defaultValue: string;
  resolve: (value: string | null) => void;
}

const [pending, setPending] = createSignal<PromptRequest | null>(null);

/** 弹出输入对话框；确认返回文本，取消返回 null。同一时间只允许一个。 */
export function showPrompt(message: string, defaultValue = ""): Promise<string | null> {
  // 若已有未关闭的请求，先按取消结算，避免 promise 悬挂
  pending()?.resolve(null);
  return new Promise((resolve) => {
    setPending({ message, defaultValue, resolve });
  });
}

export function PromptDialogHost() {
  let inputRef: HTMLInputElement | undefined;

  const ok = () => {
    const p = pending();
    if (!p) return;
    setPending(null);
    const v = inputRef?.value.trim();
    p.resolve(v ? v : null); // 空输入视同取消（与原 window.prompt 用法一致：!title return）
  };

  const cancel = () => {
    const p = pending();
    if (!p) return;
    setPending(null);
    p.resolve(null);
  };

  return (
    <Show when={pending()}>
      <div class="capture-overlay" onClick={cancel}>
        <div class="prompt-dialog" onClick={(e) => e.stopPropagation()}>
          <div class="prompt-titlebar">{APP_NAME}</div>
          <p class="prompt-message">{pending()!.message}</p>
          <input
            ref={(el) => {
              inputRef = el;
              // 元素插入后聚焦并全选默认值（rAF 确保 DOM 已挂载）
              requestAnimationFrame(() => {
                el.focus();
                el.select();
              });
            }}
            class="prompt-input"
            type="text"
            value={pending()!.defaultValue}
            onKeyDown={(e) => {
              if (e.key === "Enter") ok();
              if (e.key === "Escape") cancel();
            }}
          />
          <div class="prompt-actions">
            <button class="btn-secondary" onClick={cancel}>
              {t("dialog.cancel")}
            </button>
            <button class="btn-primary" onClick={ok}>
              {t("dialog.ok")}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
