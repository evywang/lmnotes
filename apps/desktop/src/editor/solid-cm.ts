import { onCleanup, onMount } from "solid-js";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown, markdownLanguage } from "@codemirror/lang-markdown";

/**
 * SolidJS 封装 CodeMirror 6。
 * 挂载时以 initial 文本初始化；doc 变更回调 onChange。
 * 返回 view 引用 getter，供外部 dispatch（如插入图片链接）。
 */
export function useCodeMirror(
  host: () => HTMLElement | undefined,
  initial: string,
  onChange: (doc: string) => void,
) {
  let view: EditorView | undefined;

  onMount(() => {
    const el = host();
    if (!el) return;
    view = new EditorView({
      state: EditorState.create({
        doc: initial,
        extensions: [
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          EditorView.lineWrapping,
          markdown({ base: markdownLanguage }),
          EditorView.theme({
            "&": { height: "100%", fontSize: "14px" },
            ".cm-scroller": { overflow: "auto" },
            ".cm-content": { padding: "0.5rem" },
            "&.cm-focused": { outline: "none" },
          }),
          EditorView.updateListener.of((u) => {
            if (u.docChanged) onChange(u.state.doc.toString());
          }),
        ],
      }),
      parent: el,
    });
  });

  onCleanup(() => view?.destroy());

  return () => view;
}
