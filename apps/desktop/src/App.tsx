import { Show, createSignal, onCleanup, onMount } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, message as dialogMessage } from "@tauri-apps/plugin-dialog";
import { useVault, runSearch } from "./store/vault";
import { setContextView } from "./store/ui";
import { loadSuggestions } from "./store/llm";
import { Editor } from "./editor/Editor";
import { Capture } from "./capture/Capture";
import { ProviderSettings } from "./settings/ProviderSettings";
import { VoiceCapture } from "./voice/VoiceCapture";
import { MediaTasksPanel, initMediaTaskFeed, openMediaTasks } from "./voice/MediaTasks";
import { ChatDrawer } from "./chat/ChatDrawer";
import { KnowledgeGraph } from "./graph/KnowledgeGraph";
import { TimelineView } from "./components/TimelineView";
import { PromptDialogHost } from "./components/PromptDialog";
import { CommandPalette, type PaletteAction } from "./components/CommandPalette";
import { Rail } from "./components/Rail";
import { ContextColumn } from "./components/ContextColumn";
import { RightPanel } from "./components/RightPanel";
import { Welcome } from "./components/Welcome";
import { allThemes, setTheme } from "./theme";
import { t } from "./i18n";

export function App() {
  const { setQuery, activePath, setActivePath } = useVault();
  const [captureOpen, setCaptureOpen] = createSignal(false);
  const [voiceOpen, setVoiceOpen] = createSignal(false);
  const [settingsOpen, setSettingsOpen] = createSignal(false);
  const [chatOpen, setChatOpen] = createSignal(false);
  const [graphOpen, setGraphOpen] = createSignal(false);
  const [paletteOpen, setPaletteOpen] = createSignal(false);
  const [timelineOpen, setTimelineOpen] = createSignal(false);
  const [timelineTag, setTimelineTag] = createSignal<string | null>(null);
  const [reviewBusy, setReviewBusy] = createSignal(false);
  // 面板直达问答（v0.9）：palette/上下文栏写入 → ChatDrawer 消费后自动发送
  const [askQuestion, setAskQuestion] = createSignal<string | null>(null);
  const [treeRefresh, setTreeRefresh] = createSignal(0);

  // 快捷键（v1.0 spec §3.5）：⌘N 语义由「快速捕获浮窗」改为「直接新建笔记」；
  // 浮窗入口保留在命令面板 + 全局热键（Ctrl+Shift+L）。新增 ⌘1/2/3 视图、⌘D 今日。
  const onKeyDown = (e: KeyboardEvent) => {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod) return;
    const k = e.key.toLowerCase();
    if (k === "n" && !e.shiftKey) {
      e.preventDefault();
      void createNote();
    } else if (k === ",") {
      e.preventDefault();
      setSettingsOpen(true);
    } else if (k === "j") {
      e.preventDefault();
      setChatOpen(true);
    } else if (k === "g") {
      e.preventDefault();
      setGraphOpen(true);
    } else if (k === "v" && e.shiftKey) {
      e.preventDefault();
      setVoiceOpen(true);
    } else if (k === "k") {
      e.preventDefault();
      setPaletteOpen(true);
    } else if (k === "d") {
      e.preventDefault();
      void openDaily();
    } else if (e.key === "1" || e.key === "2" || e.key === "3") {
      e.preventDefault();
      setContextView((["notes", "tags", "files"] as const)[Number(e.key) - 1]);
    }
  };
  onMount(() => {
    initMediaTaskFeed();
    void loadSuggestions(); // 右面板计数在面板未开时也要就绪
    // 全局快捷键浮窗保存 / 库导入 / 回顾生成 后：刷新列表、搜索与建议
    const refresh = () => {
      setTreeRefresh((n) => n + 1);
      runSearch("");
      void loadSuggestions();
    };
    void listen("quick-note-saved", refresh);
    void listen("vault-changed", refresh);
    // Editor 顶栏生成回顾后派发的本地刷新（回顾文件不走上述两个 Tauri 事件）
    window.addEventListener("lmnotes:refresh", refresh);
    onCleanup(() => window.removeEventListener("lmnotes:refresh", refresh));
  });
  window.addEventListener("keydown", onKeyDown);
  onCleanup(() => window.removeEventListener("keydown", onKeyDown));

  // 新建笔记（spec §6）：零弹窗直接建草稿，标题=「未命名 时间戳」，在编辑器内改；
  // 模板走上下文栏 ＋ 按钮的 ▾ 菜单（createNote(templatePath)）。
  const createNote = async (templatePath?: string) => {
    const now = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const title = `${t("app.untitled")} ${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(
      now.getDate(),
    )} ${pad(now.getHours())}:${pad(now.getMinutes())}`;
    try {
      const path = templatePath
        ? await invoke<string>("create_note_from_template", { templatePath, title })
        : await invoke<string>("create_note", { title });
      setActivePath(path);
      setQuery("");
      setTreeRefresh((n) => n + 1);
    } catch (e) {
      console.error("create note", e);
    }
  };

  const importNote = async () => {
    const selected = await open({
      multiple: false,
      filters: [
        {
          name: t("app.importFilterName"),
          extensions: ["md", "markdown", "txt", "pdf", "docx", "xlsx", "xls"],
        },
      ],
    });
    if (!selected || typeof selected !== "string") return;
    try {
      const path = await invoke<string>("import_document", { filePath: selected });
      setActivePath(path);
      setQuery("");
      setTreeRefresh((n) => n + 1);
    } catch (e) {
      console.error("import note", e);
    }
  };

  // 今日笔记（FR-SEARCH-05）：幂等打开/创建。
  const openDaily = async () => {
    try {
      const path = await invoke<string>("open_or_create_daily");
      setActivePath(path);
      setTreeRefresh((n) => n + 1);
    } catch (e) {
      console.error("open daily", e);
    }
  };

  const openTimeline = (tag: string | null) => {
    setTimelineTag(tag);
    setTimelineOpen(true);
  };

  // 每日/每周回顾（FR-LLM-07）：palette 入口（编辑器顶栏 ✦ 菜单为另一入口）。
  const generateReview = async (range: "daily" | "weekly") => {
    if (reviewBusy()) return;
    setReviewBusy(true);
    try {
      const path = await invoke<string>("generate_review", { range });
      setActivePath(path);
      setTreeRefresh((n) => n + 1);
      runSearch("");
    } catch (e) {
      console.error("generate review", e);
      void dialogMessage(String(e), { title: "LMNotes", kind: "error" });
    } finally {
      setReviewBusy(false);
    }
  };

  // 命令面板动作表（FR-SEARCH-01）：图标 Emoji 暂留，P2 统一换 Icon。
  const paletteActions = (): PaletteAction[] => [
    { id: "new-note", icon: "📝", label: t("palette.newNote"), run: () => void createNote() },
    { id: "quick-capture", icon: "⚡", label: t("palette.quickCapture"), run: () => setCaptureOpen(true) },
    { id: "voice", icon: "🎤", label: t("palette.voice"), run: () => setVoiceOpen(true) },
    { id: "chat", icon: "💬", label: t("palette.chat"), run: () => setChatOpen(true) },
    { id: "graph", icon: "🕸", label: t("palette.graph"), run: () => setGraphOpen(true) },
    { id: "timeline", icon: "🕘", label: t("palette.timeline"), run: () => openTimeline(null) },
    { id: "daily", icon: "📅", label: t("palette.daily"), run: () => void openDaily() },
    { id: "daily-review", icon: "🗓", label: t("palette.dailyReview"), run: () => void generateReview("daily") },
    { id: "weekly-review", icon: "📆", label: t("palette.weeklyReview"), run: () => void generateReview("weekly") },
    { id: "tasks", icon: "⏳", label: t("palette.tasks"), run: () => openMediaTasks() },
    { id: "settings", icon: "⚙", label: t("palette.settings"), run: () => setSettingsOpen(true) },
    ...allThemes().map((th) => ({
      id: `theme-${th.id}`,
      icon: "◐",
      label: `${t("palette.switchTheme")}: ${th.builtin ? t(th.nameKey!) : th.name}`,
      run: () => setTheme(th.id),
    })),
    {
      id: "theme-auto",
      icon: "◐",
      label: `${t("palette.switchTheme")}: ${t("settings.themeAuto")}`,
      run: () => setTheme("auto"),
    },
  ];

  return (
    <>
      <div class="app-shell">
        <Rail
          active={chatOpen() ? "chat" : graphOpen() ? "graph" : null}
          onDaily={() => void openDaily()}
          onTimeline={() => openTimeline(null)}
          onGraph={() => setGraphOpen(true)}
          onAsk={() => setChatOpen(true)}
          onTasks={() => openMediaTasks()}
          onSettings={() => setSettingsOpen(true)}
        />
        <ContextColumn
          refreshKey={treeRefresh()}
          onOpenNote={setActivePath}
          onAsk={(q) => {
            setAskQuestion(q);
            setChatOpen(true);
          }}
          onVoice={() => setVoiceOpen(true)}
          onImport={importNote}
          onNewNote={() => void createNote()}
          onNewFromTemplate={(p) => void createNote(p)}
        />
        <main class="main-col">
          {/* keyed：路径变化即重挂载 Editor（同 v0.9 行为，防止切换文件不刷内容） */}
          <Show
            when={activePath()}
            keyed
            fallback={<Welcome onNew={() => void createNote()} onImport={importNote} />}
          >
            {(path) => <Editor path={path} onNavigate={setActivePath} />}
          </Show>
        </main>
        <RightPanel />
      </div>

      <CommandPalette
        open={paletteOpen()}
        onClose={() => setPaletteOpen(false)}
        onOpenNote={setActivePath}
        actions={paletteActions}
        onAsk={(q) => {
          setAskQuestion(q);
          setChatOpen(true);
        }}
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
          onNavigate={setActivePath}
          pendingQuestion={askQuestion()}
          onQuestionConsumed={() => setAskQuestion(null)}
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
      <Show when={timelineOpen()}>
        <TimelineView
          tag={timelineTag()}
          onClose={() => setTimelineOpen(false)}
          onOpen={(path) => {
            setActivePath(path);
            setTimelineOpen(false);
          }}
        />
      </Show>
    </>
  );
}
