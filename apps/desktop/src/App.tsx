import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useVault, runSearch } from "./store/vault";
import { Editor } from "./editor/Editor";
import { Capture } from "./capture/Capture";
import { SuggestionCenter } from "./suggestions/SuggestionCenter";
import { ProviderSettings } from "./settings/ProviderSettings";
import { VoiceCapture } from "./voice/VoiceCapture";
import { MediaTasksButton, MediaTasksPanel, initMediaTaskFeed, openMediaTasks } from "./voice/MediaTasks";
import { ChatDrawer } from "./chat/ChatDrawer";
import { KnowledgeGraph } from "./graph/KnowledgeGraph";
import { FileTree } from "./components/FileTree";
import { PromptDialogHost, showPrompt } from "./components/PromptDialog";
import { CommandPalette, type PaletteAction } from "./components/CommandPalette";
import { t } from "./i18n";

/** 侧栏当前库指示（v0.4 多库）：显示库名，点击打开设置切换。 */
function VaultBadge(props: { onOpenSettings: () => void }) {
  const [name, setName] = createSignal<string | null>(null);
  onMount(async () => {
    try {
      const vs = await invoke<{ name: string; current: boolean }[]>("list_vaults");
      setName(vs.find((v) => v.current)?.name ?? null);
    } catch {
      // 静默：指示器失败不打扰
    }
  });
  return (
    <Show when={name()}>
      <button class="vault-badge" title={t("vault.badgeTooltip")} onClick={props.onOpenSettings}>
        📚 {name()}
      </button>
    </Show>
  );
}

export function App() {
  const { query, setQuery, results, searching, activePath, setActivePath } = useVault();
  const [captureOpen, setCaptureOpen] = createSignal(false);
  const [voiceOpen, setVoiceOpen] = createSignal(false);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [chatOpen, setChatOpen] = createSignal(false);
  const [graphOpen, setGraphOpen] = createSignal(false);
  const [paletteOpen, setPaletteOpen] = createSignal(false);
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
    if ((e.ctrlKey || e.metaKey) && (e.key.toLowerCase() === "g" || e.code === "KeyG")) {
      e.preventDefault();
      setGraphOpen(true);
    }
    // 语音输入：Ctrl/Cmd+Shift+V（避开 Ctrl+V 粘贴）
    if (
      (e.ctrlKey || e.metaKey) &&
      e.shiftKey &&
      (e.key.toLowerCase() === "v" || e.code === "KeyV")
    ) {
      e.preventDefault();
      setVoiceOpen(true);
    }
    // 命令面板（FR-SEARCH-01）：Ctrl/Cmd+K
    if ((e.ctrlKey || e.metaKey) && (e.key.toLowerCase() === "k" || e.code === "KeyK")) {
      e.preventDefault();
      setPaletteOpen(true);
    }
  };
  onMount(() => initMediaTaskFeed());
  window.addEventListener("keydown", onKeyDown);
  onCleanup(() => window.removeEventListener("keydown", onKeyDown));

  // 模板清单（懒加载一次）；新建时若选了模板 → create_note_from_template
  let templatesCache: { name: string; path: string }[] | null = null;
  const loadTemplates = async () => {
    if (!templatesCache) {
      try {
        templatesCache = await invoke<{ name: string; path: string }[]>("list_templates");
      } catch {
        templatesCache = [];
      }
    }
    return templatesCache;
  };

  const createNote = async () => {
    const title = await showPrompt(t("app.noteTitlePrompt"), t("app.newNoteTitle"));
    if (!title) return;
    try {
      const templates = await loadTemplates();
      let path: string;
      if (templates.length > 0) {
        // 有模板：二次弹选（含"空白笔记"项）
        const names = [t("app.noTemplate"), ...templates.map((t2) => t2.name)];
        const picked = await showPrompt(t("app.templatePrompt"), "");
        // showPrompt 只能输入文本；模板较多时体验一般——保持轻量：输入名称精确匹配，空 = 空白
        const wanted = picked?.trim();
        const tpl = wanted ? templates.find((t2) => t2.name === wanted) : undefined;
        if (wanted && !tpl) return; // 输入了未知模板名 → 取消
        path = tpl
          ? await invoke<string>("create_note_from_template", {
              templatePath: tpl.path,
              title,
            })
          : await invoke<string>("create_note", { title });
      } else {
        path = await invoke<string>("create_note", { title });
      }
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

  // 命令面板动作表（FR-SEARCH-01）：标签走 i18n，执行闭包复用既有入口。
  const paletteActions = (): PaletteAction[] => [
    { id: "new-note", icon: "📝", label: t("palette.newNote"), run: () => void createNote() },
    { id: "quick-capture", icon: "⚡", label: t("palette.quickCapture"), run: () => setCaptureOpen(true) },
    { id: "voice", icon: "🎤", label: t("palette.voice"), run: () => setVoiceOpen(true) },
    { id: "chat", icon: "💬", label: t("palette.chat"), run: () => setChatOpen(true) },
    { id: "graph", icon: "🕸", label: t("palette.graph"), run: () => setGraphOpen(true) },
    {
      id: "daily",
      icon: "📅",
      label: t("palette.daily"),
      run: () => {
        void (async () => {
          try {
            const path = await invoke<string>("open_or_create_daily");
            setActivePath(path);
            setTreeRefresh((n) => n + 1);
          } catch (e) {
            console.error("open daily", e);
          }
        })();
      },
    },
    { id: "tasks", icon: "⏳", label: t("palette.tasks"), run: () => openMediaTasks() },
    { id: "settings", icon: "⚙", label: t("palette.settings"), run: () => setSettingsOpen(true) },
  ];

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
            <button
              class="action-btn"
              onClick={() => setVoiceOpen(true)}
              title={t("app.voiceTooltip")}
            >
              {t("app.voiceBtn")}
            </button>
          </div>
          <VaultBadge onOpenSettings={() => setSettingsOpen(true)} />
          <MediaTasksButton />
          <button class="chat-btn" onClick={() => setChatOpen(true)}>
            {t("app.chatBtn")}
          </button>
          <button class="chat-btn" onClick={() => setGraphOpen(true)}>
            {t("app.graphBtn")}
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
          {/* keyed：路径变化即重挂载 Editor。否则 <Show> 在 truthy→truthy 切换时
              不重挂载，Editor 的 onMount（内容加载）只跑一次，点别的文件不换内容 */}
          <Show when={activePath()} keyed fallback={<p class="placeholder">{t("app.placeholder")}</p>}>
            {(path) => <Editor path={path} onNavigate={setActivePath} />}
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

      {/* 命令面板（FR-SEARCH-01） */}
      <CommandPalette
        open={paletteOpen()}
        onClose={() => setPaletteOpen(false)}
        onOpenNote={(path) => setActivePath(path)}
        actions={paletteActions}
      />

      {/* 文本输入对话框宿主（应用名标题，替代 window.prompt） */}
      <PromptDialogHost />

      <Show when={captureOpen()}>
        <Capture onClose={() => setCaptureOpen(false)} />
      </Show>
      <MediaTasksPanel />
      <Show when={voiceOpen()}>
        <VoiceCapture
          onClose={() => setVoiceOpen(false)}
          onNavigate={(path) => {
            setActivePath(path);
            setTreeRefresh((n) => n + 1);
          }}
          onOpenSettings={() => setSettingsOpen(true)}
        />
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
      <Show when={graphOpen()}>
        <KnowledgeGraph
          mode="drawer"
          onClose={() => setGraphOpen(false)}
          onNavigate={(path) => {
            setActivePath(path);
            setGraphOpen(false);
          }}
        />
      </Show>
    </>
  );
}
