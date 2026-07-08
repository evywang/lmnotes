import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

// ============ Types (mirror Rust DTOs) ============

export interface GraphNodeDto {
  id: string;
  title: string;
  path: string;
}

export interface GraphEdgeDto {
  src: string;
  dst: string;
  /** "explicit"（用户手写链接）| "semantic"（向量近邻） */
  kind: "explicit" | "semantic";
  /** 显式=1.0；语义=相似度 */
  weight: number;
}

export interface GraphDto {
  nodes: GraphNodeDto[];
  edges: GraphEdgeDto[];
}

// ============ Module-level signals ============

const [graphData, setGraphData] = createSignal<GraphDto | null>(null);
const [graphLoading, setGraphLoading] = createSignal(false);
const [graphMode, setGraphMode] = createSignal<"drawer" | "full">("drawer");

export function useGraph() {
  return { graphData, setGraphData, graphLoading, graphMode, setGraphMode };
}

/** 加载全库图谱（全部节点 + 显式链接边）。 */
export async function loadFullGraph() {
  setGraphLoading(true);
  try {
    const r = await invoke<GraphDto>("graph_full");
    setGraphData(r);
    setGraphMode("full");
  } catch (e) {
    console.error("load full graph", e);
    setGraphData({ nodes: [], edges: [] });
  } finally {
    setGraphLoading(false);
  }
}

/**
 * 加载单点子图（focus 笔记的出链 + 入链 + 语义近邻）。
 * 用笔记 path 反查 concept id（前端只持有 path）。
 */
export async function loadNeighborhood(conceptId: string, k?: number, threshold?: number) {
  setGraphLoading(true);
  try {
    const r = await invoke<GraphDto>("graph_neighborhood", {
      conceptId,
      k: k ?? null,
      threshold: threshold ?? null,
    });
    setGraphData(r);
    setGraphMode("drawer");
  } catch (e) {
    console.error("load neighborhood", e);
    setGraphData({ nodes: [], edges: [] });
  } finally {
    setGraphLoading(false);
  }
}
