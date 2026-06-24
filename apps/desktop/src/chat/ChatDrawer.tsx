import { createSignal, Show, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface CitationRef {
  index: number;
  concept_id: string;
  path: string;
}

export function ChatDrawer(props: {
  onClose: () => void;
  onNavigate: (path: string) => void;
}) {
  const [input, setInput] = createSignal("");
  const [answer, setAnswer] = createSignal("");
  const [citations, setCitations] = createSignal<CitationRef[]>([]);
  const [streaming, setStreaming] = createSignal(false);
  const [error, setError] = createSignal("");

  // 监听 chat-chunk 事件（流式）
  const unlistenChunk = listen<string>("chat-chunk", (e) => {
    setAnswer((prev) => prev + e.payload);
  });
  const unlistenError = listen<string>("chat-error", (e) => {
    setError(e.payload);
    setStreaming(false);
  });
  onCleanup(() => {
    unlistenChunk.then((f) => f());
    unlistenError.then((f) => f());
  });

  const ask = async () => {
    const q = input().trim();
    if (!q) return;
    setAnswer("");
    setError("");
    setCitations([]);
    setStreaming(true);
    try {
      const cites = await invoke<CitationRef[]>("chat_stream", { query: q });
      setCitations(cites);
    } catch (e) {
      setError(String(e));
    } finally {
      setStreaming(false);
    }
  };

  // 将 [n] 引用渲染为可点击 chip
  const renderAnswer = (text: string) => {
    const parts = text.split(/(\[\d+\])/g);
    return parts.map((part) => {
      const match = part.match(/^\[(\d+)\]$/);
      if (match) {
        const n = parseInt(match[1]);
        const cite = citations().find((c) => c.index === n);
        return cite
          ? `<button class="cite-chip" data-path="${cite.path}">[${n}]</button>`
          : `[${n}]`;
      }
      return part
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\n/g, "<br>");
    }).join("");
  };

  return (
    <div class="chat-drawer">
      <div class="chat-header">
        <h3>Chat with Vault</h3>
        <button class="chat-close" onClick={props.onClose}>
          ✕
        </button>
      </div>
      <div class="chat-body">
        <Show when={answer()}>
          <div
            class="chat-answer"
            innerHTML={renderAnswer(answer())}
            onClick={(e) => {
              const target = e.target as HTMLElement;
              if (target.classList.contains("cite-chip")) {
                const path = target.dataset.path;
                if (path) {
                  props.onNavigate(path);
                  props.onClose();
                }
              }
            }}
          />
        </Show>
        <Show when={error()}>
          <div class="chat-error">{error()}</div>
        </Show>
        <Show when={citations().length > 0}>
          <div class="chat-citations">
            <p class="muted small">引用：</p>
            {citations().map((c) => (
              <button
                class="cite-ref"
                onClick={() => {
                  props.onNavigate(c.path);
                  props.onClose();
                }}
              >
                [{c.index}] {c.path}
              </button>
            ))}
          </div>
        </Show>
      </div>
      <div class="chat-input-bar">
        <input
          class="chat-input"
          placeholder="问一个关于你笔记的问题…"
          value={input()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && !e.shiftKey && ask()}
          disabled={streaming()}
        />
        <Show when={streaming()}>
          <span class="chat-streaming">●</span>
        </Show>
      </div>
    </div>
  );
}
