// GLM RAG 集成测试：索引笔记 → 检索 → 验证 top-K 含相关 concept。
// 需要 GLM_API_KEY 环境变量。
use lmnotes_core::index::sqlite::SqliteIndex;
use lmnotes_core::index::tantivy::TantivyIndex;
use lmnotes_core::indexer::Indexer;
use lmnotes_core::llm::guard::GuardConfig;
use lmnotes_core::llm::openai::OpenAiProvider;
use lmnotes_core::llm::routing::{ProviderRef, Registry, Routing, Task};
use lmnotes_core::okf::concept::Concept;
use lmnotes_core::qa::context::build_context;
use lmnotes_core::qa::retriever::Retriever;
use std::sync::Arc;

#[tokio::test]
async fn glm_rag_retrieval() {
    let key = match std::env::var("GLM_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => {
            eprintln!("SKIP: GLM_API_KEY not set");
            return;
        }
    };

    let tmp = tempfile::tempdir().unwrap();
    let sqlite = Arc::new(SqliteIndex::open(tmp.path().join("rag.sqlite")).unwrap());
    sqlite.init_schema_with_vec_dim(1024).await.unwrap();
    let ft = Arc::new(TantivyIndex::open(tmp.path().join("rag.tantivy")).unwrap());
    let indexer = Indexer::new(sqlite.clone(), ft.clone());

    // 索引 3 篇笔记
    let notes = [
        ("nt_rag1", "notes/rag-chunking.md",
         "# RAG Chunking Strategies\n\nThe main chunking strategies for RAG include: fixed-size chunking, sentence-based chunking, and semantic chunking. Semantic chunking groups related sentences together for better retrieval accuracy."),
        ("nt_rag2", "notes/rag-embedding.md",
         "# Embedding Models for RAG\n\nChoosing the right embedding model is critical. Models like text-embedding-3 and nomic-embed-text offer different trade-offs between dimension, speed, and quality."),
        ("nt_rag3", "notes/transformer-attention.md",
         "# Transformer Attention\n\nThe attention mechanism computes weighted sums of value vectors based on query-key similarity. Multi-head attention runs multiple attention functions in parallel."),
    ];
    for (id, path, body) in &notes {
        let text = format!(
            "---\ntype: note\nid: {id}\ntitle: {title}\n---\n\n{body}",
            id = id,
            title = path,
            body = body
        );
        let c = Concept::parse(&text).unwrap();
        indexer.index_concept(path, &text, &c).await.unwrap();
    }

    // 用 generate_suggestions 写入向量（embed）
    let mut reg = Registry::new();
    let glm = Arc::new(OpenAiProvider::new(
        "glm",
        "https://open.bigmodel.cn/api/paas/v4",
        &key,
    ));
    reg.register_chat_arc(glm.clone());
    reg.register_embed_arc(glm);

    let routing = Routing {
        map: [(
            Task::Embed,
            (
                ProviderRef {
                    provider_id: "glm".into(),
                    model: "embedding-2".into(),
                },
                vec![],
            ),
        )]
        .into_iter()
        .collect(),
    };
    let guard = GuardConfig {
        cloud_allowed: true,
        sensitive_patterns: vec![],
    };

    // embed 每篇笔记
    for (id, path, body) in &notes {
        let c = Concept::parse(&format!(
            "---\ntype: note\nid: {id}\n---\n\n{body}",
            id = id,
            body = body
        ))
        .unwrap();
        lmnotes_core::indexer::generate_suggestions(
            &c, path, &sqlite, &reg, &routing, &guard, body,
        )
        .await
        .unwrap();
    }

    // 检索：问"chunking strategy"
    let embedder = reg.embed_for(&routing, Task::Embed).unwrap().0;
    let retriever = Retriever::new(
        sqlite.clone(),
        ft.clone(),
        sqlite.clone(),
        embedder,
        "embedding-2".into(),
    );
    let chunks = retriever
        .retrieve("What are the chunking strategies for RAG?", 3)
        .await
        .unwrap();

    println!("retrieved {} chunks:", chunks.len());
    for c in &chunks {
        println!("  [{}] {} (score={:.4})", c.concept_id, c.path, c.score);
    }

    // 验证：nt_rag1（rag-chunking）应在前 2 名
    assert!(!chunks.is_empty(), "should retrieve at least one chunk");
    assert!(
        chunks.iter().take(2).any(|c| c.concept_id == "nt_rag1"),
        "rag-chunking note should be in top-2 results"
    );

    // 验证：build_context 生成带编号的引用
    let (ctx, cites) = build_context(&chunks, 6000);
    assert!(!cites.is_empty(), "should have citations");
    println!("context preview:\n{}", &ctx[..ctx.len().min(300)]);
}
