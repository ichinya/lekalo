//! The portability projection of one task-links attachment (issue #52).
//!
//! [`report`] joins the attachment, resolved target/component profile
//! digests, and the admitted evidence runs into the closed portability
//! shape: `coreSemantics` (unchanged domain facts), `providerAdapters`
//! (object/field mapping, revision ordering, remote dedup), and
//! `targetGuarantees` (unique index, compare-and-swap, transaction,
//! durable outbox). Every claim is bound to the exact facts it
//! witnessed: evidence promoted to a row must have witnessed a
//! declared scenario that covers the fact's own symbols, target
//! guarantees require qualified native evidence of the delivery
//! surface, and provider rows derive their requirement from the
//! declared capability requirements. A provider-only change moves
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
/// summary, the exact scenario the run witnessed, and the commands the
/// run actually executed successfully.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdmittedEvidence {
    /// Evidence id (the admitted opaque reference).
    pub evidence_id: String,
    /// Declared origin (`mock`/`reference`/`native`).
    pub origin: String,
    /// The declared scenario the run witnessed.
    pub scenario_id: String,
    /// The commands of the run's successful steps: a scenario label is
    /// witnessed only when a successful step executed a covered symbol.
    pub executed: std::collections::BTreeSet<String>,
}

/// The verified native qualification the report consumes: the executed
/// battery legs and the exact receipt digest. Target-guarantee rows
/// claim support per executed leg and pin the receipt, never the
/// selected profile file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeEvidence {
    /// The executed legs the qualification receipt names.
    pub legs: std::collections::BTreeSet<String>,
    /// The exact `sha256:` digest of the qualification receipt bytes.
    pub receipt_digest: String,
}

/// Build the portability report.
///
/// `from_profile`/`to_profile` are `(id, digest)` pairs of the resolved
/// target/component profiles; `evidence` carries the admitted evidence
/// runs; `scenario_covers` maps every scenario id declared by the
/// compilation to the symbols it covers.
///
/// Every row is derived, never constant, and every claim is bound to
/// the exact facts it witnessed: a core fact says `supported` only
/// when an admitted evidence run witnessed a declared scenario that
/// covers the fact's own symbols (an unrelated receipt proves
/// nothing), target guarantees require qualified native evidence of
/// the delivery surface, and a fact this attachment does not declare
/// at all is `unsupported` outright. Labels never outrun their
/// evidence.
pub fn report(
    attachment: &Attachment,
    from_profile: ProfileRef,
    to_profile: ProfileRef,
    profile: &str,
    evidence: &[AdmittedEvidence],
    scenario_covers: &BTreeMap<String, BTreeSet<String>>,
    native: Option<&NativeEvidence>,
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
    let declared_zero_n = attachment
        .relations()
        .iter()
        .any(|relation| relation.target == "tasklinks.task_external_link" && relation.min == 0);
    let declared_history = attachment.relations().iter().any(|relation| {
        relation.target == "tasklinks.task_external_link" && relation.on_detach == "retain-history"
    });
    let declared_policies =
        !attachment.field_policies().is_empty() && !attachment.action_policies().is_empty();
    let family_pin = super::version::IDENTITY.to_owned();
    // Evidence is bound to facts: a fact only claims evidence when an
    // admitted run named the declared scenario AND a successful step of
    // that run actually executed a covered symbol. A borrowed scenario
    // label over refused or unrelated steps witnesses nothing.
    let witnessed = |fact_scenarios: &[&str], native: bool| -> bool {
        evidence.iter().any(|run| {
            let origin_matches = if native {
                run.origin == "native"
            } else {
                run.origin == "mock" || run.origin == "reference"
            };
            origin_matches
                && fact_scenarios.iter().any(|required| {
                    scenario_covers
                        .get(&run.scenario_id)
                        .is_some_and(|covered| covered.contains(*required))
                        && run.executed.contains(*required)
                })
        })
    };
    let mock_witnessed = |scenarios: &[&str]| witnessed(scenarios, false);
    let identity_scenarios: &[&str] = &["tasklinks.attach_link", "tasklinks.import_link"];
    let history_scenarios: &[&str] = &["tasklinks.detach_link", "tasklinks.link_detached"];
    let policy_scenarios: &[&str] = &["tasklinks.request_action"];
    let conflict_scenarios: &[&str] = &[
        "tasklinks.ingest_event",
        "tasklinks.conflict_recorded",
        "tasklinks.resolve_conflict",
    ];
    let intent_scenarios: &[&str] = &[
        "tasklinks.dispatch_outbox",
        "tasklinks.ack_delivery",
        "tasklinks.request_action",
    ];
    let core_row = |declared: bool, fact: &'static str, scenarios: &[&str], pin: String| {
        // A declared fact with witnessed evidence says `supported`; a
        // declared fact without any is honestly `unknown` (declaration
        // alone never proves a run); an undeclared fact is
        // `unsupported` outright.
        let seen = mock_witnessed(scenarios);
        let (provided, source) = match (declared, seen) {
            (true, true) => (Provided::Supported, EvidenceSource::MockEvidence),
            (true, false) => (Provided::Unknown, EvidenceSource::AttachmentDeclaration),
            (false, _) => (Provided::Unsupported, EvidenceSource::AttachmentDeclaration),
        };
        Row {
            fact,
            requirement: Requirement::Required,
            provided,
            pin,
            evidence_source: source,
        }
    };
    let core_semantics = vec![
        core_row(
            declared_tuple,
            "external-identity-exact-tuple",
            identity_scenarios,
            family_pin.clone(),
        ),
        core_row(
            declared_zero_n,
            "task-links-0-n",
            identity_scenarios,
            family_pin.clone(),
        ),
        core_row(
            declared_history,
            "detach-preserves-history",
            history_scenarios,
            family_pin.clone(),
        ),
        core_row(
            declared_policies,
            "actor-and-field-policy",
            policy_scenarios,
            family_pin,
        ),
        {
            let seen = mock_witnessed(conflict_scenarios);
            Row {
                fact: "version-conflict-detection",
                requirement: Requirement::Required,
                provided: if seen {
                    Provided::Supported
                } else {
                    Provided::Unknown
                },
                pin: ir_digest.clone(),
                evidence_source: if seen {
                    EvidenceSource::MockEvidence
                } else {
                    EvidenceSource::AttachmentDeclaration
                },
            }
        },
        {
            let seen = mock_witnessed(intent_scenarios);
            Row {
                fact: "idempotent-intent",
                requirement: Requirement::Required,
                provided: if seen {
                    Provided::Supported
                } else {
                    Provided::Unknown
                },
                pin: ir_digest,
                evidence_source: if seen {
                    EvidenceSource::MockEvidence
                } else {
                    EvidenceSource::AttachmentDeclaration
                },
            }
        },
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
    // Target guarantees are target facts: each claims support only from
    // qualified native evidence of the delivery surface AND the exact
    // executed battery leg that proves THAT guarantee, pinning the
    // qualification receipt as provenance. One target-independent
    // scenario label promotes nothing: an unexecuted guarantee stays
    // honestly unknown even when a different native runner passed.
    let native_seen = witnessed(
        &["tasklinks.dispatch_outbox", "tasklinks.ack_delivery"],
        true,
    );
    let leg_ran = |leg: &str| {
        native_seen && native.is_some_and(|qualification| qualification.legs.contains(leg))
    };
    let target_row = |fact: &'static str, leg: Option<&str>, pin: String| {
        let supported = match leg {
            Some(leg) => leg_ran(leg),
            None => false,
        };
        Row {
            fact,
            requirement: Requirement::Required,
            provided: if supported {
                Provided::Supported
            } else {
                Provided::Unknown
            },
            pin,
            evidence_source: if supported {
                EvidenceSource::NativeEvidence
            } else {
                EvidenceSource::AttachmentDeclaration
            },
        }
    };
    let identity_race = "synchronized_identity_race_binds_one_task";
    let receipt_pin = native
        .map(|qualification| qualification.receipt_digest.clone())
        .unwrap_or_else(|| to_profile.digest.clone());
    let target_guarantees = vec![
        // The DB-level identity race executed real competing inserts on
        // separate connections under the database unique index: it is
        // the executed proof of the uniqueness and local-transaction
        // facts, pinned to the exact receipt.
        target_row("unique-index", Some(identity_race), receipt_pin.clone()),
        // The battery executes no CAS-specific leg; the fact stays
        // honestly unknown until one runs.
        target_row("compare-and-swap", None, to_profile.digest.clone()),
        target_row(
            "local-transaction",
            Some(identity_race),
            receipt_pin.clone(),
        ),
        // The durable outbox facts have their own executed legs: the
        // ack-window recovery and the durable attempt bound.
        target_row(
            "durable-outbox",
            Some("accepted_before_ack_survives_restart_as_unknown"),
            receipt_pin,
        ),
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
        evidence_coverage: EvidenceCoverage {
            mock_evidence_ids: evidence
                .iter()
                .filter(|run| run.origin != "native")
                .map(|run| run.evidence_id.clone())
                .collect(),
            native_evidence_ids: evidence
                .iter()
                .filter(|run| run.origin == "native")
                .map(|run| run.evidence_id.clone())
                .collect(),
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
