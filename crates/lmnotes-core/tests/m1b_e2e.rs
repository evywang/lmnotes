// 集成测试：验证 generate_suggestions 完整链路对不可达 LLM 的容错。
use lmnotes_core::backend::IndexBackend;
use lmnotes_core::index::SqliteIndex;
use lmnotes_core::indexer::generate_suggestions;
use lmnotes_core::llm::guard::GuardConfig;
use lmnotes_core::llm::ollama::OllamaProvider;
use lmnotes_core::llm::routing::{ProviderRef, Registry, Routing, Task};
use lmnotes_core::okf::concept::Concept;
use std::sync::Arc;

#[tokio::test]
async fn end_to_end_pipeline_handles_unreachable_llm() {
    let tmp = tempfile::tempdir().unwrap();
    let sqlite = Arc::new(SqliteIndex::open(tmp.path().join("test.sqlite")).unwrap());
    sqlite.init_schema().await.unwrap();

    let mut reg = Registry::new();
    let ollama = Arc::new(OllamaProvider::default_local());
    reg.register_chat_arc(ollama.clone());
    reg.register_embed_arc(ollama);

    let routing = Routing {
        map: [
            (
                Task::Summarize,
                (
                    ProviderRef {
                        provider_id: "ollama".into(),
                        model: "qwen2.5:7b".into(),
                    },
                    vec![],
                ),
            ),
            (
                Task::LinkSuggest,
                (
                    ProviderRef {
                        provider_id: "ollama".into(),
                        model: "qwen2.5:7b".into(),
                    },
                    vec![],
                ),
            ),
            (
                Task::Embed,
                (
                    ProviderRef {
                        provider_id: "ollama".into(),
                        model: "nomic-embed-text".into(),
                    },
                    vec![],
                ),
            ),
        ]
        .into_iter()
        .collect(),
    };
    let guard = GuardConfig::default();

    let concept =
        Concept::parse("---\ntype: note\nid: nt_e2e\n---\n\n这是一篇关于 RAG 的笔记。\n")
            .unwrap();
    let text = "这是一篇关于 RAG 的笔记。";

    let result =
        generate_suggestions(&concept, "notes/rag.md", &sqlite, &reg, &routing, &guard, text)
            .await;
    assert!(
        result.is_ok(),
        "generate_suggestions should not error even if LLM unreachable"
    );

    let pending = sqlite.list_pending_suggestions().unwrap();
    println!("pending suggestions: {} (expect 0 if Ollama down)", pending.len());

    let v = vec![0.0f32; 768];
    let neighbors = sqlite.vector_search(&v, 5).unwrap();
    println!(
        "vector neighbors: {} (expect 0 if Ollama down)",
        neighbors.len()
    );
}
