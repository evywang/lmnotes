import { For, Show, createSignal, onCleanup } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useVault, runSearch } from "./store/vault";
import { Editor } from "./editor/Editor";
import { Capture } from "./capture/Capture";
import { SuggestionCenter } from "./suggestions/SuggestionCenter";
import { ProviderSettings } from "./settings/ProviderSettings";
import { ChatDrawer } from "./chat/ChatDrawer";
import { FileTree } from "./components/FileTree";
import { t } from "./i18n";

export function App() {
  const { query, setQuery, results, searching, activePath, setActivePath } = useVault();
  const [captureOpen, setCaptureOpen] = createSignal(false);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [chatOpen, setChatOpen] = createSignal(false);
  const [treeRefresh, setTreeRefresh] = createSignal(0);
  const [treeOpen, setTreeOpen] = createSignal(false);

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

  const createNote = async () => {
    const title = window.prompt(t("app.noteTitlePrompt"), t("app.newNoteTitle"));
    if (!title) return;
    try {
      const path = await invoke<string>("create_note", { title });
      setActivePath(path);
      runSearch("");
      setTreeRefresh((n) => n + 1);
    } catch (e) {
      console.error("create note", e);
    }
  };

  const importNote = async () => {
    const selected = await open({
      multiple: false,
      filters: [
        { name: t("app.importFilterName"), extensions: ["md", "markdown", "txt", "pdf", "docx", "xlsx", "xls"] },
      ],
    });
    if (!selected || typeof selected !== "string") return;
    try {
      const path = await invoke<string>("import_document", { filePath: selected });
      setActivePath(path);
      runSearch("");
      setTreeRefresh((n) => n + 1);
    } catch (e) {
      console.error("import note", e);
    }
  };

  return (
    <>
      <div class="layout">
        <aside class="sidebar">
          <input
            class="search-input"
            placeholder={t("app.searchPlaceholder")}
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && runSearch(query())}
          />
          <div class="sidebar-actions">
            <button class="action-btn" onClick={createNote} title={t("app.newNoteTooltip")}>
              {t("app.newNoteBtn")}
            </button>
            <button class="action-btn" onClick={importNote} title={t("app.importTooltip")}>
              {t("app.importBtn")}
            </button>
          </div>
          <button class="chat-btn" onClick={() => setChatOpen(true)}>
            {t("app.chatBtn")}
          </button>
          <Show when={searching()}>
            <p class="muted">{t("app.searching")}</p>
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
            <p class="muted small">{t("app.searchHint")}</p>
          </Show>
          <div class={`tree-stack ${treeOpen() ? "tree-stack-open" : ""}`}>
            <button
              class="tree-stack-header"
              onClick={() => setTreeOpen((v) => !v)}
            >
              <span class="tree-stack-arrow">{treeOpen() ? "▼" : "▶"}</span>
              <span>{t("app.files")}</span>
            </button>
            <Show when={treeOpen()}>
              <div class="tree-stack-body">
                <FileTree
                  onOpen={(path) => setActivePath(path)}
                  activePath={activePath}
                  refreshKey={treeRefresh}
                />
              </div>
            </Show>
          </div>
        </aside>

        <main class="content">
          <Show when={activePath()} fallback={<p class="placeholder">{t("app.placeholder")}</p>}>
            <Editor path={activePath()!} />
          </Show>
        </main>

        <aside class="backrefs">
          <h3 class="panel-title">{t("app.suggestionCenter")}</h3>
          <SuggestionCenter />
        </aside>
      </div>

      <button class="settings-btn" title={t("app.settingsTooltip")} onClick={() => setSettingsOpen(true)}>
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
