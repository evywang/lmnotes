import { onMount, onCleanup, Show, createEffect } from "solid-js";
import cytoscape, { type Core, type ElementDefinition } from "cytoscape";
import { t } from "../i18n";
import { useVault } from "../store/vault";
import {
  useGraph,
  loadFullGraph,
  loadNeighborhood,
  type GraphDto,
} from "./store/graph";

/**
 * 知识图谱组件（FR-SEARCH-03）。
 *
 * 两种模式：
 * - "drawer"：聚焦当前笔记的子图（出链 + 入链 + 语义近邻），侧边抽屉。
 * - "full"：全库图谱（全部节点 + 显式链接边），全屏。
 *
 * 渲染用 Cytoscape.js（ADR-0004）；显式边实线，语义边虚线。
 */
export function KnowledgeGraph(props: {
  mode: "drawer" | "full";
  onClose: () => void;
  onNavigate: (path: string) => void;
}) {
  const { graphData, graphLoading } = useGraph();
  const { activePath } = useVault();
  let containerRef: HTMLDivElement | undefined;
  let cy: Core | undefined;

  // 把 GraphDto 转成 Cytoscape elements。
  const toElements = (data: GraphDto): ElementDefinition[] => {
    const active = activePath();
    const nodes: ElementDefinition[] = data.nodes.map((n) => ({
      data: {
        id: n.id,
        label: n.title,
        path: n.path,
        // 当前打开的笔记高亮。
        active: n.path === active,
      },
    }));
    const edges: ElementDefinition[] = data.edges.map((e, i) => ({
      data: {
        // Cytoscape 要求每条边有唯一 id。
        id: `e${i}`,
        source: e.src,
        target: e.dst,
        kind: e.kind,
        weight: e.weight,
      },
    }));
    return [...nodes, ...edges];
  };

  // 初始化 / 更新 Cytoscape。
  const render = () => {
    const data = graphData();
    if (!containerRef || !data) return;

    const elements = toElements(data);
    const nodeCount = data.nodes.length;

    if (cy) {
      // 已存在实例：替换 elements 并重跑布局。
      cy.elements().remove();
      cy.add(elements);
      cy.layout(layoutOptions(props.mode, nodeCount)).run();
    } else {
      cy = cytoscape({
        container: containerRef,
        elements,
        style: cytoscapeStyle(),
        layout: layoutOptions(props.mode, nodeCount),
        wheelSensitivity: 0.2,
        minZoom: 0.2,
        maxZoom: 3,
      });
      // 节点点击：跳转笔记。
      cy.on("tap", "node", (evt) => {
        const path = evt.target.data("path");
        if (path) {
          props.onNavigate(path);
        }
      });
    }
  };

  onMount(() => {
    // 按模式加载初始数据。
    if (props.mode === "drawer") {
      // drawer 模式：聚焦当前笔记。需 concept id；用 activePath 反查。
      // 这里先加载全库，再前端筛选当前节点邻域过于复杂，
      // 直接走 neighborhood（conceptId 用 activePath 临时充当——后端会回退到 path 匹配）。
      const active = activePath();
      if (active) {
        loadNeighborhood(active);
      } else {
        loadFullGraph();
      }
    } else {
      loadFullGraph();
    }
  });

  // 数据变化时重渲染。
  createEffect(() => {
    if (graphData()) render();
  });

  onCleanup(() => {
    cy?.destroy();
    cy = undefined;
  });

  return (
    <div class={`graph-drawer graph-drawer-${props.mode}`}>
      <div class="graph-header">
        <h3>
          {props.mode === "full"
            ? t("graph.titleFull")
            : t("graph.titleDrawer")}
        </h3>
        <div class="graph-header-actions">
          <Show when={props.mode === "drawer"}>
            <button
              class="graph-action-btn"
              title={t("graph.fullViewTooltip")}
              onClick={() => loadFullGraph()}
            >
              {t("graph.fullView")}
            </button>
          </Show>
          <button
            class="graph-action-btn"
            title={t("graph.relayoutTooltip")}
            onClick={() => {
              const n = graphData()?.nodes.length ?? 0;
              cy?.layout(layoutOptions(props.mode, n)).run();
            }}
          >
            {t("graph.relayout")}
          </button>
          <button class="graph-close" onClick={props.onClose}>
            ✕
          </button>
        </div>
      </div>
      <div class="graph-body">
        <Show when={graphLoading()}>
          <div class="graph-loading">{t("graph.loading")}</div>
        </Show>
        <Show when={!graphLoading() && (graphData()?.nodes.length ?? 0) === 0}>
          <div class="graph-empty">{t("graph.empty")}</div>
        </Show>
        <div class="graph-canvas" ref={containerRef} />
      </div>
      <div class="graph-legend">
        <span class="graph-legend-item">
          <span class="graph-legend-line graph-legend-explicit" />
          {t("graph.explicitEdge")}
        </span>
        <span class="graph-legend-item">
          <span class="graph-legend-line graph-legend-semantic" />
          {t("graph.semanticEdge")}
        </span>
        <span class="graph-legend-count">
          {(graphData()?.nodes.length ?? 0)} {t("graph.nodes")} ·{" "}
          {(graphData()?.edges.length ?? 0)} {t("graph.edges")}
        </span>
      </div>
    </div>
  );
}

// ============ Cytoscape 布局与样式 ============

function layoutOptions(mode: "drawer" | "full", nodeCount: number) {
  if (mode === "drawer") {
    // 子图：concentric（focus 居中，邻居环绕）。
    return {
      name: "concentric",
      concentric: (n: { degree: boolean }) => n.degree ? 1 : 0,
      minNodeSpacing: 40,
      animate: true,
      animationDuration: 400,
    } as const;
  }
  // 全库：cose（力导向）。>500 节点关闭动画避免卡顿。
  return {
    name: "cose",
    animate: nodeCount <= 500,
    animationDuration: 600,
    nodeRepulsion: () => 8000,
    idealEdgeLength: () => 100,
    nodeOverlap: 20,
    randomize: true,
  } as const;
}

function cytoscapeStyle() {
  return [
    {
      selector: "node",
      style: {
        label: "data(label)",
        "text-valign": "center" as const,
        "text-halign": "center" as const,
        "text-wrap": "wrap" as const,
        "text-max-width": "80px",
        "font-size": "10px",
        color: "var(--fg)",
        "background-color": "var(--accent)",
        width: "label",
        height: "label",
        "padding": "12px",
        "padding-relative-to": "width" as const,
        shape: "round-rectangle" as const,
        "border-width": 0,
        "text-outline-width": 0,
      },
    },
    {
      selector: "node[active]",
      style: {
        "border-width": 3,
        "border-color": "#f9e2af" /* warning yellow */,
        "font-weight": "bold" as const,
      },
    },
    {
      selector: "edge",
      style: {
        width: 2,
        "line-color": "var(--border)",
        "target-arrow-color": "var(--border)",
        "target-arrow-shape": "triangle" as const,
        "curve-style": "bezier" as const,
      },
    },
    {
      selector: 'edge[kind="semantic"]',
      style: {
        "line-style": "dashed" as const,
        "line-color": "#6c7086" /* muted */,
        "target-arrow-color": "#6c7086",
        width: 1,
      },
    },
    {
      selector: "node:active",
      style: {
        "overlay-opacity": 0,
      },
    },
  ];
}
