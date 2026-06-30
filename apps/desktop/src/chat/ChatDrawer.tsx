import { createSignal, For, Show, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { t } from "../i18n";

interface CitationRef {
  index: number;
  concept_id: string;
  path: string;
}

interface ChatMessage {
  role: "user" | "assistant";
  content: string;
  citations?: CitationRef[];
}

interface HistoryRow {
  id: number;
  role: string;
  content: string;
  citations: string | null;
}

export function ChatDrawer(props: {
  onClose: () => void;
  onNavigate: (path: string) => void;
}) {
  const [messages, setMessages] = createSignal<ChatMessage[]>([]);
  const [input, setInput] = createSignal("");
  const [streaming, setStreaming] = createSignal(false);
  const [error, setError] = createSignal("");
  let bodyRef: HTMLDivElement | undefined;

  // 加载历史
  onMount(async () => {
    try {
      const rows = await invoke<HistoryRow[]>("load_chat_history");
      if (rows.length > 0) {
        const restored: ChatMessage[] = rows.map((r) => ({
          role: r.role as "user" | "assistant",
          content: r.content,
          citations: r.citations
            ? (JSON.parse(r.citations).map((c: CitationRef) => c))
            : undefined,
        }));
        setMessages(restored);
        scrollToBottom();
      }
    } catch (e) {
      console.error("load history", e);
    }
  });

  // 监听 chat-chunk 事件（流式追加到最后一条 assistant 消息）
  const unlistenChunk = listen<string>("chat-chunk", (e) => {
    setMessages((prev) => {
      const last = prev[prev.length - 1];
      if (last && last.role === "assistant" && streaming()) {
        // 追加到最后一条
        return [...prev.slice(0, -1), { ...last, content: last.content + e.payload }];
      }
      return prev;
    });
    scrollToBottom();
  });
  const unlistenError = listen<string>("chat-error", (e) => {
    setError(e.payload);
    setStreaming(false);
  });
  onCleanup(() => {
    unlistenChunk.then((f) => f());
    unlistenError.then((f) => f());
  });

  const scrollToBottom = () => {
    setTimeout(() => {
      if (bodyRef) bodyRef.scrollTop = bodyRef.scrollHeight;
    }, 50);
  };

  const ask = async () => {
    const q = input().trim();
    if (!q || streaming()) return;
    setInput("");
    setError("");

    // 添加用户消息 + 空 assistant 消息（流式填充）
    setMessages((prev) => [
      ...prev,
      { role: "user", content: q },
      { role: "assistant", content: "" },
    ]);
    setStreaming(true);
    scrollToBottom();

    // 构建历史（不含刚加的空 assistant）
    const history = messages()
      .filter((_, i) => i < messages().length)
      .map((m) => ({ role: m.role, content: m.content }));

    try {
      const cites = await invoke<CitationRef[]>("chat_stream", {
        query: q,
        history,
      });
      // 更新最后一条 assistant 消息的 citations
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last && last.role === "assistant") {
          return [...prev.slice(0, -1), { ...last, citations: cites }];
        }
        return prev;
      });
    } catch (e) {
      setError(String(e));
      // 移除空的 assistant 消息
      setMessages((prev) => {
        const last = prev[prev.length - 1];
        if (last && last.role === "assistant" && !last.content) {
          return prev.slice(0, -1);
        }
        return prev;
      });
    } finally {
      setStreaming(false);
    }
  };

  const clearHistory = async () => {
    try {
      await invoke("clear_chat_history");
      setMessages([]);
    } catch (e) {
      console.error("clear history", e);
    }
  };

  const renderContent = (text: string, cites?: CitationRef[]) => {
    if (!cites || cites.length === 0) {
      return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\n/g, "<br>");
    }
    const parts = text.split(/(\[\d+\])/g);
    return parts
      .map((part) => {
        const match = part.match(/^\[(\d+)\]$/);
        if (match) {
          const n = parseInt(match[1]);
          const cite = cites?.find((c) => c.index === n);
          return cite
            ? `<button class="cite-chip" data-path="${cite.path}">[${n}]</button>`
            : `[${n}]`;
        }
        return part
          .replace(/&/g, "&amp;")
          .replace(/</g, "&lt;")
          .replace(/>/g, "&gt;")
          .replace(/\n/g, "<br>");
      })
      .join("");
  };

  return (
    <div class="chat-drawer">
      <div class="chat-header">
        <h3>{t("chat.title")}</h3>
        <div class="chat-header-actions">
          <Show when={messages().length > 0}>
            <button class="chat-clear-btn" title={t("chat.clearTooltip")} onClick={clearHistory}>
              {t("chat.clear")}
            </button>
          </Show>
          <button class="chat-close" onClick={props.onClose}>
            ✕
          </button>
        </div>
      </div>
      <div class="chat-body" ref={bodyRef}>
        <Show when={messages().length === 0}>
          <div class="chat-empty">
            <p class="muted">{t("chat.emptyPrompt")}</p>
            <p class="muted small">{t("chat.emptyExample")}</p>
          </div>
        </Show>
        <For each={messages()}>
          {(msg) => (
            <div class={`chat-msg chat-msg-${msg.role}`}>
              <Show when={msg.role === "assistant"}>
                <div class="chat-avatar">🤖</div>
              </Show>
              <div class={`chat-bubble chat-bubble-${msg.role}`}>
                <div
                  class="chat-content"
                  innerHTML={renderContent(msg.content, msg.citations)}
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
                <Show when={msg.citations && msg.citations.length > 0}>
                  <div class="chat-cites">
                    <For each={msg.citations}>
                      {(c) => (
                        <button
                          class="cite-ref"
                          onClick={() => {
                            props.onNavigate(c.path);
                            props.onClose();
                          }}
                        >
                          [{c.index}] {c.path}
                        </button>
                      )}
                    </For>
                  </div>
                </Show>
              </div>
              <Show when={msg.role === "user"}>
                <div class="chat-avatar">👤</div>
              </Show>
            </div>
          )}
        </For>
        <Show when={error()}>
          <div class="chat-error">{error()}</div>
        </Show>
      </div>
      <div class="chat-input-bar">
        <textarea
          class="chat-input"
          placeholder={t("chat.inputPlaceholder")}
          value={input()}
          onInput={(e) => setInput(e.currentTarget.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              ask();
            }
          }}
          disabled={streaming()}
          rows="1"
        />
        <Show when={streaming()}>
          <span class="chat-streaming">●</span>
        </Show>
      </div>
    </div>
  );
}
