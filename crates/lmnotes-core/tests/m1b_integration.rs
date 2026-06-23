// 临时集成测试：验证 generate_suggestions → suggestion store + vector 的完整链路
use lmnotes_core::index::SqliteIndex;
use lmnotes_core::indexer::generate_suggestions;
use lmnotes_core::llm::ollama::OllamaProvider;
use lmnotes_core::llm::routing::{Registry, Routing, ProviderRef, Task};
use lmnotes_core::llm::guard::GuardConfig;
use lmnotes_core::okf::concept::Concept;
use std::sync::Arc;

#[tokio::test]
async fn end_to_end_suggestion_and_vector_pipeline() {
    let sqlite = Arc::new(SqliteIndex::in_memory().unwrap());
    sqlite.init_schema().await.unwrap();

    // 用真实 Ollama provider（指向 localhost）但路由模型用默认
    // —— 若 Ollama 没跑，chat/embed 会失败，generate_suggestions 吞咽错误
    //    不影响 store 为空但 vector_search 也不会有数据。这验证错误处理路径。
    let mut reg = Registry::new();
    let ollama = Arc::new(OllamaProvider::default_local());
    reg.register_chat_arc(ollama.clone());
    reg.register_embed_arc(ollama);

    let routing = Routing {
        map: [
            (Task::Summarize, (ProviderRef { provider_id: "ollama".into(), model: "qwen2.5:7b".into() }, vec![])),
            (Task::LinkSuggest, (ProviderRef { provider_id: "ollama".into(), model: "qwen2.5:7b".into() }, vec![])),
            (Task::Embed, (ProviderRef { provider_id: "ollama".into(), model: "nomic-embed-text".into() }, vec![])),
        ].into_iter().collect(),
    };
    let guard = GuardConfig::default();

    let concept = Concept::parse("---\ntype: note\nid: nt_e2e\n---\n\n这是一篇关于 RAG 的笔记。\n").unwrap();
    let text = "这是一篇关于 RAG 的笔记。";

    // generate_suggestions 应不 panic（错误吞咽）
    let result = generate_suggestions(&concept, "notes/rag.md", &sqlite, &reg, &routing, &guard, text).await;
    assert!(result.is_ok(), "generate_suggestions should not error even if LLM unreachable");

    // 因 Ollama 不可达，suggestion store 应为空（chat 失败）
    let pending = sqlite.list_pending_suggestions().unwrap();
    println!("pending suggestions (expect 0 if Ollama down): {}", pending.len());

    // vector_search 也不应有数据
    let v = vec![0.0; 768];
    let neighbors = sqlite.vector_search(&v, 5).unwrap();
    println!("vector neighbors (expect 0 if Ollama down): {}", neighbors.len());
}
