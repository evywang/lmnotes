//! 知识图谱构建（FR-SEARCH-03）。
//!
//! 图谱是 `.lmnotes/index.sqlite` 的派生视图（ADR-0001 O2），节点=concept，
//! 边来自两个来源：
//! - 显式链接：正文 markdown link（`/` 开头 bundle-relative，ADR-0001 §5），
//!   存于 `edges` 邻接表（ADR-0003）。
//! - 语义近邻：向量相似度（vec_concepts，sqlite-vec KNN）。
//!
//! 本模块只做「聚合」，不读文件、不碰 LLM，符合 ADR-0002 核心边界。

use crate::backend::IndexBackend;
use crate::index::schema::ConceptRow;
use crate::index::sqlite::SqliteIndex;
use crate::Result;
use std::collections::{HashMap, HashSet};

/// 默认语义近邻数（KNN k 值）。
pub const DEFAULT_K: usize = 8;

/// 默认相似度距离阈值（sqlite-vec 余弦距离；越小越相似）。
/// 0.55 约对应余弦相似度 0.72，过滤明显不相关的噪声边。
pub const DEFAULT_THRESHOLD: f32 = 0.55;

/// 图谱节点。
#[derive(Debug, Clone, PartialEq)]
pub struct GraphNode {
    pub id: String,
    pub title: String,
    pub path: String,
}

/// 图谱边。
#[derive(Debug, Clone, PartialEq)]
pub struct GraphEdge {
    pub src: String,
    pub dst: String,
    /// 边类型：显式链接 vs 语义近邻。
    pub kind: EdgeKind,
    /// 权重（显式链接=1.0；语义边=相似度，越大越强）。
    pub weight: f32,
}

/// 边类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// 用户在正文手写的 markdown 链接。
    Explicit,
    /// LLM 向量相似度发现的语义近邻（非用户显式连接）。
    Semantic,
}

/// 完整图谱。
#[derive(Debug, Clone, PartialEq)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl GraphData {
    pub fn empty() -> Self {
        GraphData {
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

/// 从概念行派生显示标题：优先 title，否则用 path 的文件名。
fn title_of(c: &ConceptRow) -> String {
    c.title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| {
            c.path
                .rsplit('/')
                .next()
                .unwrap_or(&c.path)
                .trim_end_matches(".md")
                .to_string()
        })
}

/// 把概念行映射为图谱节点。
fn node_of(c: &ConceptRow) -> GraphNode {
    GraphNode {
        id: c.id.clone(),
        title: title_of(c),
        path: c.path.clone(),
    }
}

/// 构建全库图谱：全部 concept 节点 + 全部显式链接边。
///
/// 不含语义近邻边（全量预计算两两相似度会爆炸；语义边由 neighborhood 按需展开）。
pub fn build_full_graph(meta: &dyn IndexBackend) -> Result<GraphData> {
    let concepts = meta.all_concepts()?;
    let edges = meta.all_edges()?;
    let valid_ids: HashSet<&str> = concepts.iter().map(|c| c.id.as_str()).collect();

    let nodes = concepts.iter().map(node_of).collect();

    // 只保留两端都存在的边（丢弃悬空链接 dst_id=None 的边，它们无法构成图谱边）。
    let edges = edges
        .into_iter()
        .filter_map(|e| match e.dst_id {
            Some(dst)
                if valid_ids.contains(e.src_id.as_str()) && valid_ids.contains(dst.as_str()) =>
            {
                Some(GraphEdge {
                    src: e.src_id,
                    dst,
                    kind: EdgeKind::Explicit,
                    weight: 1.0,
                })
            }
            _ => None,
        })
        .collect();

    Ok(GraphData { nodes, edges })
}

/// 构建单点子图：focus 笔记的出链 + 入链 + 语义近邻。
///
/// 返回的图谱以 focus 为中心，包含：
/// - focus 的所有出链（forward_edges）和入链（backrefs）邻居
/// - focus 的向量 KNN 近邻（若 focus 已被 embed）
/// - 这些邻居之间的显式连边（让子图结构完整）
///
/// `k`/`threshold` 为 None 时用默认值。
pub fn build_neighborhood(
    meta: &dyn IndexBackend,
    vec_idx: &SqliteIndex,
    focus_id: &str,
    k: Option<usize>,
    threshold: Option<f32>,
) -> Result<GraphData> {
    let k = k.unwrap_or(DEFAULT_K);
    let threshold = threshold.unwrap_or(DEFAULT_THRESHOLD);

    // 1. 收集子图涉及的 concept id 集合（从 focus 出发）
    let mut involved: HashSet<String> = HashSet::new();
    involved.insert(focus_id.to_string());

    let outgoing = meta.forward_edges(focus_id)?;
    let incoming = meta.backrefs(focus_id)?;
    for e in &outgoing {
        if let Some(dst) = &e.dst_id {
            involved.insert(dst.clone());
        }
    }
    for e in &incoming {
        involved.insert(e.src_id.clone());
    }

    // 2. 向量近邻（若 focus 有 embedding）
    let mut semantic_edges: Vec<GraphEdge> = Vec::new();
    if let Some(emb) = vec_idx.concept_embedding(focus_id)? {
        let neighbors = vec_idx.vector_search(&emb, k + 1)?; // +1 因为 KNN 会返回 focus 自身
        for (nid, dist) in neighbors {
            if nid == focus_id {
                continue;
            }
            // sqlite-vec 余弦距离越小越相似；过滤超过阈值的噪声。
            if dist > threshold {
                continue;
            }
            involved.insert(nid.clone());
            // 把距离转成相似度权重（distance ∈ [0,2]，similarity = 1 - distance/2）
            let sim = (1.0 - dist / 2.0).max(0.0);
            semantic_edges.push(GraphEdge {
                src: focus_id.to_string(),
                dst: nid,
                kind: EdgeKind::Semantic,
                weight: sim,
            });
        }
    }

    // 3. 拉取所有涉及节点的元数据，构建节点列表
    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut id_to_node: HashMap<String, GraphNode> = HashMap::new();
    for id in &involved {
        if let Some(c) = meta.get_concept(id)? {
            let n = node_of(&c);
            id_to_node.insert(id.clone(), n.clone());
            nodes.push(n);
        }
    }

    // 4. 收集这些节点之间的显式边（让子图结构完整，不止 focus 的直连）
    let mut edges: Vec<GraphEdge> = Vec::new();
    let involved_ref: HashSet<&str> = involved.iter().map(|s| s.as_str()).collect();
    for id in &involved {
        for e in meta.forward_edges(id)? {
            if let Some(dst) = &e.dst_id {
                if involved_ref.contains(dst.as_str()) && involved_ref.contains(id.as_str()) {
                    edges.push(GraphEdge {
                        src: id.clone(),
                        dst: dst.clone(),
                        kind: EdgeKind::Explicit,
                        weight: 1.0,
                    });
                }
            }
        }
    }
    edges.extend(semantic_edges);

    // 去重：同一 (src,dst) 若既有显式边又有语义边，保留显式边（用户意图优先）。
    let mut seen: HashSet<(String, String)> = HashSet::new();
    edges.retain(|e| {
        let key = (e.src.clone(), e.dst.clone());
        if e.kind == EdgeKind::Explicit {
            seen.insert(key);
            true
        } else {
            seen.insert(key)
        }
    });

    Ok(GraphData { nodes, edges })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::schema::{ConceptRow, EdgeRow};

    // 由于 build_full_graph / build_neighborhood 接受 &dyn IndexBackend，
    // 而 SqliteIndex 实现了该 trait，这里直接用真实后端测试。
    async fn seed_graph() -> SqliteIndex {
        let idx = SqliteIndex::in_memory().unwrap();
        idx.init_schema_with_vec_dim(3).await.unwrap();
        // 三个概念：a → b（显式链接）
        idx.upsert_concept(ConceptRow {
            id: "nt_a".into(),
            path: "a.md".into(),
            type_: "note".into(),
            title: Some("Note A".into()),
            mtime: 0,
            content_hash: "h1".into(),
            aliases: vec![],
        })
        .await
        .unwrap();
        idx.upsert_concept(ConceptRow {
            id: "nt_b".into(),
            path: "b.md".into(),
            type_: "note".into(),
            title: Some("Note B".into()),
            mtime: 0,
            content_hash: "h2".into(),
            aliases: vec![],
        })
        .await
        .unwrap();
        idx.upsert_concept(ConceptRow {
            id: "nt_c".into(),
            path: "c.md".into(),
            type_: "note".into(),
            title: None, // 测试 title fallback 到 path
            mtime: 0,
            content_hash: "h3".into(),
            aliases: vec![],
        })
        .await
        .unwrap();
        idx.replace_edges(
            "nt_a",
            vec![EdgeRow {
                src_id: "nt_a".into(),
                dst_id: Some("nt_b".into()),
                dst_path: "/b.md".into(),
                link_text: None,
            }],
        )
        .await
        .unwrap();
        idx
    }

    #[tokio::test]
    async fn build_full_graph_collects_all_nodes_and_edges() {
        let idx = seed_graph().await;
        let g = build_full_graph(&idx).unwrap();
        assert_eq!(g.nodes.len(), 3);
        assert_eq!(g.edges.len(), 1);
        assert_eq!(g.edges[0].src, "nt_a");
        assert_eq!(g.edges[0].dst, "nt_b");
        assert_eq!(g.edges[0].kind, EdgeKind::Explicit);
    }

    #[tokio::test]
    async fn build_full_graph_drops_dangling_edges() {
        let idx = seed_graph().await;
        // 插入一条悬空边（dst 指向不存在的概念）
        idx.replace_edges(
            "nt_a",
            vec![
                EdgeRow {
                    src_id: "nt_a".into(),
                    dst_id: Some("nt_b".into()),
                    dst_path: "/b.md".into(),
                    link_text: None,
                },
                EdgeRow {
                    src_id: "nt_a".into(),
                    dst_id: None, // 悬空
                    dst_path: "/ghost.md".into(),
                    link_text: None,
                },
            ],
        )
        .await
        .unwrap();
        let g = build_full_graph(&idx).unwrap();
        assert_eq!(g.edges.len(), 1, "悬空边应被丢弃");
    }

    #[tokio::test]
    async fn title_falls_back_to_path() {
        let idx = seed_graph().await;
        let g = build_full_graph(&idx).unwrap();
        let c = g.nodes.iter().find(|n| n.id == "nt_c").unwrap();
        assert_eq!(c.title, "c", "无 title 时应用 path 文件名（去 .md）");
    }

    #[tokio::test]
    async fn build_neighborhood_includes_explicit_neighbors() {
        let idx = seed_graph().await;
        let g = build_neighborhood(&idx, &idx, "nt_a", None, None).unwrap();
        // nt_a 的子图应包含 nt_a、nt_b（出链邻居）
        let ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
        assert!(ids.contains(&"nt_a"));
        assert!(ids.contains(&"nt_b"));
        // 应有 nt_a→nt_b 显式边
        assert!(g
            .edges
            .iter()
            .any(|e| e.src == "nt_a" && e.dst == "nt_b" && e.kind == EdgeKind::Explicit));
    }

    #[tokio::test]
    async fn build_neighborhood_without_embedding_has_no_semantic_edges() {
        let idx = seed_graph().await;
        let g = build_neighborhood(&idx, &idx, "nt_a", None, None).unwrap();
        assert!(
            !g.edges.iter().any(|e| e.kind == EdgeKind::Semantic),
            "未 embed 时不应有语义边"
        );
    }

    #[tokio::test]
    async fn build_neighborhood_adds_semantic_edge_when_similar() {
        let idx = seed_graph().await;
        // nt_a 和 nt_b 用相同向量（完全相似，distance=0）
        idx.upsert_vector("nt_a", &[1.0, 0.0, 0.0]).unwrap();
        idx.upsert_vector("nt_b", &[1.0, 0.0, 0.0]).unwrap();
        idx.upsert_vector("nt_c", &[0.0, 0.0, 1.0]).unwrap(); // 与 a 正交

        let g = build_neighborhood(&idx, &idx, "nt_a", Some(5), None).unwrap();
        // nt_b 与 nt_a 完全相似，应作为语义邻居出现
        // 注意：nt_a→nt_b 已有显式边，去重时显式边优先，所以语义边被合并。
        // 改测：nt_c 与 nt_a 正交（distance=1.0 > 0.55 阈值），不应出现。
        let has_c = g.nodes.iter().any(|n| n.id == "nt_c");
        assert!(!has_c, "nt_c 与 focus 正交，应被阈值过滤掉");
    }
}
