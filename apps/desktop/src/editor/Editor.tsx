import { createSignal, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { useCodeMirror } from "./solid-cm";
import { RewriteMenu } from "./RewriteMenu";
import type { EditorView } from "@codemirror/view";

interface ConceptFile {
  text: string;
}

export function Editor(props: { path: string }) {
  let host: HTMLDivElement | undefined;
  const [content, setContent] = createSignal("");
  const [loaded, setLoaded] = createSignal(false);
  const [dirty, setDirty] = createSignal(false);
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

  // 处理粘贴/拖拽图片：哈希去重存盘 → 在光标处插入 markdown 图片链接
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
      </div>
      <Show when={loaded()} fallback={<p class="muted">加载中…</p>}>
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
