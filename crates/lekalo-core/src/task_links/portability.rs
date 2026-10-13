//! The portability projection of one task-links attachment (issue #52).
//!
//! [`report`] joins the attachment, resolved target/component profile
//! digests, and the admitted evidence runs into the closed portability
//! shape: `coreSemantics` (unchanged domain facts), `providerAdapters`
//! (object/field mapping, revision ordering, remote dedup), and
//! `targetGuarantees` (unique index, compare-and-swap, transaction,
//! durable outbox). File-origin evidence and native receipt/row
//! declarations have no authenticated execution channel, so execution-
//! dependent core and target rows stay unknown. Provider declaration
//! rows derive their requirements from the declared capabilities. A provider-only change moves
//! adapter and guarantee rows while every core row stays
//! byte-identical; missing remote dedup or target isolation visibly
//! lowers guarantee coverage instead of collapsing into a generic pass.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::wire::Attachment;

/// The requirement state of one row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Requirement {
    /// Required by the pilot.
    Required,
    /// Optional.
    Optional,
}

/// The provided state of one row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provided {
    /// The guarantee is supplied and evidenced.
    Supported,
    /// A subset is supplied.
    Partial,
    /// Not supplied.
    Unsupported,
    /// Unprovable from admitted evidence.
    Unknown,
}

/// The evidence source of one row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceSource {
    /// Declared by the attachment.
    AttachmentDeclaration,
    /// Admitted mock execution evidence.
    MockEvidence,
    /// Qualified native execution evidence.
    NativeEvidence,
}

/// One portability row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    /// The closed fact id.
    pub fact: &'static str,
    /// Requirement.
    pub requirement: Requirement,
    /// Provided state.
    pub provided: Provided,
    /// Exact pin.
    pub pin: String,
    /// Evidence source.
    pub evidence_source: EvidenceSource,
}

/// The full portability report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// The exact portability discriminator.
    pub schema_version: &'static str,
    /// The portability identity.
    pub identity: &'static str,
    /// The attachment revision.
    pub attachment_revision: String,
    /// The bound project id.
    pub project_id: String,
    /// The source profile reference (id + digest).
    pub from_profile: ProfileRef,
    /// The target profile reference (id + digest).
    pub to_profile: ProfileRef,
    /// `strict` or `permissive`.
    pub profile: String,
    /// Core domain rows.
    pub core_semantics: Vec<Row>,
    /// Provider adapter rows.
    pub provider_adapters: Vec<Row>,
    /// Target guarantee rows.
    pub target_guarantees: Vec<Row>,
    /// The evidence coverage summary.
    pub evidence_coverage: EvidenceCoverage,
    /// Transparent owner/input/build/profile/result bindings of live executions.
    /// Exported records cannot be re-imported as execution authority.
    pub execution_provenance: Vec<ExecutionProvenance>,
}

/// Export-only provenance of a live owner execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionProvenance {
    /// The executing owner.
    pub owner: String,
    /// The actual implementation.
    pub implementation: String,
    /// Embedded implementation/toolchain build digest.
    pub build_digest: String,
    /// Actual compiled source digest.
    pub source_digest: String,
    /// Input/recipe/profile binding.
    pub input_digest: String,
    /// Independently asserted result digest.
    pub result_digest: String,
    /// Executed profile identity.
    pub profile_id: String,
    /// Exact selected profile/build digest.
    pub profile_digest: String,
    /// Origin established by execution, never by a file label.
    pub origin: String,
    /// Passed assertions (specific guarantees, not scenario-name inference).
    pub assertions: Vec<String>,
}

/// One profile reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRef {
    /// Profile id.
    pub profile_id: String,
    /// Exact profile digest.
    pub digest: String,
}

/// The evidence coverage block.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCoverage {
    /// Admitted mock evidence ids.
    pub mock_evidence_ids: Vec<String>,
    /// Admitted native evidence ids.
    pub native_evidence_ids: Vec<String>,
    /// Required rows explicitly unsupported.
    pub unsupported_required: u64,
    /// Required rows explicitly unknown.
    pub unknown_required: u64,
}

/// The admitted-evidence view the portability join consumes: the
/// summary and the producer-declared scenario/command labels. Matching
/// pins and coherent labels authenticate no execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedEvidence {
    /// Evidence id (the admitted opaque reference).
    pub evidence_id: String,
    /// Declared origin (`mock`/`reference`/`native`).
    pub origin: String,
    /// The declared scenario the run witnessed.
    pub scenario_id: String,
    /// Owner-attested execution set (empty for file-origin admission).
    /// Legacy callers may fill this field; it does not confer custody.
    pub executed: std::collections::BTreeSet<String>,
}

/// Structurally checked native run/target declarations. Co-located
/// receipts and database rows are caller-writable and are not an
/// authenticated execution channel; they cannot promote guarantees.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeEvidence {
    /// The executed legs the qualification receipt names.
    pub legs: std::collections::BTreeSet<String>,
    /// The exact `sha256:` digest of the qualification receipt bytes.
    pub receipt_digest: String,
    /// The implementation that actually executed the battery.
    pub target_implementation: String,
    /// The build profile the battery executed under.
    pub target_profile: String,
}

/// Build the portability report.
///
/// `from_profile`/`to_profile` are `(id, digest)` pairs of the resolved
/// target/component profiles; `evidence` carries the admitted evidence
/// runs; `scenario_covers` maps every scenario id declared by the
/// compilation to the symbols it covers.
///
/// Declared core facts stay unknown until owner-attested execution
/// provenance is available; absent declarations are unsupported. Native
/// rows/receipt strings and producer-authored successful steps cannot
/// promote execution-dependent guarantees. Provider declaration facts
/// remain explicitly separated from these execution claims.
pub fn report(
    attachment: &Attachment,
    from_profile: ProfileRef,
    to_profile: ProfileRef,
    profile: &str,
    evidence: &[AdmittedEvidence],
    _scenario_covers: &BTreeMap<String, BTreeSet<String>>,
    _native: Option<&NativeEvidence>,
) -> Report {
    report_with_execution(attachment, from_profile, to_profile, profile, evidence, &[])
}

/// Join only sealed process-local execution-owner capabilities. Exported receipts
/// and legacy producer summaries cannot populate this channel.
pub fn report_with_execution(
    attachment: &Attachment,
    from_profile: ProfileRef,
    to_profile: ProfileRef,
    profile: &str,
    evidence: &[AdmittedEvidence],
    execution: &[super::execution::Execution],
) -> Report {
    let revision = attachment.attachment_revision().as_str().to_owned();
    let project_id = attachment.project_id().to_owned();
    let ir_digest = attachment.ir_digest().as_str().to_owned();
    // Resolved declarations drive the core rows: each fact is checked
    // against this attachment's own typed members, so a mutated
    // attachment cannot ride constant "supported" labels.
    let declared_tuple = attachment.unique_constraints().iter().any(|constraint| {
        constraint.entity == "tasklinks.task_external_link"
            && constraint.scope == "workspace-installation-provider-object-external-id"
            && constraint.fields.len() == super::validate::EXTERNAL_IDENTITY_TUPLE.len()
            && constraint
                .fields
                .iter()
                .zip(super::validate::EXTERNAL_IDENTITY_TUPLE.iter())
                .all(|(field, expected)| field == expected)
    });
    let declared_zero_n = attachment.relations().iter().any(|relation| {
        relation.entity == "planner.task"
            && relation.target == "tasklinks.task_external_link"
            && relation.min == 0
    });
    let declared_history = attachment.relations().iter().any(|relation| {
        relation.target == "tasklinks.task_external_link" && relation.on_detach == "retain-history"
    });
    let declared_policies =
        !attachment.field_policies().is_empty() && !attachment.action_policies().is_empty();
    let family_pin = super::version::IDENTITY.to_owned();
    // File-origin summaries, including internally consistent decisions,
    // recomputable receipt digests and co-located SQLite qualification
    // rows, are producer declarations, not an authenticated execution
    // channel. There is currently no owner-attested run/input/build/result
    // receipt in this API. Do not promote execution-dependent guarantees
    // from any of these caller-writable facts (even for the named target).
    let core_row = |declared: bool, fact: &'static str, pin: String| {
        let witnessed = declared
            .then(|| execution.iter().find(|run| run.witnesses(attachment, fact)))
            .flatten();
        Row {
            fact,
            requirement: Requirement::Required,
            provided: if witnessed.is_some() {
                Provided::Supported
            } else if declared {
                Provided::Unknown
            } else {
                Provided::Unsupported
            },
            pin: witnessed.map_or(pin, |run| run.report().result_digest.clone()),
            evidence_source: if witnessed.is_some() {
                EvidenceSource::MockEvidence
            } else {
                EvidenceSource::AttachmentDeclaration
            },
        }
    };
    let core_semantics = vec![
        core_row(
            declared_tuple,
            "external-identity-exact-tuple",
            family_pin.clone(),
        ),
        core_row(declared_zero_n, "task-links-0-n", family_pin.clone()),
        core_row(
            declared_history,
            "detach-preserves-history",
            family_pin.clone(),
        ),
        core_row(declared_policies, "actor-and-field-policy", family_pin),
        core_row(true, "version-conflict-detection", ir_digest.clone()),
        core_row(true, "idempotent-intent", ir_digest),
    ];
    let mut provider_adapters = Vec::new();
    for contract in attachment.provider_contracts() {
        // The requirement derives from the declared capability
        // requirements: a contract carrying any `required` capability
        // is a required adapter surface; optional-only contracts stay
        // optional.
        let requirement = if attachment
            .capability_requirements()
            .iter()
            .any(|capability| {
                capability.contract_id == contract.contract_id
                    && capability.requirement == "required"
            }) {
            Requirement::Required
        } else {
            Requirement::Optional
        };
        provider_adapters.push(Row {
            fact: "object-type-mapping",
            requirement,
            provided: Provided::Supported,
            pin: contract.upstream_pin.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        });
        provider_adapters.push(Row {
            fact: "revision-ordering",
            requirement,
            provided: if contract.revision_ordering == "comparable-total" {
                Provided::Supported
            } else {
                Provided::Partial
            },
            pin: contract.upstream_pin.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        });
        provider_adapters.push(Row {
            fact: "remote-dedup",
            requirement,
            provided: match contract.remote_dedup.as_str() {
                "idempotency-key" | "receipt-reconciliation" => Provided::Supported,
                _ => Provided::Unsupported,
            },
            pin: contract.upstream_pin.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        });
    }
    let rules = attachment.delivery_rules();
    // A battery may genuinely execute in the gate, but a file reader
    // cannot distinguish its rows from an identical caller-authored DB.
    // Target qualification remains unknown until an admitted execution
    // owner supplies provenance; copying a receipt across targets (or
    // inventing matching rows on the same target) proves no guarantee.
    let target_row = |fact: &'static str| Row {
        fact,
        requirement: Requirement::Required,
        provided: Provided::Unknown,
        pin: to_profile.digest.clone(),
        evidence_source: EvidenceSource::AttachmentDeclaration,
    };
    let target_guarantees = vec![
        target_row("unique-index"),
        target_row("compare-and-swap"),
        target_row("local-transaction"),
        target_row("durable-outbox"),
        Row {
            fact: "delivery-uncertainty",
            requirement: Requirement::Required,
            provided: if rules.uncertainty_policy == "receipt-reconciliation"
                || rules.uncertainty_policy == "report-unknown-reconcile"
            {
                Provided::Supported
            } else {
                Provided::Unsupported
            },
            pin: attachment.ir_digest().as_str().to_owned(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        },
    ];
    let unsupported_required = core_semantics
        .iter()
        .chain(provider_adapters.iter())
        .chain(target_guarantees.iter())
        .filter(|row| {
            row.requirement == Requirement::Required && row.provided == Provided::Unsupported
        })
        .count() as u64;
    let unknown_required = core_semantics
        .iter()
        .chain(provider_adapters.iter())
        .chain(target_guarantees.iter())
        .filter(|row| row.requirement == Requirement::Required && row.provided == Provided::Unknown)
        .count() as u64;
    Report {
        schema_version: super::version::PORTABILITY_SCHEMA_VERSION,
        identity: super::version::PORTABILITY_IDENTITY,
        attachment_revision: revision,
        project_id,
        from_profile,
        to_profile,
        profile: profile.to_owned(),
        core_semantics,
        provider_adapters,
        target_guarantees,
        execution_provenance: execution
            .iter()
            .filter(|run| run.witnesses(attachment, "idempotent-intent"))
            .map(|run| {
                let receipt = run.report();
                ExecutionProvenance {
                    owner: receipt.owner.to_owned(),
                    implementation: receipt.implementation.to_owned(),
                    build_digest: receipt.build_digest.clone(),
                    source_digest: receipt.source_digest.clone(),
                    input_digest: receipt.input_digest.clone(),
                    result_digest: receipt.result_digest.clone(),
                    profile_id: receipt.implementation.to_owned(),
                    profile_digest: receipt.build_digest.clone(),
                    origin: receipt.origin.to_owned(),
                    assertions: receipt.assertions.clone(),
                }
            })
            .collect(),
        evidence_coverage: EvidenceCoverage {
            mock_evidence_ids: evidence
                .iter()
                .filter(|run| run.origin != "native")
                .map(|run| run.evidence_id.clone())
                .chain(
                    execution
                        .iter()
                        .filter(|run| run.witnesses(attachment, "idempotent-intent"))
                        .map(|run| run.report().execution_id.clone()),
                )
                .collect(),
            // A producer's origin label does not establish native
            // execution coverage. No attested channel exists here yet.
            native_evidence_ids: Vec::new(),
            unsupported_required,
            unknown_required,
        },
    }
}

/// The admission findings of one portability report under `profile`.
///
/// Strict denies missing/unsupported required semantics: every required
/// row that is not `supported` becomes one registered
/// `tasklink.required-guarantee-unsupported` finding. Permissive
/// reports the same gaps without invalidating: the rows and counts
/// stay visible and the unknown/unsupported reasons are retained.
pub fn findings(report: &Report) -> Vec<super::validate::Finding> {
    let mut out = Vec::new();
    let mut push = |row: &Row| {
        if row.requirement != Requirement::Required {
            return;
        }
        let detail = match row.provided {
            Provided::Unsupported => "required-guarantee-unsupported",
            Provided::Unknown => "required-guarantee-unknown",
            _ => return,
        };
        out.push(super::validate::Finding {
            rule: super::diagnostic::REQUIRED_GUARANTEE_UNSUPPORTED,
            code: "LEK-TASKLINK-007",
            subject: row.fact.to_owned(),
            detail,
        });
    };
    for row in &report.core_semantics {
        push(row);
    }
    for row in &report.provider_adapters {
        push(row);
    }
    for row in &report.target_guarantees {
        push(row);
    }
    out.sort();
    out.dedup();
    out
}
