import { createSignal, For, Show, onMount, createMemo } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

interface FileTreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children: FileTreeNode[];
}

export function FileTree(props: {
  onOpen: (path: string) => void;
  activePath: () => string | null;
  refreshKey: () => number;
}) {
  const [tree, setTree] = createSignal<FileTreeNode[]>([]);
  const [expanded, setExpanded] = createSignal<Set<string>>(new Set());

  const loadTree = async () => {
    try {
      const nodes = await invoke<FileTreeNode[]>("list_tree", { relPath: null });
      setTree(nodes);
      // 默认展开第一层
      const first = new Set<string>();
      nodes.filter((n) => n.is_dir).forEach((n) => first.add(n.path));
      setExpanded(first);
    } catch (e) {
      console.error("list_tree", e);
    }
  };

  // 刷新树（外部操作后）
  createMemo(() => {
    props.refreshKey();
    loadTree();
  });

  onMount(() => loadTree());

  const toggle = (path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const deleteFile = async (path: string, name: string) => {
    if (!confirm(`确定删除 "${name}"？此操作不可撤销。`)) return;
    try {
      await invoke("delete_note", { path });
      await loadTree();
    } catch (e) {
      alert("删除失败: " + e);
    }
  };

  const renderNode = (node: FileTreeNode, depth: number) => {
    const isOpen = expanded().has(node.path);
    const isActive = props.activePath() === node.path;

    return (
      <div class="tree-node">
        <div
          class={`tree-row ${isActive ? "tree-row-active" : ""}`}
          style={{ "padding-left": `${depth * 14 + 4}px` }}
          onClick={() => (node.is_dir ? toggle(node.path) : props.onOpen(node.path))}
        >
          <span class="tree-icon">{node.is_dir ? (isOpen ? "📂" : "📁") : "📄"}</span>
          <span class="tree-name">{node.name}</span>
          <Show when={!node.is_dir}>
            <button
              class="tree-delete"
              title="删除"
              onClick={(e) => {
                e.stopPropagation();
                deleteFile(node.path, node.name);
              }}
            >
              🗑
            </button>
          </Show>
        </div>
        <Show when={node.is_dir && isOpen}>
          <For each={node.children}>
            {(child) => renderNode(child, depth + 1)}
          </For>
        </Show>
      </div>
    );
  };

  return (
    <div class="file-tree">
      <Show when={tree().length === 0}>
        <p class="muted small" style={{ padding: "0.5rem" }}>
          暂无笔记
        </p>
      </Show>
      <For each={tree()}>{(node) => renderNode(node, 0)}</For>
    </div>
  );
}
