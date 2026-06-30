import { createSignal, createMemo, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { marked } from "marked";
import { useCodeMirror } from "./solid-cm";
import { RewriteMenu } from "./RewriteMenu";
import type { EditorView } from "@codemirror/view";
import { t } from "../i18n";

interface ConceptFile {
  text: string;
}

export function Editor(props: { path: string }) {
  let host: HTMLDivElement | undefined;
  const [content, setContent] = createSignal("");
  const [loaded, setLoaded] = createSignal(false);
  const [dirty, setDirty] = createSignal(false);
  const [preview, setPreview] = createSignal(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let viewGetter = () => undefined as EditorView | undefined;

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
      if (!f.type.startsWith("image/")) continue;
      const buf = new Uint8Array(await f.arrayBuffer());
      const ext = f.name.split(".").pop() || "png";
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
    </div>
  );
}
