import { createSignal, For, Show, onMount, createMemo } from "solid-js";
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

  const [moveTarget, setMoveTarget] = createSignal<string | null>(null);
  const [dragOver, setDragOver] = createSignal<string | null>(null);

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
    onDragStart: (path: string) => void;
    onDragEnd: () => void;
    onDrop: (destDir: string) => void;
    dragOver: () => string | null;
    setDragOver: (path: string | null) => void;
    moveTarget: () => string | null;
  }) {
    const node = props.node;
    const isOpen = () => props.expanded().has(node.path);
    const isActive = () => props.activePath() === node.path;
    const isDropTarget = () => node.is_dir && props.dragOver() === node.path;

    return (
      <div class="tree-node">
        <div
          class={`tree-row ${isActive() ? "tree-row-active" : ""} ${isDropTarget() ? "tree-row-drop" : ""}`}
          style={{ "padding-left": `${props.depth * 14 + 4}px` }}
          draggable={true}
          onDragStart={(e) => {
            e.dataTransfer?.setData("text/plain", node.path);
            props.onDragStart(node.path);
          }}
          onDragEnd={() => props.onDragEnd()}
          onDragOver={(e) => {
            if (node.is_dir) {
              e.preventDefault();
              props.setDragOver(node.path);
            }
          }}
          onDragLeave={() => {
            if (props.dragOver() === node.path) props.setDragOver(null);
          }}
          onDrop={(e) => {
            e.preventDefault();
            if (node.is_dir) {
              props.onDrop(node.path);
              props.setDragOver(null);
            }
          }}
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
                onDragStart={props.onDragStart}
                onDragEnd={props.onDragEnd}
                onDrop={props.onDrop}
                dragOver={props.dragOver}
                setDragOver={props.setDragOver}
                moveTarget={props.moveTarget}
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
            onDragStart={setMoveTarget}
            onDragEnd={() => { setMoveTarget(null); setDragOver(null); }}
            onDrop={(destDir) => {
              if (moveTarget()) doMove(moveTarget()!, destDir);
              setMoveTarget(null);
            }}
            dragOver={dragOver}
            setDragOver={setDragOver}
            moveTarget={moveTarget}
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
