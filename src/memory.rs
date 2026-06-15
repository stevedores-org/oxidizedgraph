//! Memory, repository retrieval, and context packing primitives.
//!
//! This module provides a lightweight in-process implementation for agent
//! memory. It is designed to be useful without a vector database while keeping
//! the public types serializable so callers can persist records in their own
//! storage layer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::cmp::{Ordering, Reverse};
use std::collections::{HashMap, HashSet};

/// A document extracted from a repository and made available for retrieval.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RepositoryDocument {
    /// Stable document identifier. Use a path, symbol path, or content hash.
    pub id: String,
    /// Repository name, for example `stevedores-org/oxidizedgraph`.
    pub repo: String,
    /// Repository-relative file path.
    pub path: String,
    /// Optional symbol or semantic anchor inside the file.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    /// The indexed text content.
    pub content: String,
    /// Document kind used to tune ranking and display.
    pub kind: DocumentKind,
    /// Arbitrary metadata carried with the document.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Time the document was indexed or last refreshed.
    pub updated_at: DateTime<Utc>,
}

impl RepositoryDocument {
    /// Create a source file document.
    pub fn source(
        repo: impl Into<String>,
        path: impl Into<String>,
        content: impl Into<String>,
    ) -> Self {
        let path = path.into();
        Self {
            id: path.clone(),
            repo: repo.into(),
            path,
            symbol: None,
            content: content.into(),
            kind: DocumentKind::Source,
            metadata: HashMap::new(),
            updated_at: Utc::now(),
        }
    }

    /// Attach a symbol name to the document.
    pub fn with_symbol(mut self, symbol: impl Into<String>) -> Self {
        self.symbol = Some(symbol.into());
        self
    }

    /// Replace the stable document identifier.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }
}

/// Type of repository document.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    /// Source code or generated code.
    Source,
    /// Human-authored documentation.
    Documentation,
    /// Configuration or deployment artifact.
    Configuration,
    /// Test source or fixture.
    Test,
    /// Other indexed text.
    Other,
}

/// Query parameters for retrieval.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetrievalQuery {
    /// Natural-language or keyword query.
    pub query: String,
    /// Optional repository filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Optional path prefixes to include.
    #[serde(default)]
    pub path_prefixes: Vec<String>,
    /// Maximum number of results.
    pub limit: usize,
    /// Minimum score a result must reach.
    pub min_score: f32,
}

impl RetrievalQuery {
    /// Create a retrieval query with conservative defaults.
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            repo: None,
            path_prefixes: Vec::new(),
            limit: 8,
            min_score: 0.0,
        }
    }

    /// Restrict the query to a repository.
    pub fn repo(mut self, repo: impl Into<String>) -> Self {
        self.repo = Some(repo.into());
        self
    }

    /// Restrict the query to a path prefix.
    pub fn path_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.path_prefixes.push(prefix.into());
        self
    }

    /// Set the result limit.
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    /// Set the minimum score threshold.
    pub fn min_score(mut self, min_score: f32) -> Self {
        self.min_score = min_score;
        self
    }
}

/// A ranked retrieval hit.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RetrievalResult {
    /// Retrieved document.
    pub document: RepositoryDocument,
    /// Lexical relevance score.
    pub score: f32,
    /// Query terms that matched the document.
    pub matched_terms: Vec<String>,
}

/// In-memory repository index for deterministic semantic-adjacent retrieval.
#[derive(Clone, Debug, Default)]
pub struct RepositoryIndex {
    documents: Vec<RepositoryDocument>,
    inverted: HashMap<String, HashSet<usize>>,
}

impl RepositoryIndex {
    /// Create an empty repository index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the number of indexed documents.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Return true when no documents are indexed.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Index a single document, replacing any prior document with the same ID.
    pub fn index_document(&mut self, document: RepositoryDocument) {
        self.remove_document(&document.id);
        let idx = self.documents.len();
        let terms = document_terms(&document);
        self.documents.push(document);

        for term in terms {
            self.inverted.entry(term).or_default().insert(idx);
        }
    }

    /// Index many documents.
    pub fn index_documents(&mut self, documents: impl IntoIterator<Item = RepositoryDocument>) {
        for document in documents {
            self.index_document(document);
        }
    }

    /// Remove a document by ID.
    pub fn remove_document(&mut self, id: &str) -> Option<RepositoryDocument> {
        let position = self.documents.iter().position(|doc| doc.id == id)?;
        let removed = self.documents.remove(position);
        self.rebuild_inverted_index();
        Some(removed)
    }

    /// Query the index and return ranked, de-duplicated results.
    pub fn query(&self, query: &RetrievalQuery) -> Vec<RetrievalResult> {
        if query.limit == 0 {
            return Vec::new();
        }

        let query_terms = tokenize(&query.query);
        if query_terms.is_empty() {
            return Vec::new();
        }

        let mut candidate_ids = HashSet::new();
        for term in &query_terms {
            if let Some(ids) = self.inverted.get(term) {
                candidate_ids.extend(ids.iter().copied());
            }
        }

        let mut results: Vec<_> = candidate_ids
            .into_iter()
            .filter_map(|idx| self.documents.get(idx))
            .filter(|document| matches_repo(document, query) && matches_path(document, query))
            .filter_map(|document| score_document(document, &query_terms))
            .filter(|result| result.score >= query.min_score)
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.document.id.cmp(&b.document.id))
        });
        results.truncate(query.limit);
        results
    }

    fn rebuild_inverted_index(&mut self) {
        self.inverted.clear();
        for (idx, document) in self.documents.iter().enumerate() {
            for term in document_terms(document) {
                self.inverted.entry(term).or_default().insert(idx);
            }
        }
    }
}

/// Outcome of a prior agent run.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    /// The run completed successfully.
    Success,
    /// The run completed only part of the requested work.
    Partial,
    /// The run failed and should inform future planning.
    Failure,
    /// The run was blocked by missing input, credentials, or external state.
    Blocked,
}

/// Durable episodic memory for a task/run/repository tuple.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EpisodicMemory {
    /// Task or issue identifier.
    pub task_id: String,
    /// Unique run identifier.
    pub run_id: String,
    /// Repository the run acted on.
    pub repo: String,
    /// Human-readable summary of what happened.
    pub summary: String,
    /// Run outcome.
    pub outcome: RunOutcome,
    /// Artifact identifiers, paths, commits, or PR URLs produced by the run.
    #[serde(default)]
    pub artifacts: Vec<String>,
    /// Time the memory was recorded.
    pub created_at: DateTime<Utc>,
}

impl EpisodicMemory {
    /// Create a new episodic memory record.
    pub fn new(
        task_id: impl Into<String>,
        run_id: impl Into<String>,
        repo: impl Into<String>,
        summary: impl Into<String>,
        outcome: RunOutcome,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            run_id: run_id.into(),
            repo: repo.into(),
            summary: summary.into(),
            outcome,
            artifacts: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Attach an artifact reference to the memory.
    pub fn with_artifact(mut self, artifact: impl Into<String>) -> Self {
        self.artifacts.push(artifact.into());
        self
    }
}

/// Durable decision memory for important implementation choices.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DecisionMemory {
    /// Stable decision identifier.
    pub id: String,
    /// Task or issue identifier.
    pub task_id: String,
    /// Repository where the decision applies.
    pub repo: String,
    /// Change or design choice that was made.
    pub change: String,
    /// Rationale for the decision.
    pub rationale: String,
    /// Alternatives considered.
    #[serde(default)]
    pub alternatives: Vec<String>,
    /// Time the decision was recorded.
    pub created_at: DateTime<Utc>,
}

impl DecisionMemory {
    /// Create a new decision memory record.
    pub fn new(
        id: impl Into<String>,
        task_id: impl Into<String>,
        repo: impl Into<String>,
        change: impl Into<String>,
        rationale: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            task_id: task_id.into(),
            repo: repo.into(),
            change: change.into(),
            rationale: rationale.into(),
            alternatives: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Attach an alternative considered during decision making.
    pub fn with_alternative(mut self, alternative: impl Into<String>) -> Self {
        self.alternatives.push(alternative.into());
        self
    }
}

/// In-memory durable-agent memory store.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentMemoryStore {
    episodes: Vec<EpisodicMemory>,
    decisions: Vec<DecisionMemory>,
}

impl AgentMemoryStore {
    /// Create an empty memory store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a prior run.
    pub fn record_episode(&mut self, memory: EpisodicMemory) {
        self.episodes.push(memory);
    }

    /// Record an implementation decision, replacing any existing record with the same ID.
    pub fn record_decision(&mut self, memory: DecisionMemory) {
        self.decisions.retain(|decision| decision.id != memory.id);
        self.decisions.push(memory);
    }

    /// Return episodes matching a task ID, newest first.
    pub fn episodes_for_task(&self, task_id: &str) -> Vec<&EpisodicMemory> {
        let mut episodes: Vec<_> = self
            .episodes
            .iter()
            .filter(|episode| episode.task_id == task_id)
            .collect();
        episodes.sort_by_key(|episode| Reverse(episode.created_at));
        episodes
    }

    /// Return failed or blocked attempts for a task, newest first.
    pub fn failed_attempts_for_task(&self, task_id: &str) -> Vec<&EpisodicMemory> {
        self.episodes_for_task(task_id)
            .into_iter()
            .filter(|episode| matches!(episode.outcome, RunOutcome::Failure | RunOutcome::Blocked))
            .collect()
    }

    /// Return decisions for a repository, newest first.
    pub fn decisions_for_repo(&self, repo: &str) -> Vec<&DecisionMemory> {
        let mut decisions: Vec<_> = self
            .decisions
            .iter()
            .filter(|decision| decision.repo == repo)
            .collect();
        decisions.sort_by_key(|decision| Reverse(decision.created_at));
        decisions
    }

    /// Query decisions by keywords in change, rationale, and alternatives.
    pub fn query_decisions(&self, query: &str, limit: usize) -> Vec<&DecisionMemory> {
        let terms = tokenize(query);
        if terms.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut scored: Vec<_> = self
            .decisions
            .iter()
            .filter_map(|decision| {
                let haystack = format!(
                    "{} {} {}",
                    decision.change,
                    decision.rationale,
                    decision.alternatives.join(" ")
                );
                let haystack_terms = tokenize(&haystack);
                let score = terms
                    .iter()
                    .filter(|term| haystack_terms.contains(*term))
                    .count();
                (score > 0).then_some((score, decision))
            })
            .collect();

        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.created_at.cmp(&a.1.created_at))
        });
        scored.truncate(limit);
        scored.into_iter().map(|(_, decision)| decision).collect()
    }
}

/// Context assembly policy.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextPolicy {
    /// Maximum number of retrieval documents to include.
    pub max_documents: usize,
    /// Maximum number of prior episodes to include.
    pub max_episodes: usize,
    /// Maximum number of decisions to include.
    pub max_decisions: usize,
}

impl Default for ContextPolicy {
    fn default() -> Self {
        Self {
            max_documents: 6,
            max_episodes: 3,
            max_decisions: 3,
        }
    }
}

/// Context section assembled for an agent prompt.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextSection {
    /// Section title.
    pub title: String,
    /// Section body.
    pub body: String,
    /// Estimated token cost for the section.
    pub estimated_tokens: usize,
}

/// Packed context ready to inject into a prompt.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackedContext {
    /// Included context sections.
    pub sections: Vec<ContextSection>,
    /// Total estimated tokens.
    pub estimated_tokens: usize,
    /// True when one or more candidate sections did not fit.
    pub truncated: bool,
}

impl PackedContext {
    /// Render the packed context as markdown.
    pub fn as_prompt(&self) -> String {
        self.sections
            .iter()
            .map(|section| format!("## {}\n\n{}", section.title, section.body))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// Budgeted context packer.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextPacker {
    /// Maximum estimated tokens for the final context.
    pub max_tokens: usize,
    /// Tokens reserved for model instructions and response room.
    pub reserved_tokens: usize,
}

impl ContextPacker {
    /// Create a packer with a maximum token budget.
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            reserved_tokens: 0,
        }
    }

    /// Reserve tokens for caller-managed prompt content.
    pub fn reserved_tokens(mut self, reserved_tokens: usize) -> Self {
        self.reserved_tokens = reserved_tokens;
        self
    }

    /// Assemble repository retrieval, episode memory, and decision memory.
    pub fn pack(
        &self,
        retrieval: &[RetrievalResult],
        episodes: &[&EpisodicMemory],
        decisions: &[&DecisionMemory],
        policy: &ContextPolicy,
    ) -> PackedContext {
        let budget = self.max_tokens.saturating_sub(self.reserved_tokens);
        let mut sections = Vec::new();
        let mut estimated_tokens = 0;
        let mut truncated = false;

        for result in retrieval.iter().take(policy.max_documents) {
            let symbol = result
                .document
                .symbol
                .as_deref()
                .map(|symbol| format!(" ({symbol})"))
                .unwrap_or_default();
            let title = format!("{}{}", result.document.path, symbol);
            let body = format!(
                "repo: {}\nscore: {:.2}\nmatched: {}\n\n{}",
                result.document.repo,
                result.score,
                result.matched_terms.join(", "),
                result.document.content
            );
            append_section(
                &mut sections,
                &mut estimated_tokens,
                &mut truncated,
                budget,
                title,
                body,
            );
        }

        for episode in episodes.iter().take(policy.max_episodes) {
            let title = format!("prior run {} ({:?})", episode.run_id, episode.outcome);
            let body = format!(
                "task: {}\nrepo: {}\nsummary: {}\nartifacts: {}",
                episode.task_id,
                episode.repo,
                episode.summary,
                episode.artifacts.join(", ")
            );
            append_section(
                &mut sections,
                &mut estimated_tokens,
                &mut truncated,
                budget,
                title,
                body,
            );
        }

        for decision in decisions.iter().take(policy.max_decisions) {
            let title = format!("decision {}", decision.id);
            let body = format!(
                "task: {}\nrepo: {}\nchange: {}\nrationale: {}\nalternatives: {}",
                decision.task_id,
                decision.repo,
                decision.change,
                decision.rationale,
                decision.alternatives.join(", ")
            );
            append_section(
                &mut sections,
                &mut estimated_tokens,
                &mut truncated,
                budget,
                title,
                body,
            );
        }

        PackedContext {
            sections,
            estimated_tokens,
            truncated,
        }
    }
}

fn append_section(
    sections: &mut Vec<ContextSection>,
    estimated_tokens: &mut usize,
    truncated: &mut bool,
    budget: usize,
    title: String,
    body: String,
) {
    let tokens = estimate_tokens(&title) + estimate_tokens(&body);
    if *estimated_tokens + tokens > budget {
        *truncated = true;
        return;
    }

    *estimated_tokens += tokens;
    sections.push(ContextSection {
        title,
        body,
        estimated_tokens: tokens,
    });
}

fn matches_repo(document: &RepositoryDocument, query: &RetrievalQuery) -> bool {
    query
        .repo
        .as_ref()
        .map(|repo| &document.repo == repo)
        .unwrap_or(true)
}

fn matches_path(document: &RepositoryDocument, query: &RetrievalQuery) -> bool {
    query.path_prefixes.is_empty()
        || query
            .path_prefixes
            .iter()
            .any(|prefix| document.path.starts_with(prefix))
}

fn score_document(
    document: &RepositoryDocument,
    query_terms: &[String],
) -> Option<RetrievalResult> {
    let content_terms = tokenize(&document.content);
    let path_terms = tokenize(&document.path);
    let symbol_terms = document
        .symbol
        .as_ref()
        .map(|symbol| tokenize(symbol))
        .unwrap_or_default();

    let mut score = 0.0;
    let mut matched_terms = Vec::new();
    for term in query_terms {
        let mut term_score = 0.0;
        if path_terms.contains(term) {
            term_score += 3.0;
        }
        if symbol_terms.contains(term) {
            term_score += 4.0;
        }
        let frequency = content_terms
            .iter()
            .filter(|candidate| *candidate == term)
            .count();
        if frequency > 0 {
            term_score += 1.0 + frequency as f32;
        }
        if term_score > 0.0 {
            matched_terms.push(term.clone());
            score += term_score;
        }
    }

    (!matched_terms.is_empty()).then_some(RetrievalResult {
        document: document.clone(),
        score,
        matched_terms,
    })
}

fn document_terms(document: &RepositoryDocument) -> HashSet<String> {
    let mut terms: HashSet<_> = tokenize(&document.content).into_iter().collect();
    terms.extend(tokenize(&document.path));
    if let Some(symbol) = &document.symbol {
        terms.extend(tokenize(symbol));
    }
    terms
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|term| term.len() > 1)
        .map(|term| term.to_ascii_lowercase())
        .collect()
}

fn estimate_tokens(text: &str) -> usize {
    (text.len() / 4).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(path: &str, content: &str) -> RepositoryDocument {
        RepositoryDocument::source("stevedores-org/oxidizedgraph", path, content)
    }

    #[test]
    fn repository_index_ranks_symbol_and_content_matches() {
        let mut index = RepositoryIndex::new();
        index.index_documents([
            doc(
                "src/memory.rs",
                "context packing retrieval ranking decisions",
            )
            .with_symbol("ContextPacker"),
            doc("src/graph.rs", "graph execution nodes edges"),
        ]);

        let results = index.query(&RetrievalQuery::new("context packer ranking").limit(2));

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].document.path, "src/memory.rs");
        assert!(results[0].score > 3.0);
        assert!(results[0].matched_terms.contains(&"context".to_string()));
    }

    #[test]
    fn repository_index_replaces_documents_by_id() {
        let mut index = RepositoryIndex::new();
        index.index_document(doc("src/lib.rs", "old memory"));
        index.index_document(doc("src/lib.rs", "new retrieval"));

        let old_results = index.query(&RetrievalQuery::new("old"));
        let new_results = index.query(&RetrievalQuery::new("new"));

        assert!(old_results.is_empty());
        assert_eq!(new_results.len(), 1);
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn memory_store_returns_failed_attempts_and_queryable_decisions() {
        let mut store = AgentMemoryStore::new();
        store.record_episode(EpisodicMemory::new(
            "issue-23",
            "run-1",
            "stevedores-org/oxidizedgraph",
            "Tried vector retrieval before indexing existed.",
            RunOutcome::Failure,
        ));
        store.record_episode(EpisodicMemory::new(
            "issue-23",
            "run-2",
            "stevedores-org/oxidizedgraph",
            "Added deterministic lexical retrieval.",
            RunOutcome::Success,
        ));
        store.record_decision(
            DecisionMemory::new(
                "decision-1",
                "issue-23",
                "stevedores-org/oxidizedgraph",
                "Use lexical retrieval first",
                "It avoids a mandatory vector database for local agents.",
            )
            .with_alternative("Require embeddings at index time"),
        );

        assert_eq!(store.failed_attempts_for_task("issue-23").len(), 1);
        let decisions = store.query_decisions("vector database", 4);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].id, "decision-1");
    }

    #[test]
    fn context_packer_respects_budget_and_marks_truncation() {
        let result = RetrievalResult {
            document: doc("src/memory.rs", "relevant context ".repeat(40).as_str()),
            score: 10.0,
            matched_terms: vec!["context".to_string()],
        };
        let episode = EpisodicMemory::new(
            "issue-23",
            "run-1",
            "stevedores-org/oxidizedgraph",
            "previous failed attempt should be considered",
            RunOutcome::Failure,
        );
        let decision = DecisionMemory::new(
            "decision-1",
            "issue-23",
            "stevedores-org/oxidizedgraph",
            "keep prompt compact",
            "budgeted context avoids overflowing model limits",
        );

        let packer = ContextPacker::new(70);
        let packed = packer.pack(
            &[result],
            &[&episode],
            &[&decision],
            &ContextPolicy::default(),
        );

        assert!(packed.estimated_tokens <= 70);
        assert!(packed.truncated);
        assert!(!packed.sections.is_empty());
    }
}
pub mod decision;
