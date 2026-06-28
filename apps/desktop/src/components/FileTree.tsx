import { createSignal, For, Show, onMount, onCleanup, createMemo } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

interface FileTreeNode {
  name: string;
  path: string;
  is_dir: boolean;
  children: FileTreeNode[];
}

// 右键上下文菜单状态
const [ctxMenu, setCtxMenu] = createSignal<{
  x: number;
  y: number;
  node: FileTreeNode;
} | null>(null);

// 移动对话框状态
const [moveDialog, setMoveDialog] = createSignal<{
  srcPath: string;
  srcName: string;
  dirs: { path: string; name: string }[];
} | null>(null);

// 全局拖拽状态
const [dragSrc, setDragSrc] = createSignal<string | null>(null);
const [dragOverPath, setDragOverPath] = createSignal<string | null>(null);

// 用 ref 回调绑定原生拖拽（queueMicrotask 确保 DOM 挂载）
function bindDrag(el: HTMLElement, node: FileTreeNode) {
  el.setAttribute("data-node-path", node.path);
  el.setAttribute("data-is-dir", node.is_dir ? "1" : "0");

  el.addEventListener("mousedown", (e: MouseEvent) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    console.log("[drag] mousedown on", node.path);

    const startX = e.clientX;
    const startY = e.clientY;
    let started = false;

    const onMove = (ev: MouseEvent) => {
      if (!started) {
        if (Math.abs(ev.clientX - startX) > 4 || Math.abs(ev.clientY - startY) > 4) {
          started = true;
          setDragSrc(node.path);
          document.body.style.userSelect = "none";
          console.log("[drag] started, src=", node.path);
        }
      }
      if (started) {
        ev.preventDefault();
        const tgt = document.elementFromPoint(ev.clientX, ev.clientY);
        const dirEl = tgt?.closest("[data-is-dir='1']") as HTMLElement | null;
        if (dirEl) {
          const dp = dirEl.getAttribute("data-node-path")!;
          setDragOverPath(dp !== node.path ? dp : null);
        } else {
          setDragOverPath(null);
        }
      }
    };

    const onUp = (ev: MouseEvent) => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.userSelect = "";
      if (started) {
        const tgt = document.elementFromPoint(ev.clientX, ev.clientY);
        const dirEl = tgt?.closest("[data-is-dir='1']") as HTMLElement | null;
        if (dirEl) {
          const dp = dirEl.getAttribute("data-node-path")!;
          if (dp !== node.path) {
            console.log("[drag] drop", node.path, "->", dp);
            window.dispatchEvent(new CustomEvent("lmnotes-treedrop", { detail: { src: node.path, dest: dp } }));
          }
        }
        setDragSrc(null);
        setDragOverPath(null);
      }
    };

    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  });
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
      // 默认展开前 2 层
      const first = new Set<string>();
      const expandDepth = (nodes: FileTreeNode[], depth: number) => {
        for (const n of nodes) {
          if (n.is_dir && depth < 2) {
            first.add(n.path);
            expandDepth(n.children, depth + 1);
          }
        }
      };
      expandDepth(nodes, 0);
      setExpanded(first);
    } catch (e) {
      console.error("list_tree", e);
    }
  };

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

  const createNoteInDir = async (dir: string) => {
    const title = window.prompt("笔记标题：", "新笔记");
    if (!title) return;
    try {
      const path = await invoke<string>("create_note", { title, parentDir: dir });
      props.onOpen(path);
      await loadTree();
    } catch (e) {
      alert("创建失败: " + e);
    }
  };

  const createFolderInDir = async (dir: string) => {
    const name = window.prompt("文件夹名称：", "new-folder");
    if (!name) return;
    try {
      await invoke("create_folder", { parentDir: dir, name });
      await loadTree();
    } catch (e) {
      alert("创建失败: " + e);
    }
  };

  const revealInExplorer = async (path: string) => {
    try {
      await invoke("reveal_in_explorer", { relPath: path });
    } catch (e) {
      alert("打开失败: " + e);
    }
  };

  const doMove = async (srcPath: string, destDir: string) => {
    try {
      await invoke("move_item", { srcPath, destDir });
      await loadTree();
    } catch (e) {
      alert("移动失败: " + e);
    }
  };

  const moveToDialog = (srcPath: string, srcName: string) => {
    const dirs = collectDirs(tree()).map((d) => ({ path: d.path, name: d.name }));
    if (dirs.length === 0) {
      alert("没有可移动到的目录");
      return;
    }
    setMoveDialog({ srcPath, srcName, dirs });
  };

  // 监听全局 treedrop 事件
  onMount(() => {
    const handler = (e: Event) => {
      const { src, dest } = (e as CustomEvent).detail;
      doMove(src, dest);
    };
    window.addEventListener("lmnotes-treedrop", handler);
    onCleanup(() => window.removeEventListener("lmnotes-treedrop", handler));
  });

  const collectDirs = (nodes: FileTreeNode[]): FileTreeNode[] => {
    let result: FileTreeNode[] = [];
    for (const n of nodes) {
      if (n.is_dir) {
        result.push(n);
        result = result.concat(collectDirs(n.children));
      }
    }
    return result;
  };

  // TreeNode 是真正的 SolidJS 组件（首字母大写），其内部的 expanded() 读取是响应式的
  function TreeNode(props: {
    node: FileTreeNode;
    depth: number;
    expanded: () => Set<string>;
    activePath: () => string | null;
    onToggle: (path: string) => void;
    onOpen: (path: string) => void;
    onDelete: (path: string, name: string) => void;
    onCreateNote: (dir: string) => void;
    onCreateFolder: (dir: string) => void;
    onReveal: (path: string) => void;
    onMoveDialog: (path: string, name: string) => void;
    onMove: (src: string, dest: string) => void;
  }) {
    const node = props.node;
    const isOpen = () => props.expanded().has(node.path);
    const isActive = () => props.activePath() === node.path;
    const isDropTarget = () => node.is_dir && dragOverPath() === node.path;

    return (
      <div class="tree-node">
        <div
          ref={(el) => queueMicrotask(() => bindDrag(el, node))}
          class={`tree-row ${isActive() ? "tree-row-active" : ""} ${isDropTarget() ? "tree-row-drop" : ""}`}
          style={{ "padding-left": `${props.depth * 14 + 4}px` }}
          attr:data-node-path={node.path}
          attr:data-is-dir={node.is_dir ? "1" : "0"}
          onClick={() => (node.is_dir ? props.onToggle(node.path) : props.onOpen(node.path))}
          onContextMenu={(e) => {
            e.preventDefault();
            setCtxMenu({ x: e.clientX, y: e.clientY, node });
          }}
        >
          <span class="tree-icon">{node.is_dir ? (isOpen() ? "📂" : "📁") : "📄"}</span>
          <span class="tree-name">{node.name}</span>
          <Show when={node.is_dir}>
            <button
              class="tree-action"
              title="新建笔记"
              onClick={(e) => { e.stopPropagation(); props.onCreateNote(node.path); }}
            >
              ＋
            </button>
            <button
              class="tree-action"
              title="新建文件夹"
              onClick={(e) => { e.stopPropagation(); props.onCreateFolder(node.path); }}
            >
              📁＋
            </button>
          </Show>
          <Show when={!node.is_dir}>
            <button
              class="tree-delete"
              title="删除"
              onClick={(e) => {
                e.stopPropagation();
                props.onDelete(node.path, node.name);
              }}
            >
              🗑
            </button>
          </Show>
        </div>
        <Show when={node.is_dir && isOpen()}>
          <For each={node.children}>
            {(child) => (
              <TreeNode
                node={child}
                depth={props.depth + 1}
                expanded={props.expanded}
                activePath={props.activePath}
                onToggle={props.onToggle}
                onOpen={props.onOpen}
                onDelete={props.onDelete}
                onCreateNote={props.onCreateNote}
                onCreateFolder={props.onCreateFolder}
                onReveal={props.onReveal}
                onMoveDialog={props.onMoveDialog}
                onMove={props.onMove}
              />
            )}
          </For>
        </Show>
      </div>
    );
  }

  return (
    <div class="file-tree">
      <Show when={tree().length === 0}>
        <p class="muted small" style={{ padding: "0.5rem" }}>
          暂无笔记
        </p>
      </Show>
      <For each={tree()}>
        {(node) => (
          <TreeNode
            node={node}
            depth={0}
            expanded={expanded}
            activePath={props.activePath}
            onToggle={toggle}
            onOpen={props.onOpen}
            onDelete={deleteFile}
            onCreateNote={createNoteInDir}
            onCreateFolder={createFolderInDir}
            onReveal={revealInExplorer}
            onMoveDialog={moveToDialog}
            onMove={doMove}
          />
        )}
      </For>

      {/* 右键上下文菜单 */}
      <Show when={ctxMenu()}>
        {(menu) => (
          <>
            <div
              class="ctx-menu-overlay"
              onClick={() => setCtxMenu(null)}
              onContextMenu={(e) => { e.preventDefault(); setCtxMenu(null); }}
            />
            <div
              class="ctx-menu"
              style={{ left: `${menu().x}px`, top: `${menu().y}px` }}
            >
              <Show when={menu().node.is_dir}>
                <button class="ctx-item" onClick={() => { createNoteInDir(menu().node.path); setCtxMenu(null); }}>
                  📄 新建笔记
                </button>
                <button class="ctx-item" onClick={() => { createFolderInDir(menu().node.path); setCtxMenu(null); }}>
                  📁 新建文件夹
                </button>
                <div class="ctx-sep" />
              </Show>
              <Show when={!menu().node.is_dir}>
                <button class="ctx-item" onClick={() => { props.onOpen(menu().node.path); setCtxMenu(null); }}>
                  📄 打开
                </button>
                <div class="ctx-sep" />
                <button class="ctx-item" onClick={() => { deleteFile(menu().node.path, menu().node.name); setCtxMenu(null); }}>
                  🗑 删除
                </button>
                <div class="ctx-sep" />
              </Show>
              <button class="ctx-item" onClick={() => { moveToDialog(menu().node.path, menu().node.name); setCtxMenu(null); }}>
                ✂️ 移动到…
              </button>
              <button class="ctx-item" onClick={() => { revealInExplorer(menu().node.path); setCtxMenu(null); }}>
                🖥 在文件管理器中打开
              </button>
            </div>
          </>
        )}
      </Show>

      {/* 移动到对话框 */}
      <Show when={moveDialog()}>
        {(dlg) => (
          <div class="move-overlay" onClick={() => setMoveDialog(null)}>
            <div class="move-dialog" onClick={(e) => e.stopPropagation()}>
              <h3>移动 "{dlg().srcName}" 到</h3>
              <div class="move-list">
                <For each={dlg().dirs}>
                  {(dir) => (
                    <button
                      class="move-dir-item"
                      onClick={async () => {
                        await doMove(dlg().srcPath, dir.path);
                        setMoveDialog(null);
                      }}
                    >
                      📁 {dir.path}
                    </button>
                  )}
                </For>
              </div>
              <button class="btn-secondary" onClick={() => setMoveDialog(null)}>
                取消
              </button>
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
