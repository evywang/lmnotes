// 真实 GLM 集成测试：验证 generate_suggestions 的 happy path。
// 需要 ~/.lmnotes/config.json 配置有效的 GLM API key。
// 跳过条件：config 中 api_key 为占位符时不跑。
use lmnotes_core::index::SqliteIndex;
use lmnotes_core::indexer::generate_suggestions;
use lmnotes_core::llm::guard::GuardConfig;
use lmnotes_core::llm::openai::OpenAiProvider;
use lmnotes_core::llm::routing::{ProviderRef, Registry, Routing, Task};
use lmnotes_core::llm::suggestion::Suggestion;
use lmnotes_core::okf::concept::Concept;
use std::sync::Arc;

fn glm_key() -> Option<String> {
    // 从环境变量或硬编码（测试用）取 key
    let key = std::env::var("GLM_API_KEY").unwrap_or_default();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

#[tokio::test]
async fn glm_generate_suggestions_happy_path() {
    let key = match glm_key() {
        Some(k) => k,
        None => {
            eprintln!("SKIP: GLM_API_KEY not set");
            return;
        }
    };

    let tmp = tempfile::tempdir().unwrap();
    let sqlite = Arc::new(SqliteIndex::open(tmp.path().join("test.sqlite")).unwrap());
    sqlite.init_schema_with_vec_dim(1024).await.unwrap();

    // 用真实 GLM Provider
    let mut reg = Registry::new();
    let glm = Arc::new(OpenAiProvider::new(
        "glm",
        "https://open.bigmodel.cn/api/paas/v4",
        &key,
    ));
    reg.register_chat_arc(glm.clone());
    reg.register_embed_arc(glm);

    let routing = Routing {
        map: [
            (
                Task::Summarize,
                (
                    ProviderRef {
                        provider_id: "glm".into(),
                        model: "glm-4-flash".into(),
                    },
                    vec![],
                ),
            ),
            (
                Task::LinkSuggest,
                (
                    ProviderRef {
                        provider_id: "glm".into(),
                        model: "glm-4-flash".into(),
                    },
                    vec![],
                ),
            ),
            (
                Task::Embed,
                (
                    ProviderRef {
                        provider_id: "glm".into(),
                        model: "embedding-2".into(),
                    },
                    vec![],
                ),
            ),
        ]
        .into_iter()
        .collect(),
    };
    let guard = GuardConfig {
        cloud_allowed: true,
        sensitive_patterns: vec![],
    };

    let concept = Concept::parse(
        "---\ntype: note\nid: nt_glm\n---\n\n# Attention Mechanism\n\nThe attention mechanism is the core innovation of the Transformer architecture. It allows the model to focus on relevant parts of the input when producing each part of the output.\n",
    )
    .unwrap();
    let text = "# Attention Mechanism\n\nThe attention mechanism is the core innovation of the Transformer architecture.";

    generate_suggestions(
        &concept,
        "notes/attention.md",
        &sqlite,
        &reg,
        &routing,
        &guard,
        text,
    )
    .await
    .unwrap();

    // 验证：建议 store 应有 pending 摘要（至少 1 条）
    let pending = sqlite.list_pending_suggestions().unwrap();
    println!("pending suggestions: {}", pending.len());
    for p in &pending {
        println!(
            "  - kind={}, concept={}",
            p.suggestion.kind_str(),
            p.concept_id
        );
    }
    assert!(
        !pending.is_empty(),
        "should have at least one suggestion (summary)"
    );
    assert!(
        pending
            .iter()
            .any(|s| matches!(&s.suggestion, Suggestion::Summary { .. })),
        "should have a Summary suggestion"
    );

    // 验证：vec_concepts 应有 nt_glm 的向量（1024 维）
    let q = vec![0.0f32; 1024];
    let neighbors = sqlite.vector_search(&q, 5).unwrap();
    println!("vector neighbors: {}", neighbors.len());
    assert!(
        neighbors.iter().any(|(id, _)| id == "nt_glm"),
        "concept nt_glm should be in vector index"
    );
}
