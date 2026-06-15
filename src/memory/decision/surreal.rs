//! SurrealDB-backed decision store for production agents.
//!
//! Gated by the `persistence` feature, mirroring [`SurrealCheckpointer`].
//!
//! ```toml
//! oxidizedgraph = { version = "0.2", features = ["persistence"] }
//! ```
//!
//! [`SurrealCheckpointer`]: crate::checkpoint::SurrealCheckpointer

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use surrealdb::engine::any::{connect, Any};
use surrealdb::opt::auth::Root;
use surrealdb::sql::Thing;
use surrealdb::Surreal;

use super::{Decision, DecisionStore};
use crate::error::RuntimeError;

const TABLE: &str = "decisions";

/// SurrealDB-backed implementation of [`DecisionStore`].
///
/// Supports the same connection modes as the existing
/// [`SurrealCheckpointer`]:
///
/// - In-memory: [`SurrealDecisionStore::memory`]
/// - File-based: [`SurrealDecisionStore::file`]
/// - Remote: [`SurrealDecisionStore::connect_remote`]
///
/// [`SurrealCheckpointer`]: crate::checkpoint::SurrealCheckpointer
pub struct SurrealDecisionStore {
    db: Surreal<Any>,
}

/// Builder for connecting [`SurrealDecisionStore`] to a remote SurrealDB.
pub struct SurrealDecisionStoreBuilder {
    url: String,
    username: Option<String>,
    password: Option<String>,
    namespace: String,
    database: String,
}

impl SurrealDecisionStoreBuilder {
    /// Set authentication credentials.
    pub fn credentials(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    /// Set the namespace.
    pub fn namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = ns.into();
        self
    }

    /// Set the database.
    pub fn database(mut self, db: impl Into<String>) -> Self {
        self.database = db.into();
        self
    }

    /// Connect, sign in, and select ns/db.
    pub async fn build(self) -> Result<SurrealDecisionStore, RuntimeError> {
        let db: Surreal<Any> = connect(&self.url)
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB connect failed: {}", e)))?;

        if let (Some(user), Some(pass)) = (self.username, self.password) {
            db.signin(Root {
                username: &user,
                password: &pass,
            })
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB auth failed: {}", e)))?;
        }

        db.use_ns(&self.namespace)
            .use_db(&self.database)
            .await
            .map_err(|e| {
                RuntimeError::InvalidState(format!("SurrealDB use ns/db failed: {}", e))
            })?;

        let store = SurrealDecisionStore { db };
        store.setup_schema().await?;
        Ok(store)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DecisionRecord {
    #[allow(dead_code)]
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Thing>,
    decision_id: String,
    run_id: String,
    change_ref: String,
    title: String,
    rationale: String,
    #[serde(default)]
    tags: Vec<String>,
    created_at: String,
    #[serde(default)]
    metadata: serde_json::Value,
}

impl From<Decision> for DecisionRecord {
    fn from(d: Decision) -> Self {
        Self {
            id: None,
            decision_id: d.id,
            run_id: d.run_id,
            change_ref: d.change_ref,
            title: d.title,
            rationale: d.rationale,
            tags: d.tags,
            created_at: d.created_at.to_rfc3339(),
            metadata: if d.metadata.is_null() {
                serde_json::json!({})
            } else {
                d.metadata
            },
        }
    }
}

impl TryFrom<DecisionRecord> for Decision {
    type Error = RuntimeError;

    fn try_from(r: DecisionRecord) -> Result<Self, Self::Error> {
        let created_at = chrono::DateTime::parse_from_rfc3339(&r.created_at)
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to parse timestamp: {}", e)))?
            .with_timezone(&chrono::Utc);

        Ok(Decision {
            id: r.decision_id,
            run_id: r.run_id,
            change_ref: r.change_ref,
            title: r.title,
            rationale: r.rationale,
            tags: r.tags,
            created_at,
            metadata: r.metadata,
        })
    }
}

impl SurrealDecisionStore {
    /// In-memory backend, for tests.
    pub async fn memory() -> Result<Self, RuntimeError> {
        Self::open("mem://").await
    }

    /// File-backed embedded store.
    pub async fn file(path: impl AsRef<str>) -> Result<Self, RuntimeError> {
        let url = format!("file://{}", path.as_ref());
        Self::open(&url).await
    }

    /// Start a builder for a remote SurrealDB connection.
    pub fn connect_remote(url: impl Into<String>) -> SurrealDecisionStoreBuilder {
        SurrealDecisionStoreBuilder {
            url: url.into(),
            username: None,
            password: None,
            namespace: "oxidizedgraph".to_string(),
            database: "decisions".to_string(),
        }
    }

    async fn open(url: &str) -> Result<Self, RuntimeError> {
        let db: Surreal<Any> = connect(url)
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB connect failed: {}", e)))?;

        db.use_ns("oxidizedgraph")
            .use_db("decisions")
            .await
            .map_err(|e| {
                RuntimeError::InvalidState(format!("SurrealDB use ns/db failed: {}", e))
            })?;

        let store = Self { db };
        store.setup_schema().await?;
        Ok(store)
    }

    async fn setup_schema(&self) -> Result<(), RuntimeError> {
        self.db
            .query(
                r#"
                DEFINE INDEX IF NOT EXISTS idx_run_id ON TABLE decisions COLUMNS run_id;
                DEFINE INDEX IF NOT EXISTS idx_change_ref ON TABLE decisions COLUMNS change_ref;
                DEFINE INDEX IF NOT EXISTS idx_tags ON TABLE decisions COLUMNS tags;
                DEFINE INDEX IF NOT EXISTS idx_created_at ON TABLE decisions COLUMNS created_at;
                "#,
            )
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to setup schema: {}", e)))?;
        Ok(())
    }

    async fn query_records(
        &self,
        sql: &'static str,
        bindings: Vec<(&'static str, String)>,
    ) -> Result<Vec<DecisionRecord>, RuntimeError> {
        let mut q = self.db.query(sql);
        for (k, v) in bindings {
            q = q.bind((k, v));
        }
        let mut result = q
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB query failed: {}", e)))?;
        result
            .take(0)
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to parse result: {}", e)))
    }
}

#[async_trait]
impl DecisionStore for SurrealDecisionStore {
    async fn record(&self, decision: Decision) -> Result<(), RuntimeError> {
        let record: DecisionRecord = decision.into();
        let id = record.decision_id.clone();

        // upsert: a re-`record` of the same id must overwrite, matching the
        // in-memory backend's contract.
        let _: Option<DecisionRecord> = self
            .db
            .upsert((TABLE, id))
            .content(record)
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to record decision: {}", e)))?;
        Ok(())
    }

    async fn get(&self, id: &str) -> Result<Option<Decision>, RuntimeError> {
        let record: Option<DecisionRecord> =
            self.db.select((TABLE, id)).await.map_err(|e| {
                RuntimeError::InvalidState(format!("Failed to load decision: {}", e))
            })?;
        match record {
            Some(r) => Ok(Some(r.try_into()?)),
            None => Ok(None),
        }
    }

    async fn list_by_run(&self, run_id: &str) -> Result<Vec<Decision>, RuntimeError> {
        let records = self
            .query_records(
                "SELECT * FROM decisions WHERE run_id = $run_id ORDER BY created_at DESC",
                vec![("run_id", run_id.to_string())],
            )
            .await?;
        records.into_iter().map(|r| r.try_into()).collect()
    }

    async fn list_by_change(&self, change_ref: &str) -> Result<Vec<Decision>, RuntimeError> {
        let records = self
            .query_records(
                "SELECT * FROM decisions WHERE change_ref = $change_ref ORDER BY created_at DESC",
                vec![("change_ref", change_ref.to_string())],
            )
            .await?;
        records.into_iter().map(|r| r.try_into()).collect()
    }

    async fn list_by_tag(&self, tag: &str) -> Result<Vec<Decision>, RuntimeError> {
        let records = self
            .query_records(
                "SELECT * FROM decisions WHERE $tag IN tags ORDER BY created_at DESC",
                vec![("tag", tag.to_string())],
            )
            .await?;
        records.into_iter().map(|r| r.try_into()).collect()
    }

    async fn recent(&self, limit: usize) -> Result<Vec<Decision>, RuntimeError> {
        let mut result = self
            .db
            .query("SELECT * FROM decisions ORDER BY created_at DESC LIMIT $limit")
            .bind(("limit", limit as i64))
            .await
            .map_err(|e| RuntimeError::InvalidState(format!("SurrealDB query failed: {}", e)))?;
        let records: Vec<DecisionRecord> = result
            .take(0)
            .map_err(|e| RuntimeError::InvalidState(format!("Failed to parse result: {}", e)))?;
        records.into_iter().map(|r| r.try_into()).collect()
    }

    async fn delete(&self, id: &str) -> Result<(), RuntimeError> {
        let _: Option<DecisionRecord> =
            self.db.delete((TABLE, id)).await.map_err(|e| {
                RuntimeError::InvalidState(format!("Failed to delete decision: {}", e))
            })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(run: &str, change: &str, title: &str) -> Decision {
        Decision::new(run, change, title, "rationale")
    }

    #[tokio::test]
    async fn round_trip_through_memory_backend() {
        let store = SurrealDecisionStore::memory().await.unwrap();
        let decision = d("run-1", "src/a.rs", "swap lock").with_tag("perf");
        let id = decision.id.clone();
        store.record(decision).await.unwrap();

        let got = store.get(&id).await.unwrap().expect("present");
        assert_eq!(got.title, "swap lock");
        assert_eq!(got.tags, vec!["perf".to_string()]);
    }

    #[tokio::test]
    async fn list_by_run_orders_newest_first() {
        let store = SurrealDecisionStore::memory().await.unwrap();
        store.record(d("run-1", "a", "first")).await.unwrap();
        // Sleep briefly so created_at differs at rfc3339 resolution.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        store.record(d("run-1", "b", "second")).await.unwrap();

        let listed = store.list_by_run("run-1").await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].title, "second");
    }

    #[tokio::test]
    async fn list_by_change_and_tag() {
        let store = SurrealDecisionStore::memory().await.unwrap();
        store
            .record(d("run-1", "src/a.rs", "a1").with_tag("perf"))
            .await
            .unwrap();
        store
            .record(d("run-1", "src/b.rs", "b1").with_tag("security"))
            .await
            .unwrap();
        store
            .record(d("run-2", "src/a.rs", "a2").with_tag("perf"))
            .await
            .unwrap();

        let by_change = store.list_by_change("src/a.rs").await.unwrap();
        assert_eq!(by_change.len(), 2);

        let by_tag = store.list_by_tag("perf").await.unwrap();
        assert_eq!(by_tag.len(), 2);

        assert!(store.list_by_tag("nope").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn recent_respects_limit() {
        let store = SurrealDecisionStore::memory().await.unwrap();
        for i in 0..5 {
            store
                .record(d("run-1", &format!("c{i}"), &format!("t{i}")))
                .await
                .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }
        let top = store.recent(3).await.unwrap();
        assert_eq!(top.len(), 3);
        // Newest first.
        assert_eq!(top[0].title, "t4");
    }

    #[tokio::test]
    async fn delete_round_trips() {
        let store = SurrealDecisionStore::memory().await.unwrap();
        let decision = d("run-1", "src/a.rs", "x");
        let id = decision.id.clone();
        store.record(decision).await.unwrap();
        store.delete(&id).await.unwrap();
        assert!(store.get(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn record_overwrites_same_id() {
        let store = SurrealDecisionStore::memory().await.unwrap();
        let mut decision = d("run-1", "src/a.rs", "v1");
        let id = decision.id.clone();
        store.record(decision.clone()).await.unwrap();

        decision.title = "v2".into();
        store.record(decision).await.unwrap();

        let got = store.get(&id).await.unwrap().expect("present");
        assert_eq!(got.title, "v2");
        // Still only one record overall.
        assert_eq!(store.list_by_run("run-1").await.unwrap().len(), 1);
    }
}
