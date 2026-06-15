//! Integration tests for EPIC5 memory and retrieval (#23).

use oxidizedgraph::prelude::*;

const REPO: &str = "stevedores-org/oxidizedgraph";

#[test]
fn epic5_retrieval_ranks_relevant_symbols() {
    let mut index = RepositoryIndex::new();
    index.index_document(
        RepositoryDocument::source(REPO, "src/memory.rs", "context packing retrieval")
            .with_symbol("ContextPacker"),
    );
    index.index_document(RepositoryDocument::source(
        REPO,
        "src/runner.rs",
        "graph runner invoke recursion",
    ));

    let results = index.query(&RetrievalQuery::new("context packer").repo(REPO));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].document.path, "src/memory.rs");
}

#[test]
fn epic5_failed_attempts_inform_planning_context() {
    let mut store = AgentMemoryStore::new();
    store.record_episode(EpisodicMemory::new(
        "issue-23",
        "run-1",
        REPO,
        "Index build timed out",
        RunOutcome::Failure,
    ));
    store.record_episode(EpisodicMemory::new(
        "issue-23",
        "run-2",
        REPO,
        "Completed with lexical index",
        RunOutcome::Success,
    ));

    let failures = store.failed_attempts_for_task("issue-23");
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].run_id, "run-1");
}

#[test]
fn epic5_decision_memory_is_queryable() {
    let mut store = AgentMemoryStore::new();
    store.record_decision(DecisionMemory::new(
        "dec-1",
        "issue-23",
        REPO,
        "Lexical retrieval baseline",
        "No vector DB dependency for local dev.",
    ));

    let hits = store.query_decisions("lexical retrieval", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "dec-1");
}

#[test]
fn epic5_context_packer_respects_token_budget() {
    let mut index = RepositoryIndex::new();
    let content = "retrieval ".repeat(200);
    index.index_document(RepositoryDocument::source(
        REPO,
        "src/memory.rs",
        content,
    ));

    let hits = index.query(&RetrievalQuery::new("retrieval").limit(1));
    let packer = ContextPacker::new(100);
    let packed = packer.pack(&hits, &[], &[], &ContextPolicy::default());

    assert!(packed.estimated_tokens <= 100);
    assert!(!packed.sections.is_empty() || packed.truncated);
}
