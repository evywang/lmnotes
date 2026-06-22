import { createSignal, onMount, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { useCodeMirror } from "./solid-cm";

interface ConceptFile {
  text: string;
}

export function Editor(props: { path: string }) {
  let host: HTMLDivElement | undefined;
  const [content, setContent] = createSignal("");
  const [loaded, setLoaded] = createSignal(false);
  const [dirty, setDirty] = createSignal(false);
  let saveTimer: ReturnType<typeof setTimeout> | null = null;
  let viewGetter = () => undefined as ReturnType<ReturnType<typeof useCodeMirror>>;

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

  return (
    <div class="editor-wrap">
      <div class="editor-toolbar">
        <span class="editor-path">{props.path}</span>
        <Show when={dirty()}>
          <span class="dirty-dot">●</span>
        </Show>
      </div>
      <Show when={loaded()} fallback={<p class="muted">加载中…</p>}>
        <div class="cm-host" ref={host} />
      </Show>
    </div>
  );
}
