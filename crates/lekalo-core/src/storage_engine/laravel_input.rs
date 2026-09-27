//! The bounded Laravel migration input document (issue #57).
//!
//! [`laravel_migration_input`] renders one canonical document the PHP
//! adapter consumes to emit a Laravel migration: the pinned engine
//! plan (already gated by the effective destructive ∪ backfill
//! policy), the validated rename history digest, the typed rollback
//! classification of every step, and the exact input digests. The
//! document is closed data — no credentials, hosts, URLs, or
//! timestamps — and byte-identical for value-equal inputs, so two
//! generations of the same transition produce the same migration
//! bytes.

use crate::diagnostics::DiagnosticSet;
use crate::storage_projection::StorageProjectionAttachment;

use super::rollback::{classify, reverse_statement, RollbackClass};
use super::migration::MigrationPlan;
use super::{diagnostic, StorageEngineAttachment};

/// The contract identity this input document speaks.
pub const IDENTITY: &str = "dev.lekalo.laravel-migration-input@0.4.0";
/// The contract schema token this input document speaks.
pub const SCHEMA_VERSION: &str = "lekalo/laravel-migration-input/v0.4.0";

/// One ordered operation of the bounded input: the plan step plus its
/// rollback classification and optional rendered inverse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputOperation {
    /// The 1-based ordinal in the plan's dependency order.
    ordinal: usize,
    /// The closed step kind.
    kind: String,
    /// The deterministic SQL statement.
    statement: String,
    /// The closed data risk key.
    risk: String,
    /// The 1-based ordinals this operation requires.
    requires: Vec<usize>,
    /// The closed rollback class key.
    rollback: RollbackClass,
    /// The rendered inverse statement, when the rollback is
    /// reversible and fully determined.
    inverse: Option<String>,
}

impl InputOperation {
    /// The 1-based ordinal.
    pub const fn ordinal(&self) -> usize {
        self.ordinal
    }

    /// The closed step kind.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The deterministic SQL statement.
    pub fn statement(&self) -> &str {
        &self.statement
    }

    /// The closed data risk key.
    pub fn risk(&self) -> &str {
        &self.risk
    }

    /// The required ordinals.
    pub fn requires(&self) -> &[usize] {
        &self.requires
    }

    /// The closed rollback class.
    pub const fn rollback(&self) -> RollbackClass {
        self.rollback
    }

    /// The rendered inverse statement, when one exists.
    pub fn inverse(&self) -> Option<&str> {
        self.inverse.as_deref()
    }
}

/// The finished bounded input document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaravelMigrationInput {
    project_id: String,
    engine_version: String,
    base_digest: String,
    candidate_digest: String,
    diff_digest: String,
    history_digest: Option<String>,
    plan_id: String,
    gated: bool,
    backfill_gated: bool,
    effective_status: String,
    operations: Vec<InputOperation>,
}

impl LaravelMigrationInput {
    /// The stable project identity.
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// The pinned engine version.
    pub fn engine_version(&self) -> &str {
        &self.engine_version
    }

    /// The exact plan identity this input consumes.
    pub fn plan_id(&self) -> &str {
        &self.plan_id
    }

    /// The effective gate verdict: `ready` only when neither a
    /// destructive nor a backfill obligation exists, `blocked`
    /// otherwise (generation writes zero bytes until confirmed).
    pub fn effective_status(&self) -> &str {
        &self.effective_status
    }

    /// The ordered operations.
    pub fn operations(&self) -> &[InputOperation] {
        &self.operations
    }

    /// The canonical input bytes (compact JSON, byte-sorted keys), or
    /// the typed over-bound refusal.
    pub fn canonical_bytes(&self) -> Result<String, DiagnosticSet> {
        let bytes = self.payload();
        if bytes.len() > super::version::MAX_CANONICAL_BYTES {
            return Err(diagnostic::export_limit_set(bytes.len()));
        }
        Ok(bytes)
    }

    fn payload(&self) -> String {
        use super::canonical::{array, flag, object, string};
        let operations: Vec<String> = self
            .operations
            .iter()
            .map(|operation| {
                let mut members: Vec<(&str, Option<String>)> = vec![
                    ("ordinal", Some(operation.ordinal.to_string())),
                    ("kind", Some(string(&operation.kind))),
                    ("statement", Some(string(&operation.statement))),
                    ("risk", Some(string(&operation.risk))),
                    (
                        "requires",
                        super::migration::optional_usize_array(&operation.requires),
                    ),
                    ("rollback", Some(string(operation.rollback.key()))),
                ];
                if let Some(inverse) = operation.inverse.as_deref() {
                    members.push(("inverse", Some(string(inverse))));
                }
                object(members)
            })
            .collect();
        object(vec![
            ("schemaVersion", Some(string(SCHEMA_VERSION))),
            ("identity", Some(string(IDENTITY))),
            ("engine", Some(string("postgres"))),
            ("engineVersion", Some(string(&self.engine_version))),
            ("projectId", Some(string(&self.project_id))),
            ("baseDigest", Some(string(&self.base_digest))),
            ("candidateDigest", Some(string(&self.candidate_digest))),
            ("diffDigest", Some(string(&self.diff_digest))),
            (
                "historyDigest",
                self.history_digest.as_deref().map(string),
            ),
            ("planId", Some(string(&self.plan_id))),
            ("gated", Some(flag(self.gated))),
            ("backfillGated", Some(flag(self.backfill_gated))),
            ("effectiveStatus", Some(string(&self.effective_status))),
            ("operations", Some(array(&operations))),
        ])
    }
}

/// Build the bounded Laravel migration input from a finished plan and
/// its inputs. The plan's custody status is the effective gate: the
/// caller confirms through the plan API first — this builder never
/// changes a verdict. Pure and read-only; byte-identical for
/// value-equal inputs.
pub fn laravel_migration_input(
    profile: &StorageEngineAttachment,
    base: &StorageProjectionAttachment,
    plan: &MigrationPlan,
) -> Result<LaravelMigrationInput, DiagnosticSet> {
    if plan.project_id() != base.project_id().as_str() {
        return Err(super::diagnostic::rule_invalid(
            super::diagnostic::MIGRATION_INVALID,
            "project-mismatch",
            None,
        ));
    }
    if plan.engine_version() != profile.engine_version().as_str() {
        return Err(super::diagnostic::rule_invalid(
            super::diagnostic::MIGRATION_INVALID,
            "engine-version-mismatch",
            None,
        ));
    }
    let operations = plan
        .steps()
        .iter()
        .map(|step| InputOperation {
            ordinal: step.id(),
            kind: step.kind().to_owned(),
            statement: step.statement().to_owned(),
            risk: step.risk().key().to_owned(),
            requires: step.requires().to_vec(),
            rollback: classify(step),
            inverse: reverse_statement(step),
        })
        .collect();
    Ok(LaravelMigrationInput {
        project_id: plan.project_id().to_owned(),
        engine_version: plan.engine_version().to_owned(),
        base_digest: plan.base_digest().to_owned(),
        candidate_digest: plan.candidate_digest().to_owned(),
        diff_digest: plan.diff_digest().to_owned(),
        history_digest: plan.history_digest().map(|digest| digest.to_owned()),
        plan_id: plan.plan_id().to_owned(),
        gated: plan.gated(),
        backfill_gated: plan.backfill_gated(),
        effective_status: plan.status().key().to_owned(),
        operations,
    })
}

/// The canonical digest of one built input document.
pub fn input_digest(input: &LaravelMigrationInput) -> Result<String, DiagnosticSet> {
    let bytes = input.canonical_bytes()?;
    Ok(format!(
        "sha256:{}",
        crate::digest::sha256_hex(bytes.as_bytes())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_engine::history::StorageRenameMap;
    use crate::storage_engine::{plan_migration, plan_with_history};
    use crate::storage_projection::StorageProjectionAttachment;

    fn load(path: &str) -> StorageProjectionAttachment {
        let bytes = std::fs::read(path).expect("fixture");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        StorageProjectionAttachment::from_value(&value).expect("valid")
    }


    fn base() -> StorageProjectionAttachment {
        load("../../tests/fixtures/storage-projection/valid/planner-storage.json")
    }

    fn additive_candidate() -> StorageProjectionAttachment {
        load("../../tests/fixtures/storage-engine/migration/candidate-additive.json")
    }

    #[test]
    fn the_input_document_is_canonical_and_bounded() {
        let base_attachment = base();
        // The planner verifies the profile's projectionRef against the
        // base digest; build a profile bound to this exact projection.
        let base_digest = format!(
            "sha256:{}",
            crate::digest::sha256_hex(
                base_attachment.canonical_bytes().expect("bytes").as_bytes()
            )
        );
        let mut profile_value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/storage-engine/valid/planner-postgres.json"
        ))
        .expect("fixture");
        profile_value["projectionRef"] = serde_json::Value::String(base_digest.clone());
        let profile =
            crate::storage_engine::StorageEngineAttachment::from_value(&profile_value)
                .expect("valid profile");
        let candidate = additive_candidate();
        let plan = plan_migration(&profile, &base_attachment, &candidate, None).expect("plans");
        let input = laravel_migration_input(&profile, &base_attachment, &plan)
            .expect("input");
        let bytes = input.canonical_bytes().expect("bytes");
        let parsed: serde_json::Value = serde_json::from_str(&bytes).expect("json");
        assert_eq!(parsed["identity"], IDENTITY);
        assert_eq!(parsed["effectiveStatus"], "ready");
        assert!(input.operations().iter().all(|op| op.ordinal() > 0));
        // Determinism: two builds are byte-identical.
        let again =
            laravel_migration_input(&profile, &base_attachment, &plan).expect("input");
        assert_eq!(input, again);
        assert_eq!(bytes, again.canonical_bytes().expect("bytes"));
    }

    #[test]
    fn a_blocked_plan_carries_blocked_effective_status() {
        let base_attachment = base();
        let base_digest = format!(
            "sha256:{}",
            crate::digest::sha256_hex(
                base_attachment.canonical_bytes().expect("bytes").as_bytes()
            )
        );
        let mut profile_value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/storage-engine/valid/planner-postgres.json"
        ))
        .expect("fixture");
        profile_value["projectionRef"] = serde_json::Value::String(base_digest);
        let profile =
            crate::storage_engine::StorageEngineAttachment::from_value(&profile_value)
                .expect("valid profile");
        let destructive =
            load("../../tests/fixtures/storage-engine/migration/candidate-destructive.json");
        let blocked = plan_migration(&profile, &base_attachment, &destructive, None)
            .expect("plans blocked");
        assert_eq!(blocked.status(), crate::storage_engine::PlanStatus::Blocked);
        let input = laravel_migration_input(&profile, &base_attachment, &blocked)
            .expect("input");
        assert_eq!(input.effective_status(), "blocked");
    }

    #[test]
    fn rename_history_flows_into_the_document() {
        let base_attachment =
            load("../../tests/fixtures/storage-engine/migration/base.json");
        let base_digest = format!(
            "sha256:{}",
            crate::digest::sha256_hex(
                base_attachment.canonical_bytes().expect("bytes").as_bytes()
            )
        );
        let mut profile_value: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../tests/fixtures/storage-engine/valid/planner-postgres.json"
        ))
        .expect("fixture");
        profile_value["projectionRef"] = serde_json::Value::String(base_digest);
        let profile =
            crate::storage_engine::StorageEngineAttachment::from_value(&profile_value)
                .expect("valid profile");
        let candidate =
            load("../../tests/fixtures/storage-engine/migration/candidate-backfill.json");
        let history = StorageRenameMap {
            project_id: base_attachment.project_id().as_str().to_owned(),
            base_digest: format!(
                "sha256:{}",
                crate::digest::sha256_hex(
                    base_attachment.canonical_bytes().expect("bytes").as_bytes()
                )
            ),
            candidate_digest: format!(
                "sha256:{}",
                crate::digest::sha256_hex(
                    candidate.canonical_bytes().expect("bytes").as_bytes()
                )
            ),
            renames: Vec::new(),
        };
        let plan = plan_with_history(&profile, &base_attachment, &candidate, &history, None)
            .expect("plans");
        let input =
            laravel_migration_input(&profile, &base_attachment, &plan).expect("input");
        assert!(input.operations().len() > 0);
        assert!(input.canonical_bytes().is_ok());
    }
}
