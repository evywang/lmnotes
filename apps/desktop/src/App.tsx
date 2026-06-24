import { For, Show, createSignal, onCleanup } from "solid-js";
import { useVault, runSearch } from "./store/vault";
import { Editor } from "./editor/Editor";
import { Capture } from "./capture/Capture";
import { SuggestionCenter } from "./suggestions/SuggestionCenter";
import { ProviderSettings } from "./settings/ProviderSettings";
import { ChatDrawer } from "./chat/ChatDrawer";

export function App() {
  const { query, setQuery, results, searching, activePath, setActivePath } = useVault();
  const [captureOpen, setCaptureOpen] = createSignal(false);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [chatOpen, setChatOpen] = createSignal(false);

  const onKeyDown = (e: KeyboardEvent) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
      e.preventDefault();
      setCaptureOpen(true);
    }
    if ((e.ctrlKey || e.metaKey) && e.key === ",") {
      e.preventDefault();
      setSettingsOpen(true);
    }
    if ((e.ctrlKey || e.metaKey) && (e.key.toLowerCase() === "j" || e.code === "KeyJ")) {
      e.preventDefault();
      setChatOpen(true);
    }
  };
  window.addEventListener("keydown", onKeyDown);
  onCleanup(() => window.removeEventListener("keydown", onKeyDown));

  return (
    <>
      <div class="layout">
        <aside class="sidebar">
          <input
            class="search-input"
            placeholder="搜索…（回车）"
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && runSearch(query())}
          />
          <button class="chat-btn" onClick={() => setChatOpen(true)}>
            💬 Chat with Vault (Ctrl+J)
          </button>
          <Show when={searching()}>
            <p class="muted">搜索中…</p>
          </Show>
          <ul class="result-list">
            <For each={results()}>
              {(r) => (
                <li>
                  <button class="result-item" onClick={() => setActivePath(r.path)}>
                    <span class="result-title">{r.title || r.path}</span>
                    <span class="result-path">{r.path}</span>
                  </button>
                </li>
              )}
            </For>
          </ul>
          <Show when={!searching() && results().length === 0}>
            <p class="muted small">输入关键词搜索笔记</p>
          </Show>
        </aside>

        <main class="content">
          <Show when={activePath()} fallback={<p class="placeholder">选择左侧笔记或搜索</p>}>
            <Editor path={activePath()!} />
          </Show>
        </main>

        <aside class="backrefs">
          <h3 class="panel-title">建议中心</h3>
          <SuggestionCenter />
        </aside>
      </div>

      <button class="settings-btn" title="Provider 设置 (Ctrl+,)" onClick={() => setSettingsOpen(true)}>
        ⚙
      </button>

      <Show when={captureOpen()}>
        <Capture onClose={() => setCaptureOpen(false)} />
      </Show>
      <Show when={settingsOpen()}>
        <ProviderSettings onClose={() => setSettingsOpen(false)} />
      </Show>
      <Show when={chatOpen()}>
        <ChatDrawer
          onClose={() => setChatOpen(false)}
          onNavigate={(path) => setActivePath(path)}
        />
      </Show>
    </>
  );
}
