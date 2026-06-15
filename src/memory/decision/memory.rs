//! In-memory decision store for tests and single-process agents.
//!
//! Data is lost when the process exits — for durable storage, enable the
//! `persistence` feature and use [`SurrealDecisionStore`].
//!
//! [`SurrealDecisionStore`]: super::SurrealDecisionStore

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;

use super::{Decision, DecisionStore};
use crate::error::RuntimeError;

fn poisoned<E: std::fmt::Display>(e: E) -> RuntimeError {
    RuntimeError::InvalidState(format!("decision store lock poisoned: {}", e))
}

/// In-memory implementation of [`DecisionStore`].
///
/// Holds three indexes (by run, by change_ref, by tag) alongside the
/// canonical id → decision map. Indexes are append-only on `record` and
/// pruned on `delete`. Listings are sorted newest-first at read time.
pub struct MemoryDecisionStore {
    by_id: RwLock<HashMap<String, Decision>>,
    by_run: RwLock<HashMap<String, Vec<String>>>,
    by_change: RwLock<HashMap<String, Vec<String>>>,
    by_tag: RwLock<HashMap<String, Vec<String>>>,
}

impl MemoryDecisionStore {
    /// Create an empty in-memory store.
    pub fn new() -> Self {
        Self {
            by_id: RwLock::new(HashMap::new()),
            by_run: RwLock::new(HashMap::new()),
            by_change: RwLock::new(HashMap::new()),
            by_tag: RwLock::new(HashMap::new()),
        }
    }

    fn sorted_newest_first(&self, ids: &[String]) -> Result<Vec<Decision>, RuntimeError> {
        let by_id = self.by_id.read().map_err(poisoned)?;
        let mut out: Vec<Decision> = ids.iter().filter_map(|id| by_id.get(id).cloned()).collect();
        out.sort_by_key(|d| std::cmp::Reverse(d.created_at));
        Ok(out)
    }
}

impl Default for MemoryDecisionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DecisionStore for MemoryDecisionStore {
    async fn record(&self, decision: Decision) -> Result<(), RuntimeError> {
        let id = decision.id.clone();
        let run_id = decision.run_id.clone();
        let change_ref = decision.change_ref.clone();
        let tags = decision.tags.clone();

        let prev = {
            let mut by_id = self.by_id.write().map_err(poisoned)?;
            by_id.insert(id.clone(), decision)
        };

        // If we're overwriting, drop the prior index entries first so
        // run/change/tag lookups don't return stale duplicates.
        if let Some(prev) = prev {
            self.remove_from_indexes(&prev)?;
        }

        {
            let mut by_run = self.by_run.write().map_err(poisoned)?;
            by_run.entry(run_id).or_default().push(id.clone());
        }
        {
            let mut by_change = self.by_change.write().map_err(poisoned)?;
            by_change.entry(change_ref).or_default().push(id.clone());
        }
        {
            let mut by_tag = self.by_tag.write().map_err(poisoned)?;
            for tag in tags {
                by_tag.entry(tag).or_default().push(id.clone());
            }
        }

        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<Decision>, RuntimeError> {
        let by_id = self.by_id.read().map_err(poisoned)?;
        Ok(by_id.get(id).cloned())
    }

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<Decision>, RuntimeError> {
        let ids = {
            let by_run = self.by_run.read().map_err(poisoned)?;
            by_run.get(run_id).cloned().unwrap_or_default()
        };
        self.sorted_newest_first(&ids)
    }

    async fn list_by_change(&self, change_ref: &str) -> Result<Vec<Decision>, RuntimeError> {
        let ids = {
            let by_change = self.by_change.read().map_err(poisoned)?;
            by_change.get(change_ref).cloned().unwrap_or_default()
        };
        self.sorted_newest_first(&ids)
    }

    async fn list_by_tag(&self, tag: &str) -> Result<Vec<Decision>, RuntimeError> {
        let ids = {
            let by_tag = self.by_tag.read().map_err(poisoned)?;
            by_tag.get(tag).cloned().unwrap_or_default()
        };
        self.sorted_newest_first(&ids)
    }

    async fn recent(&self, limit: usize) -> Result<Vec<Decision>, RuntimeError> {
        let mut all: Vec<Decision> = {
            let by_id = self.by_id.read().map_err(poisoned)?;
            by_id.values().cloned().collect()
        };
        all.sort_by_key(|d| std::cmp::Reverse(d.created_at));
        all.truncate(limit);
        Ok(all)
    }

    async fn delete(&self, id: &str) -> Result<(), RuntimeError> {
        let removed = {
            let mut by_id = self.by_id.write().map_err(poisoned)?;
            by_id.remove(id)
        };
        if let Some(d) = removed {
            self.remove_from_indexes(&d)?;
        }
        Ok(())
    }
}

impl MemoryDecisionStore {
    fn remove_from_indexes(&self, d: &Decision) -> Result<(), RuntimeError> {
        {
            let mut by_run = self.by_run.write().map_err(poisoned)?;
            if let Some(ids) = by_run.get_mut(&d.run_id) {
                ids.retain(|x| x != &d.id);
            }
        }
        {
            let mut by_change = self.by_change.write().map_err(poisoned)?;
            if let Some(ids) = by_change.get_mut(&d.change_ref) {
                ids.retain(|x| x != &d.id);
            }
        }
        {
            let mut by_tag = self.by_tag.write().map_err(poisoned)?;
            for tag in &d.tags {
                if let Some(ids) = by_tag.get_mut(tag) {
                    ids.retain(|x| x != &d.id);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn d(run: &str, change: &str, title: &str) -> Decision {
        Decision::new(run, change, title, "rationale")
    }

    #[tokio::test]
    async fn record_and_get_round_trip() {
        let store = MemoryDecisionStore::new();
        let decision = d("run-1", "src/a.rs", "swap lock").with_tag("perf");
        let id = decision.id.clone();
        store.record(decision).await.unwrap();

        let got = store.get(&id).await.unwrap().expect("present");
        assert_eq!(got.title, "swap lock");
        assert_eq!(got.tags, vec!["perf"]);
    }

    #[tokio::test]
    async fn get_missing_returns_none() {
        let store = MemoryDecisionStore::new();
        assert!(store.get("does-not-exist").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_by_run_returns_newest_first() {
        let store = MemoryDecisionStore::new();
        let mut older = d("run-1", "a", "older");
        let newer = d("run-1", "b", "newer");
        // Force a deterministic ordering — `Utc::now()` resolution can
        // collide inside a single test on fast machines.
        older.created_at = newer.created_at - Duration::seconds(10);
        store.record(older).await.unwrap();
        store.record(newer).await.unwrap();

        let listed = store.list_by_run("run-1").await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].title, "newer");
        assert_eq!(listed[1].title, "older");
    }

    #[tokio::test]
    async fn list_by_change_isolates_artifacts() {
        let store = MemoryDecisionStore::new();
        store.record(d("run-1", "src/a.rs", "a1")).await.unwrap();
        store.record(d("run-1", "src/b.rs", "b1")).await.unwrap();
        store.record(d("run-2", "src/a.rs", "a2")).await.unwrap();

        let a = store.list_by_change("src/a.rs").await.unwrap();
        assert_eq!(a.len(), 2);
        assert!(a.iter().all(|d| d.change_ref == "src/a.rs"));

        let b = store.list_by_change("src/b.rs").await.unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].title, "b1");

        assert!(store.list_by_change("src/c.rs").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_by_tag_groups_across_runs_and_changes() {
        let store = MemoryDecisionStore::new();
        store
            .record(d("run-1", "src/a.rs", "perf change").with_tag("perf"))
            .await
            .unwrap();
        store
            .record(
                d("run-2", "src/b.rs", "perf + sec change")
                    .with_tag("perf")
                    .with_tag("security"),
            )
            .await
            .unwrap();
        store
            .record(d("run-3", "src/c.rs", "doc only").with_tag("docs"))
            .await
            .unwrap();

        let perf = store.list_by_tag("perf").await.unwrap();
        assert_eq!(perf.len(), 2);

        let sec = store.list_by_tag("security").await.unwrap();
        assert_eq!(sec.len(), 1);

        assert!(store.list_by_tag("nope").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn recent_truncates_and_orders() {
        let store = MemoryDecisionStore::new();
        let now = Utc::now();
        for i in 0..5 {
            let mut decision = d("run-1", &format!("change-{i}"), &format!("t{i}"));
            decision.created_at = now + Duration::seconds(i);
            store.record(decision).await.unwrap();
        }

        let top3 = store.recent(3).await.unwrap();
        assert_eq!(top3.len(), 3);
        assert_eq!(top3[0].title, "t4");
        assert_eq!(top3[1].title, "t3");
        assert_eq!(top3[2].title, "t2");
    }

    #[tokio::test]
    async fn delete_removes_from_all_indexes() {
        let store = MemoryDecisionStore::new();
        let decision = d("run-1", "src/a.rs", "t").with_tag("perf");
        let id = decision.id.clone();
        store.record(decision).await.unwrap();
        store.delete(&id).await.unwrap();

        assert!(store.get(&id).await.unwrap().is_none());
        assert!(store.list_by_run("run-1").await.unwrap().is_empty());
        assert!(store.list_by_change("src/a.rs").await.unwrap().is_empty());
        assert!(store.list_by_tag("perf").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_missing_is_noop() {
        let store = MemoryDecisionStore::new();
        store.delete("never-recorded").await.unwrap();
    }

    #[tokio::test]
    async fn record_overwrites_and_reindexes() {
        let store = MemoryDecisionStore::new();
        let original = d("run-1", "src/a.rs", "v1").with_tag("perf");
        let id = original.id.clone();
        store.record(original).await.unwrap();

        // Re-record under the same id but with different run/change/tags.
        let updated = Decision {
            id: id.clone(),
            run_id: "run-2".into(),
            change_ref: "src/b.rs".into(),
            title: "v2".into(),
            rationale: "different".into(),
            tags: vec!["security".into()],
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
        };
        store.record(updated).await.unwrap();

        // Old indexes must be empty; new indexes must contain it once.
        assert!(store.list_by_run("run-1").await.unwrap().is_empty());
        assert!(store.list_by_change("src/a.rs").await.unwrap().is_empty());
        assert!(store.list_by_tag("perf").await.unwrap().is_empty());

        assert_eq!(store.list_by_run("run-2").await.unwrap().len(), 1);
        assert_eq!(store.list_by_change("src/b.rs").await.unwrap().len(), 1);
        assert_eq!(store.list_by_tag("security").await.unwrap().len(), 1);
    }
}
