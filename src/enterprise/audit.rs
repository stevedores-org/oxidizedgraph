//! Compliance-grade audit retention and export.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Fields used to compute audit record hashes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AuditEventFields {
    /// Monotonic sequence within the log.
    pub sequence: u64,
    /// Tenant that produced the event.
    pub tenant_id: String,
    /// Run correlation id.
    pub run_id: String,
    /// Actor subject id.
    pub actor: String,
    /// Action verb (execute, export, deny, rotate_secret).
    pub action: String,
    /// Resource affected.
    pub resource: String,
    /// Outcome (allowed, denied, completed).
    pub outcome: String,
    /// Sanitized detail (no secret material).
    pub detail: String,
    /// SHA-256 of prior record for hash chain.
    pub prev_hash: String,
    /// Timestamp.
    pub recorded_at: DateTime<Utc>,
}

impl AuditEventFields {
    /// Create event fields; sequence, prev_hash, and timestamp are set on append.
    pub fn new(
        tenant_id: impl Into<String>,
        run_id: impl Into<String>,
        actor: impl Into<String>,
        action: impl Into<String>,
        resource: impl Into<String>,
        outcome: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            sequence: 0,
            tenant_id: tenant_id.into(),
            run_id: run_id.into(),
            actor: actor.into(),
            action: action.into(),
            resource: resource.into(),
            outcome: outcome.into(),
            detail: detail.into(),
            prev_hash: String::new(),
            recorded_at: Utc::now(),
        }
    }

    /// Compute hash for a record payload (excluding record_hash field).
    pub fn compute_hash(&self) -> String {
        let payload = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.sequence,
            self.tenant_id,
            self.run_id,
            self.actor,
            self.action,
            self.resource,
            self.outcome,
            self.detail,
            self.prev_hash,
            self.recorded_at
        );
        let mut hasher = Sha256::new();
        hasher.update(payload.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// Immutable audit record for compliance retention.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AuditRecord {
    /// Monotonic sequence within the log.
    pub sequence: u64,
    /// Tenant that produced the event.
    pub tenant_id: String,
    /// Run correlation id.
    pub run_id: String,
    /// Actor subject id.
    pub actor: String,
    /// Action verb (execute, export, deny, rotate_secret).
    pub action: String,
    /// Resource affected.
    pub resource: String,
    /// Outcome (allowed, denied, completed).
    pub outcome: String,
    /// Sanitized detail (no secret material).
    pub detail: String,
    /// SHA-256 of prior record for hash chain.
    pub prev_hash: String,
    /// Hash of this record.
    pub record_hash: String,
    /// Timestamp.
    pub recorded_at: DateTime<Utc>,
}

impl From<AuditEventFields> for AuditRecord {
    fn from(fields: AuditEventFields) -> Self {
        let record_hash = fields.compute_hash();
        Self {
            sequence: fields.sequence,
            tenant_id: fields.tenant_id,
            run_id: fields.run_id,
            actor: fields.actor,
            action: fields.action,
            resource: fields.resource,
            outcome: fields.outcome,
            detail: fields.detail,
            prev_hash: fields.prev_hash,
            record_hash,
            recorded_at: fields.recorded_at,
        }
    }
}

/// Append-only audit log with hash chain integrity.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AuditLog {
    records: Vec<AuditRecord>,
}

impl AuditLog {
    /// Create an empty audit log.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a sanitized audit record.
    pub fn append(&mut self, event: AuditEventFields) -> AuditRecord {
        let sequence = self.records.len() as u64 + 1;
        let prev_hash = self
            .records
            .last()
            .map(|r| r.record_hash.clone())
            .unwrap_or_else(|| "genesis".into());
        let fields = AuditEventFields {
            sequence,
            prev_hash,
            recorded_at: Utc::now(),
            ..event
        };
        let record = AuditRecord::from(fields);
        self.records.push(record.clone());
        record
    }

    /// All records (clone).
    pub fn records(&self) -> Vec<AuditRecord> {
        self.records.clone()
    }

    /// Verify hash chain integrity.
    pub fn verify_chain(&self) -> bool {
        let mut prev = "genesis".to_string();
        for record in &self.records {
            if record.prev_hash != prev {
                return false;
            }
            let expected = AuditEventFields {
                sequence: record.sequence,
                tenant_id: record.tenant_id.clone(),
                run_id: record.run_id.clone(),
                actor: record.actor.clone(),
                action: record.action.clone(),
                resource: record.resource.clone(),
                outcome: record.outcome.clone(),
                detail: record.detail.clone(),
                prev_hash: record.prev_hash.clone(),
                recorded_at: record.recorded_at,
            }
            .compute_hash();
            if expected != record.record_hash {
                return false;
            }
            prev = record.record_hash.clone();
        }
        true
    }
}

/// Compliance export bundle for auditors.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ComplianceExport {
    /// Export format version.
    pub version: u32,
    /// Tenant scope.
    pub tenant_id: String,
    /// Exported records.
    pub records: Vec<AuditRecord>,
    /// Whether hash chain verified at export time.
    pub chain_valid: bool,
    /// Export timestamp.
    pub exported_at: DateTime<Utc>,
}

/// Exports audit artifacts for compliance review.
#[derive(Clone, Debug, Default)]
pub struct ComplianceExporter;

impl ComplianceExporter {
    /// Create an exporter.
    pub fn new() -> Self {
        Self
    }

    /// Export tenant-scoped records with integrity check.
    pub fn export_tenant(&self, log: &AuditLog, tenant_id: &str) -> ComplianceExport {
        let records: Vec<AuditRecord> = log
            .records()
            .into_iter()
            .filter(|r| r.tenant_id == tenant_id)
            .collect();
        ComplianceExport {
            version: 1,
            tenant_id: tenant_id.to_string(),
            records,
            chain_valid: log.verify_chain(),
            exported_at: Utc::now(),
        }
    }

    /// Internal compliance checks on an export bundle.
    pub fn passes_internal_checks(&self, export: &ComplianceExport) -> bool {
        if !export.chain_valid {
            return false;
        }
        if export.records.is_empty() {
            return false;
        }
        for record in &export.records {
            if record.detail.contains("api_key=")
                || record.detail.contains("Bearer ghs_")
                || record.detail.contains("Bearer ghp_")
            {
                return false;
            }
        }
        true
    }
}

/// Context key for audit log handle.
pub const CTX_AUDIT_LOG: &str = "audit_log";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_chain_verifies() {
        let mut log = AuditLog::new();
        log.append(AuditEventFields::new(
            "t1", "run-1", "alice", "execute", "graph", "allowed", "ok",
        ));
        log.append(AuditEventFields::new(
            "t1", "run-1", "alice", "export", "audit", "completed", "bundle",
        ));
        assert!(log.verify_chain());
    }

    #[test]
    fn export_passes_compliance_checks() {
        let mut log = AuditLog::new();
        log.append(AuditEventFields::new(
            "t1", "run-1", "alice", "execute", "graph", "allowed", "sanitized detail",
        ));
        let exporter = ComplianceExporter::new();
        let export = exporter.export_tenant(&log, "t1");
        assert!(exporter.passes_internal_checks(&export));
    }
}
