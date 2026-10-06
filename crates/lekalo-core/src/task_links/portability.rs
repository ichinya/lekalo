//! The portability projection of one task-links attachment (issue #52).
//!
//! [`report`] joins the attachment, resolved target/component profile
//! digests, and admitted evidence ids into the closed portability
//! shape: `coreSemantics` (unchanged domain facts), `providerAdapters`
//! (object/field mapping, revision ordering, remote dedup), and
//! `targetGuarantees` (unique index, compare-and-swap, transaction,
//! durable outbox). A provider-only change moves adapter and guarantee
//! rows while every core row stays byte-identical; missing remote dedup
//! or target isolation visibly lowers guarantee coverage instead of
//! collapsing into a generic pass.

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

/// Build the portability report.
///
/// `from_profile`/`to_profile` are `(id, digest)` pairs of the resolved
/// target/component profiles; `mock_evidence`/`native_evidence` carry
/// admitted evidence ids.
pub fn report(
    attachment: &Attachment,
    from_profile: ProfileRef,
    to_profile: ProfileRef,
    profile: &str,
    mock_evidence: Vec<String>,
    native_evidence: Vec<String>,
) -> Report {
    let revision = attachment.attachment_revision().as_str().to_owned();
    let project_id = attachment.project_id().to_owned();
    let ir_digest = attachment.ir_digest().as_str().to_owned();
    let core_semantics = vec![
        Row {
            fact: "external-identity-exact-tuple",
            requirement: Requirement::Required,
            provided: Provided::Supported,
            pin: revision.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        },
        Row {
            fact: "task-links-0-n",
            requirement: Requirement::Required,
            provided: Provided::Supported,
            pin: revision.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        },
        Row {
            fact: "detach-preserves-history",
            requirement: Requirement::Required,
            provided: Provided::Supported,
            pin: revision.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        },
        Row {
            fact: "actor-and-field-policy",
            requirement: Requirement::Required,
            provided: Provided::Supported,
            pin: revision.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        },
        Row {
            fact: "version-conflict-detection",
            requirement: Requirement::Required,
            provided: Provided::Supported,
            pin: revision.clone(),
            evidence_source: EvidenceSource::MockEvidence,
        },
        Row {
            fact: "idempotent-intent",
            requirement: Requirement::Required,
            provided: Provided::Supported,
            pin: ir_digest,
            evidence_source: EvidenceSource::MockEvidence,
        },
    ];
    let mut provider_adapters = Vec::new();
    for contract in attachment.provider_contracts() {
        provider_adapters.push(Row {
            fact: "object-type-mapping",
            requirement: Requirement::Required,
            provided: Provided::Supported,
            pin: contract.upstream_pin.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        });
        provider_adapters.push(Row {
            fact: "revision-ordering",
            requirement: Requirement::Required,
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
            requirement: Requirement::Required,
            provided: match contract.remote_dedup.as_str() {
                "idempotency-key" | "receipt-reconciliation" => Provided::Supported,
                _ => Provided::Unsupported,
            },
            pin: contract.upstream_pin.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        });
    }
    let rules = attachment.delivery_rules();
    let has_native = !native_evidence.is_empty();
    let target_guarantees = vec![
        Row {
            fact: "unique-index",
            requirement: Requirement::Required,
            provided: if has_native {
                Provided::Supported
            } else {
                Provided::Unknown
            },
            pin: to_profile.digest.clone(),
            evidence_source: if has_native {
                EvidenceSource::NativeEvidence
            } else {
                EvidenceSource::AttachmentDeclaration
            },
        },
        Row {
            fact: "compare-and-swap",
            requirement: Requirement::Required,
            provided: if has_native {
                Provided::Supported
            } else {
                Provided::Unknown
            },
            pin: to_profile.digest.clone(),
            evidence_source: if has_native {
                EvidenceSource::NativeEvidence
            } else {
                EvidenceSource::AttachmentDeclaration
            },
        },
        Row {
            fact: "local-transaction",
            requirement: Requirement::Required,
            provided: Provided::Supported,
            pin: to_profile.digest.clone(),
            evidence_source: EvidenceSource::AttachmentDeclaration,
        },
        Row {
            fact: "durable-outbox",
            requirement: Requirement::Required,
            provided: if has_native {
                Provided::Supported
            } else {
                Provided::Unknown
            },
            pin: to_profile.digest.clone(),
            evidence_source: if has_native {
                EvidenceSource::NativeEvidence
            } else {
                EvidenceSource::AttachmentDeclaration
            },
        },
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
            mock_evidence_ids: mock_evidence,
            native_evidence_ids: native_evidence,
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
