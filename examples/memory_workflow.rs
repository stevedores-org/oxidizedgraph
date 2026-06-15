//! Example: Memory, retrieval, and context packing (Issue #23 / EPIC5)
//!
//! Run with: cargo run --example memory_workflow

use oxidizedgraph::prelude::*;

fn main() -> anyhow::Result<()> {
    let repo = "stevedores-org/oxidizedgraph";

    let mut index = RepositoryIndex::new();
    index.index_documents([
        RepositoryDocument::source(repo, "src/memory.rs", "context packing retrieval ranking")
            .with_symbol("ContextPacker"),
        RepositoryDocument::source(repo, "src/planning/plan.rs", "EpicPlan task decomposition scheduler"),
        RepositoryDocument::source(repo, "src/governance/node.rs", "GovernanceNode role guidance manifest"),
    ]);

    let query = RetrievalQuery::new("context packing governance")
        .repo(repo)
        .limit(3);
    let hits = index.query(&query);
    println!("Retrieval hits: {}", hits.len());
    for hit in &hits {
        println!("  {:.2} {} — {:?}", hit.score, hit.document.path, hit.matched_terms);
    }

    let mut store = AgentMemoryStore::new();
    store.record_episode(EpisodicMemory::new(
        "issue-23",
        "run-failed-1",
        repo,
        "Vector DB dependency blocked local dev — switched to lexical index.",
        RunOutcome::Failure,
    ));
    store.record_decision(
        DecisionMemory::new(
            "dec-ctx-pack",
            "issue-23",
            repo,
            "Use lexical RepositoryIndex + ContextPacker",
            "Keeps EPIC5 useful without external vector infrastructure.",
        )
        .with_alternative("Embed with external vector DB"),
    );

    let failures = store.failed_attempts_for_task("issue-23");
    println!("Prior failures for issue-23: {}", failures.len());

    let episodes: Vec<_> = store.episodes_for_task("issue-23");
    let decisions = store.decisions_for_repo(repo);
    let packer = ContextPacker::new(2_000).reserved_tokens(500);
    let packed = packer.pack(&hits, &episodes, &decisions, &ContextPolicy::default());

    println!(
        "Packed context: {} sections, ~{} tokens, truncated={}",
        packed.sections.len(),
        packed.estimated_tokens,
        packed.truncated
    );
    println!("\n--- prompt preview ---\n{}\n", packed.as_prompt());

    Ok(())
}
