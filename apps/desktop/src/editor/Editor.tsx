import { createSignal, createMemo, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { marked } from "marked";
import { message } from "@tauri-apps/plugin-dialog";
import { useCodeMirror } from "./solid-cm";
import { RewriteMenu } from "./RewriteMenu";
import { HistoryPanel } from "./HistoryPanel";
import { APP_NAME } from "../components/PromptDialog";
import type { EditorView } from "@codemirror/view";
import { t } from "../i18n";

interface ConceptFile {
  text: string;
}

export function Editor(props: { path: string; onNavigate?: (path: string) => void }) {
  let host: HTMLDivElement | undefined;
  const [content, setContent] = createSignal("");
  const [loaded, setLoaded] = createSignal(false);
  const [dirty, setDirty] = createSignal(false);
  const [preview, setPreview] = createSignal(false);
  const [historyOpen, setHistoryOpen] = createSignal(false);
  const [mediaBusy, setMediaBusy] = createSignal(false); // 音视频转录中（FR-CAP-04）
  const [extracting, setExtracting] = createSignal(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let viewGetter = () => undefined as EditorView | undefined;

  // 笔记类型（frontmatter type:）——决定是否显示「抽取行动项」（FR-LLM-06
  // 面向会议记录/语音转录；prompt 也只为这两类设计）
  const noteType = createMemo(() => {
    const m = content().match(/^---\n[\s\S]*?\ntype:\s*(\S+)/);
    return m?.[1] ?? "";
  });
  const canExtractActions = () =>
    noteType() === "transcript" || noteType() === "meeting";

  const saveSnapshotNow = async () => {
    try {
      await invoke("save_snapshot", { conceptPath: props.path, text: viewGetter()?.state.doc.toString() ?? content() });
    } catch (e) {
      console.error("save_snapshot failed", e);
    }
  };

  // 行动项抽取：快照当前内容 → LLM 抽取 → checklist 追加正文尾部
  const extractActions = async () => {
    const view = viewGetter();
    if (!view || extracting()) return;
    setExtracting(true);
    try {
      await invoke("save_snapshot", {
        conceptPath: props.path,
        text: view.state.doc.toString(),
      });
      const result = await invoke<string>("extract_action_items", { path: props.path });
      const insert = `\n\n## 行动项\n\n${result.trim()}\n`;
      const end = view.state.doc.length;
      view.dispatch({
        changes: { from: end, insert },
        selection: { anchor: end + insert.length },
      });
    } catch (e) {
      void message(`${t("actions.extractFailed")}${e}`, {
        title: APP_NAME,
        kind: "error",
      });
    } finally {
      setExtracting(false);
    }
  };

  const onChange = (doc: string) => {
    setContent(doc);
    setDirty(true);
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      invoke("save_concept", { path: props.path, text: doc })
        .then(() => setDirty(false))
        .catch((e) => console.error("save failed", e));
    }, 800);
  };

  // 预览 HTML（实时跟随 content）
  const previewHtml = createMemo(() => {
    // 去掉 frontmatter 后渲染
    const body = content().replace(/^---\n[\s\S]*?\n---\n*/, "");
    return marked.parse(body, { async: false }) as string;
  });

  const handleFiles = async (files: FileList) => {
    for (const f of Array.from(files)) {
      const buf = new Uint8Array(await f.arrayBuffer());
      const ext = f.name.split(".").pop() || "bin";

      // 音视频 → 归档 + 转录成 transcript 笔记（FR-CAP-04）
      if (f.type.startsWith("audio/") || f.type.startsWith("video/")) {
        const kind = f.type.startsWith("video/") ? "video" : "audio";
        setMediaBusy(true);
        try {
          const path = await invoke<string>("create_media_note", {
            data: Array.from(buf),
            ext,
            mime: f.type,
            kind,
            durationMs: null,
            language: null,
            title: null,
          });
          props.onNavigate?.(path);
        } catch (e) {
          console.error("create_media_note failed", e);
          void message(`${t("editor.mediaFailed")}${e}`, {
            title: APP_NAME,
            kind: "error",
          });
        } finally {
          setMediaBusy(false);
        }
        continue;
      }

      // 图片 → 归档 + 插入链接（原有路径）
      if (!f.type.startsWith("image/")) continue;
      try {
        const rel = await invoke<string>("insert_image", {
          data: Array.from(buf),
          ext,
        });
        const view = viewGetter();
        if (view) {
          const sel = view.state.selection.main;
          view.dispatch({
            changes: { from: sel.from, insert: `![${f.name}](${rel})\n` },
          });
        }
        // FR-MEDIA-02：后台生成图片描述（image-desc 笔记）。未配视觉模型时静默跳过。
        invoke<string>("describe_image", { assetRel: rel })
          .then((descPath) => console.log("image described →", descPath))
          .catch(() => {/* 未配视觉 provider 或被护栏拒——不打扰 */});
      } catch (e) {
        console.error("insert_image failed", e);
      }
    }
  };

  onMount(async () => {
    try {
      const file = await invoke<ConceptFile>("read_concept", { path: props.path });
      setContent(file.text);
      setLoaded(true);
      viewGetter = useCodeMirror(() => host, file.text, onChange);
    } catch (e) {
      console.error("read_concept failed", e);
      setLoaded(true);
    }
  });

  return (
    <div class="editor-wrap">
      <div class="editor-toolbar">
        <span class="editor-path">{props.path}</span>
        <Show when={dirty()}>
          <span class="dirty-dot">●</span>
        </Show>
        <Show when={mediaBusy()}>
          <span class="muted small">🎙 {t("editor.mediaTranscribing")}</span>
        </Show>
        <Show when={canExtractActions()}>
          <button
            class="preview-toggle"
            disabled={extracting()}
            onClick={extractActions}
            title={t("editor.extractTooltip")}
          >
            {extracting() ? t("editor.extractBusy") : t("editor.extractActions")}
          </button>
        </Show>
        <button
          class="preview-toggle"
          onClick={() => setHistoryOpen(true)}
          title={t("editor.historyTooltip")}
        >
          🕘 {t("editor.history")}
        </button>
        <button
          class={`preview-toggle ${preview() ? "active" : ""}`}
          onClick={() => setPreview((v) => !v)}
          title={t("editor.toggleTooltip")}
        >
          {preview() ? t("editor.edit") : t("editor.preview")}
        </button>
      </div>
      <div class={`editor-content-area ${preview() ? "split" : ""}`}>
        <Show when={loaded()} fallback={<p class="muted">{t("editor.loading")}</p>}>
          <div
            class="cm-host"
            ref={host}
            onPaste={(e) => {
              const files = e.clipboardData?.files;
              if (files && files.length) {
                e.preventDefault();
                handleFiles(files);
              }
            }}
            onDrop={(e) => {
              e.preventDefault();
              const files = e.dataTransfer?.files;
              if (files) handleFiles(files);
            }}
            onDragOver={(e) => e.preventDefault()}
          />
        </Show>
        <Show when={preview()}>
          <div
            class="markdown-preview"
            innerHTML={previewHtml()}
            onClick={(e) => {
              const target = e.target as HTMLElement;
              if (target.classList.contains("cite-chip")) {
                const path = target.dataset.path;
                if (path) console.log("navigate to", path);
              }
            }}
          />
        </Show>
      </div>
      <RewriteMenu
        view={viewGetter}
        conceptPath={props.path}
        onSaveSnapshot={async (text) => {
          try {
            await invoke("save_snapshot", { conceptPath: props.path, text });
          } catch (e) {
            console.error("save_snapshot failed", e);
          }
        }}
      />
      <Show when={historyOpen()}>
        <HistoryPanel
          conceptPath={props.path}
          view={viewGetter}
          onClose={() => setHistoryOpen(false)}
        />
      </Show>
    </div>
  );
}
