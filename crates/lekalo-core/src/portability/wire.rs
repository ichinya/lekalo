//! The closed portability wire: typed report rows, request documents,
//! and duplicate-key-safe decoding (issue #19).
//!
//! Every structure is closed (`deny_unknown_fields`), classification
//! states are closed tagged unions, and ingestion refuses duplicate JSON
//! keys before typed decoding. The report shape mirrors
//! `contracts/portability-report.schema.v0.8.0.json` exactly; the
//! request documents mirror the targets/evidence/recipe contracts.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::diagnostic;
use super::version;
use crate::diagnostics::DiagnosticSet;

/// Whether one exact `sha256:<64 lowercase hex>` digest spelling.
pub fn is_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

// The closed grammars of the published portability contracts. The typed
// decoders enforce every pattern, enum, bound, and set rule the Draft
// 2020-12 schemas state, so a schema-invalid document can never reach
// semantic evaluation and an emitted report can never violate its own
// schema (decoder/schema parity in both directions).

/// The closed `semanticId` grammar: 1..192 characters, starting with an
/// alphanumeric, then alphanumerics and `. _ : -`.
pub(crate) fn is_semantic_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 192
        && value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

/// The closed dotted `capabilityId` grammar: dot-separated lowercase
/// components, each starting with a letter.
pub(crate) fn is_capability_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.split('.').all(|part| {
            part.starts_with(|c: char| c.is_ascii_lowercase())
                && !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
        && value.contains('.')
}

/// The closed target-id grammar: lowercase start, then lowercase,
/// digits, and `-`.
pub(crate) fn is_target_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The exact semver grammar `major.minor.patch`, at most 32 characters.
pub(crate) fn is_semver(value: &str) -> bool {
    value.len() <= 32 && {
        let parts: Vec<&str> = value.split('.').collect();
        parts.len() == 3
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    }
}

/// The closed adapter identity/protocol grammar: alphanumeric start,
/// then alphanumerics and `. @ / _ -`.
pub(crate) fn is_adapter_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '@' | '/' | '_' | '-'))
}

/// The closed single-token grammar (`^[a-z][a-z0-9-]*$`).
pub(crate) fn is_lower_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.starts_with(|c: char| c.is_ascii_lowercase())
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Whether one policy value satisfies the closed policy grammar: enum
/// kind, bounded token value, and the exact numeric bounds.
pub(crate) fn valid_policy(policy: &Policy) -> bool {
    if !matches!(
        policy.kind.as_str(),
        "decimal" | "isolation" | "ordering" | "null-handling" | "rounding"
    ) {
        return false;
    }
    if policy.value.is_empty()
        || policy.value.len() > 64
        || !policy
            .value
            .starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        || !policy
            .value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return false;
    }
    if let Some(precision) = policy.precision {
        if !(1..=1000).contains(&precision) {
            return false;
        }
    }
    if let Some(scale) = policy.scale {
        if !(0..=1000).contains(&scale) {
            return false;
        }
    }
    true
}

/// The closed recorded-context revision grammar: 40 or 64 lowercase hex.
pub(crate) fn is_revision(value: &str) -> bool {
    (value.len() == 40 || value.len() == 64)
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The closed recorded-context note grammar.
pub(crate) fn is_context_note(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, ' ' | '.' | ',' | ':' | ';' | '/' | '(' | ')' | '_' | '-')
        })
}

/// Refuse an explicit JSON `null` for a member whose type forbids it:
/// serde's `Option` silently accepts `null` as `None`, but the closed
/// schemas distinguish null from absent for non-nullable members.
pub(crate) fn refuse_null_member(
    value: &serde_json::Value,
    member: &str,
) -> Result<(), DiagnosticSet> {
    if value.get(member).map(|v| v.is_null()) == Some(true) {
        return Err(diagnostic::invalid("closed-shape"));
    }
    Ok(())
}

/// The closed four-dimension assessment vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Dimension {
    Structural,
    Behavioral,
    Storage,
    Operational,
}

impl Dimension {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Structural => "structural",
            Self::Behavioral => "behavioral",
            Self::Storage => "storage",
            Self::Operational => "operational",
        }
    }

    /// Every dimension in stable wire order.
    pub const ALL: [Dimension; 4] = [
        Dimension::Structural,
        Dimension::Behavioral,
        Dimension::Storage,
        Dimension::Operational,
    ];

    /// Parse the stable lowercase wire spelling.
    pub fn parse_name(name: &str) -> Option<Self> {
        match name {
            "structural" => Some(Self::Structural),
            "behavioral" => Some(Self::Behavioral),
            "storage" => Some(Self::Storage),
            "operational" => Some(Self::Operational),
            _ => None,
        }
    }
}

/// The closed subject-state union. The states never collapse into one
/// another: unsupported is an explicit verdict, unknown is an explicit
/// evidence gap, and `supported-with-mapping` is never a native
/// equivalence claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Classification {
    Portable,
    SupportedWithMapping,
    Partial,
    TargetSpecific,
    Unsupported,
    Unknown,
    NonApplicable,
}

impl Classification {
    /// The stable kebab-case wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Portable => "portable",
            Self::SupportedWithMapping => "supported-with-mapping",
            Self::Partial => "partial",
            Self::TargetSpecific => "target-specific",
            Self::Unsupported => "unsupported",
            Self::Unknown => "unknown",
            Self::NonApplicable => "non-applicable",
        }
    }

    /// The recipe severity rank used for the primary-class precedence:
    /// lower wins when one obligation admits several readings.
    pub const fn rank(self) -> u8 {
        match self {
            Self::Unsupported => 0,
            Self::Partial => 1,
            Self::Unknown => 2,
            Self::TargetSpecific => 3,
            Self::SupportedWithMapping => 4,
            Self::Portable => 5,
            Self::NonApplicable => 6,
        }
    }
}

/// The orthogonal verification state: how the classification is known,
/// never what the classification is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verification {
    Untested,
    RecordedPass,
    RecordedFail,
    NotApplicable,
}

impl Verification {
    /// The stable kebab-case wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Untested => "untested",
            Self::RecordedPass => "recorded-pass",
            Self::RecordedFail => "recorded-fail",
            Self::NotApplicable => "not-applicable",
        }
    }
}

/// The closed implementation-binding vocabulary of the validated
/// implementation projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Binding {
    Generated,
    Custom,
    Foreign,
    External,
    Unsupported,
    Missing,
}

impl Binding {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generated => "generated",
            Self::Custom => "custom",
            Self::Foreign => "foreign",
            Self::External => "external",
            Self::Unsupported => "unsupported",
            Self::Missing => "missing",
        }
    }
}

/// The closed advisory planning action vocabulary. A row never carries a
/// script, command, time quote, or code edit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    ImplementHook,
    ReplaceMapping,
    ReviewStoragePolicy,
    BindTarget,
    VerifyScenario,
    CollectEvidence,
}

impl Action {
    /// The stable kebab-case wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ImplementHook => "implement-hook",
            Self::ReplaceMapping => "replace-mapping",
            Self::ReviewStoragePolicy => "review-storage-policy",
            Self::BindTarget => "bind-target",
            Self::VerifyScenario => "verify-scenario",
            Self::CollectEvidence => "collect-evidence",
        }
    }
}

/// The closed scenario/gate join-state vocabulary. A recorded failure is
/// its own state: a failing receipt is material planning information and
/// never collapses into `planned`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JoinState {
    Declared,
    Planned,
    Executed,
    Failed,
    Unknown,
}

impl JoinState {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Planned => "planned",
            Self::Executed => "executed",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
}

/// The closed pin-state vocabulary of a target selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PinState {
    /// Exact profile pins verified against the resolved document.
    Pinned,
    /// Derived from the Model declarations without exact profile pins.
    Derived,
    /// The exact pins are unavailable.
    Unknown,
}

impl PinState {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pinned => "pinned",
            Self::Derived => "derived",
            Self::Unknown => "unknown",
        }
    }
}

/// The closed adapter-availability vocabulary of a destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterState {
    /// No adapter snapshot exists; execution evidence is unknown.
    Absent,
    /// A recorded, digest-pinned capability snapshot exists.
    Recorded,
}

/// The fixed work bounds of one assessment, carried from the recipe.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub max_subjects: u64,
    pub max_traversal_depth: u64,
    pub max_dependency_edges: u64,
    pub max_obligation_rows: u64,
    pub max_report_bytes: u64,
    pub max_work_units: u64,
}

/// The integer ratio published with its explicit basis; a zero
/// denominator is unknown, never 100%.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Ratio {
    pub numerator: u64,
    pub denominator: u64,
}

/// One exact semantic policy requirement or offering.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Policy {
    pub kind: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<u64>,
}

/// The recipe reference the report binds to.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeRef {
    pub id: String,
    pub version: String,
    pub digest: String,
}

/// The exact project pin of a report or evidence document.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRef {
    pub project_id: String,
    pub model_version: String,
    pub ir_digest: String,
}

/// The assessment scope: the whole project, one symbol, or one module.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScopeKind {
    Project,
    Symbol,
    Module,
}

impl ScopeKind {
    /// The stable lowercase wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Symbol => "symbol",
            Self::Module => "module",
        }
    }
}

/// One exact resolved-profile pin: declaration and effective resolution
/// plus every component definition version and composed capability.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfilePin {
    pub id: String,
    pub version: String,
    pub source_digest: String,
    pub resolved_digest: String,
    pub component_versions: Vec<ProfileComponent>,
    pub capabilities: Vec<ProfileCapability>,
}

/// One pinned component axis of a resolved profile.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileComponent {
    pub axis: String,
    pub component: String,
    pub definition_version: String,
}

/// One composed capability of a resolved profile at its weakest support.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileCapability {
    pub id: String,
    pub support: String,
}

/// The recorded adapter snapshot of one destination, or `None` when the
/// manifest declares no qualified snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterSnapshot {
    pub state: AdapterState,
    pub identity: Option<String>,
    pub version: Option<String>,
    pub protocol: Option<String>,
    pub capability_digest: Option<String>,
    /// The exact digest of the adapter evidence artifact read through
    /// the confined project filesystem, when the target binding named
    /// one; `None` keeps the provenance explicitly unbound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_digest: Option<String>,
}

/// One assessed destination target with its exact pins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Destination {
    pub target_id: String,
    pub state: PinState,
    pub profile: Option<ProfilePin>,
    pub adapter: Option<AdapterSnapshot>,
    pub storage_profile_refs: Vec<String>,
}

/// The assessed source target.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceSelection {
    pub target_id: String,
    pub state: PinState,
    pub profile: Option<ProfilePin>,
}

/// The exact targets-configuration input pin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetsInput {
    pub state: PinState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// The evidence input pin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceInput {
    pub state: EvidencePresence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// The closed evidence-presence vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidencePresence {
    Supplied,
    Absent,
}

/// Refuse a required-but-nullable member that was omitted entirely: the
/// closed schemas require key presence even where the value may be
/// `null`, and serde's implicit `Option` default would silently accept
/// the absent key and diverge from the contract.
pub(crate) fn require_members(
    value: &serde_json::Value,
    members: &[&str],
) -> Result<(), DiagnosticSet> {
    let Some(object) = value.as_object() else {
        // A missing or non-object container is a typed decode refusal.
        return Ok(());
    };
    for member in members {
        if !object.contains_key(*member) {
            return Err(diagnostic::invalid("closed-shape"));
        }
    }
    Ok(())
}

/// The lock-custody input pin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LockInput {
    pub state: LockPresence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// The closed lock-presence vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LockPresence {
    Known,
    Absent,
}

/// The recorded input pins of one report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Inputs {
    pub targets_config: TargetsInput,
    pub evidence: EvidenceInput,
    pub lock_digest: LockInput,
}

/// The bounded coverage statement of one report.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Coverage {
    pub eligible_subjects: u64,
    pub obligation_rows: u64,
    pub non_applicable_rows: u64,
    pub complete: bool,
    pub truncated: bool,
    pub limits: Limits,
}

/// One obligation row: one subject, one destination, one dimension.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObligationRow {
    pub destination: String,
    pub dimension: Dimension,
    pub classification: Classification,
    pub verification: Verification,
    pub binding: Option<Binding>,
    pub blocking: bool,
    pub reason: String,
    pub required: Option<Policy>,
    pub offered: Option<Policy>,
    pub evidence_refs: Vec<String>,
    pub witnesses: Vec<String>,
}

/// One selected subject with its typed identity and obligation rows.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubjectRow {
    /// The typed `kind:id` semantic identity.
    pub subject: String,
    /// The typed member (field name) when the row is a member obligation.
    pub member: Option<String>,
    pub requirements: Vec<String>,
    pub rows: Vec<ObligationRow>,
}

/// The per-destination verification counts of one dimension.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationCounts {
    pub untested: u64,
    pub recorded_pass: u64,
    pub recorded_fail: u64,
    pub not_applicable: u64,
}

/// One dimension's mutually exclusive primary-class partition plus the
/// published structural ratio.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DimensionSummary {
    pub portable: u64,
    pub supported_with_mapping: u64,
    pub partial: u64,
    pub target_specific: u64,
    pub unsupported: u64,
    pub unknown: u64,
    pub non_applicable: u64,
    pub eligible_applicable: u64,
    /// The recipe ratio (`portable/eligibleApplicableSubjects`) for the
    /// structural dimension; `None` elsewhere and for zero denominators.
    pub structural_portable: Option<Ratio>,
    pub verification: VerificationCounts,
}

impl DimensionSummary {
    /// The empty partition.
    pub const fn empty() -> Self {
        Self {
            portable: 0,
            supported_with_mapping: 0,
            partial: 0,
            target_specific: 0,
            unsupported: 0,
            unknown: 0,
            non_applicable: 0,
            eligible_applicable: 0,
            structural_portable: None,
            verification: VerificationCounts {
                untested: 0,
                recorded_pass: 0,
                recorded_fail: 0,
                not_applicable: 0,
            },
        }
    }
}

/// One destination's four dimension summaries and unresolved totals.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Summary {
    pub destination: String,
    pub dimensions: BTreeMap<Dimension, DimensionSummary>,
    pub blockers: u64,
    pub unknowns: u64,
    pub custom_obligations: u64,
}

/// One advisory migration-planning row: a stable ID linking subject,
/// dimension, reason, action, prerequisites, and affected joins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningRow {
    pub id: String,
    pub subject: String,
    pub member: Option<String>,
    pub dimension: Dimension,
    pub destination: Option<String>,
    pub action: Action,
    pub reason: String,
    pub requirement: Option<String>,
    pub prerequisites: Vec<String>,
    pub scenarios: Vec<String>,
    pub gates: Vec<String>,
    pub evidence_needed: Vec<String>,
}

/// The estimated migration surface: reconciled counts plus the closed
/// machine-readable planning rows.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MigrationSurface {
    pub missing_implementations: u64,
    pub mapped_symbols: u64,
    pub storage_reviews: u64,
    pub affected_scenarios: u64,
    pub affected_gates: u64,
    pub unresolved_obligations: u64,
    pub complete: bool,
    pub rows: Vec<PlanningRow>,
}

/// One affected scenario with the subjects that reach it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AffectedScenario {
    pub scenario: String,
    pub state: JoinState,
    pub subjects: Vec<String>,
}

/// One affected gate with the scenarios that reach it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AffectedGate {
    pub gate: String,
    pub state: JoinState,
    pub scenarios: Vec<String>,
}

/// One bounded report reason record joining a finding to its registered
/// diagnostic code; the envelope diagnostics stay authoritative.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReportDiagnostic {
    pub code: String,
    pub subject: Option<String>,
    pub dimension: Option<Dimension>,
    pub detail: String,
}

/// The closed portability report (`lekalo/portability-report/v0.8.0`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Report {
    pub schema_version: String,
    pub identity: String,
    pub recipe_ref: RecipeRef,
    pub project_ref: ProjectRef,
    pub scope: Scope,
    pub source: SourceSelection,
    pub destinations: Vec<Destination>,
    pub inputs: Inputs,
    pub coverage: Coverage,
    pub subjects: Vec<SubjectRow>,
    pub summaries: Vec<Summary>,
    pub migration_surface: MigrationSurface,
    pub affected_scenarios: Vec<AffectedScenario>,
    pub affected_gates: Vec<AffectedGate>,
    pub diagnostics: Vec<ReportDiagnostic>,
}

/// The report scope member.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Scope {
    pub kind: ScopeKind,
    pub selector: Option<String>,
}

/// The pinned target binding of the configuration document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetBinding {
    pub target_id: String,
    pub profile_document_ref: String,
    pub profile_ref: ProfileRef,
    pub adapter_evidence_ref: Option<String>,
    pub storage_profile_refs: Vec<String>,
}

/// The exact profile pin of a configuration binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileRef {
    pub id: String,
    pub version: String,
    pub source_digest: String,
    pub resolved_digest: String,
}

/// The closed targets configuration (`lekalo/portability-targets/v0.8.0`).
///
/// `default_destinations` mirrors the contract exactly: the member is
/// optional, but when present it admits at least one entry — an empty
/// list is a decode refusal, never a silent fallback.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TargetsDocument {
    pub schema_version: String,
    pub identity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_destinations: Option<Vec<String>>,
    pub targets: Vec<TargetBinding>,
}

/// The explicit recorded evaluation context of an evidence manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordedContext {
    pub state: ContextState,
    pub revision: Option<String>,
    pub note: Option<String>,
}

/// The closed recorded-context vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextState {
    Recorded,
    Absent,
}

/// One declared evidence family with its completeness statement.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FamilyRecord {
    pub family: EvidenceFamily,
    pub complete: bool,
    pub artifact_ref: Option<String>,
    pub digest: Option<String>,
}

/// The closed evidence-family vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceFamily {
    Implementation,
    Storage,
    ExtendedEffects,
    Scenario,
    Trace,
}

impl EvidenceFamily {
    /// The stable kebab-case wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Implementation => "implementation",
            Self::Storage => "storage",
            Self::ExtendedEffects => "extended-effects",
            Self::Scenario => "scenario",
            Self::Trace => "trace",
        }
    }
}

/// One recorded adapter snapshot of the manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdapterRecord {
    pub target_id: String,
    pub state: AdapterState,
    pub identity: Option<String>,
    pub version: Option<String>,
    pub protocol: Option<String>,
    pub capability_digest: Option<String>,
}

/// One reviewed semantic mapping record. A mapping is destination-scoped:
/// it credits exactly the named destination target and is invisible to
/// every other assessment destination.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MappingRecord {
    pub id: String,
    pub requirement: String,
    pub capability: String,
    pub target_id: String,
    pub subject: Option<String>,
    pub coverage: MappingCoverage,
    pub conditions: Vec<String>,
    pub definition_pins: Vec<DefinitionPin>,
}

/// The closed mapping-coverage vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MappingCoverage {
    Full,
    Partial,
}

/// One exact definition-version pin inside a mapping.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DefinitionPin {
    pub id: String,
    pub version: String,
}

/// One required storage policy of the manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageRequirement {
    pub subject: String,
    pub policy: Policy,
}

/// One offered storage policy of the manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageOffering {
    pub target_id: String,
    pub policy: Policy,
}

/// One qualified operational requirement of the manifest. A bridged
/// requirement is admitted only with the exact reviewed precondition set
/// for its destination and an agreeing definition pin, exactly like a
/// behavioral mapping record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationalRequirement {
    pub subject: String,
    pub requirement: String,
    pub conditions: Vec<String>,
    pub definition_pins: Vec<DefinitionPin>,
}

/// One recorded scenario receipt of the manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioRecord {
    pub scenario: String,
    pub target_id: Option<String>,
    pub state: ScenarioReceipt,
    pub receipt_ref: Option<String>,
    /// The exact digest of the referenced receipt artifact; a claimed
    /// execution result is digest-bound custody, never an authored enum.
    pub receipt_digest: Option<String>,
}

/// The closed scenario-receipt vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ScenarioReceipt {
    Untested,
    RecordedPass,
    RecordedFail,
}

/// One declared or recorded gate of the manifest.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GateRecord {
    pub gate: String,
    pub scenarios: Vec<String>,
    pub state: JoinState,
}

/// One qualified observed implicit hook: an inferred status with an
/// explicit proof gap, never a canonical Model member.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HookObservation {
    pub subject: String,
    pub hook: String,
    pub state: HookObservationState,
    pub target_id: String,
}

/// The closed observed-hook vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HookObservationState {
    ObservedSupported,
    ObservedUnsupported,
}

/// The closed evidence manifest (`lekalo/portability-evidence/v0.8.0`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceDocument {
    pub schema_version: String,
    pub identity: String,
    pub project_ref: ProjectRef,
    pub recipe_ref: RecipeRef,
    pub recorded_context: RecordedContext,
    pub families: Vec<FamilyRecord>,
    pub adapters: Vec<AdapterRecord>,
    pub mappings: Vec<MappingRecord>,
    pub storage_required: Vec<StorageRequirement>,
    pub storage_offered: Vec<StorageOffering>,
    pub operational_requirements: Vec<OperationalRequirement>,
    pub scenarios: Vec<ScenarioRecord>,
    pub gates: Vec<GateRecord>,
    pub hooks: Vec<HookObservation>,
}

/// The reviewed recipe document (`lekalo/portability-recipe/v0.8.0`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeDocument {
    pub schema_version: String,
    pub identity: String,
    pub dimensions: BTreeMap<String, DimensionRecipe>,
    pub excluded_from_denominator: Vec<String>,
    pub limits: Limits,
    pub classification: ClassificationRecipe,
    pub semantic_mappings: Vec<RecipeMapping>,
}

/// One dimension assignment of the recipe.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DimensionRecipe {
    pub definition_kinds: Vec<String>,
    pub decision: String,
}

/// The aggregation and precedence definitions of the recipe.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassificationRecipe {
    pub precedence: Vec<String>,
    pub denominator_basis: String,
    pub ratio_basis: RatioBasis,
    pub rounding: String,
}

/// The published ratio basis of the recipe.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RatioBasis {
    pub dimension: String,
    pub formula: String,
}

/// One reviewed semantic mapping definition of the recipe. The optional
/// `destinationPreconditions` map scopes the reviewed precondition set
/// per destination target: a mapping record on a keyed destination must
/// declare exactly that set, so a precondition that names another
/// target's runtime can never admit a mapping here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecipeMapping {
    pub requirement: String,
    pub capability: String,
    pub definition_version: String,
    pub preconditions: Vec<String>,
    #[serde(default)]
    pub destination_preconditions: BTreeMap<String, Vec<String>>,
}

impl RecipeMapping {
    /// The exact reviewed precondition set of one destination: the
    /// destination-scoped list when keyed, the global list otherwise.
    pub fn preconditions_for(&self, destination: &str) -> &[String] {
        match self.destination_preconditions.get(destination) {
            Some(conditions) => conditions,
            None => &self.preconditions,
        }
    }
}

/// The decoded recipe with its exact instance digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recipe {
    pub document: RecipeDocument,
    pub digest: String,
}

/// Recursive duplicate-key refusal before typed decoding; a plain
/// `serde_json::Value` would silently discard the duplicate evidence.
pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DiagnosticSet> {
    let value = decode_dup_safe(bytes)?;
    serde_json::from_value(value).map_err(|_| super::diagnostic::invalid("closed-shape"))
}

/// Decode one generic JSON value with the same recursive duplicate-key
/// refusal and byte bound, for artifact families that continue through a
/// `serde_json::Value` bridge (the implementation attachment). A plain
/// `serde_json::from_slice` there would silently collapse duplicate keys.
pub fn decode_dup_safe(bytes: &[u8]) -> Result<serde_json::Value, DiagnosticSet> {
    if bytes.len() > version::MAX_INPUT_BYTES {
        return Err(super::diagnostic::invalid("input-size"));
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let value = Unique::deserialize(&mut de)
        .map_err(|error| {
            // The duplicate-key refusal keeps its named token: a silently
            // collapsed duplicate would be a contract lie of its own.
            if error.to_string().contains("duplicate-key") {
                super::diagnostic::invalid("duplicate-key")
            } else {
                super::diagnostic::invalid("json-shape")
            }
        })?
        .0;
    de.end()
        .map_err(|_| super::diagnostic::invalid("json-trailing"))?;
    Ok(value)
}

struct Unique(serde_json::Value);

impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique-key JSON")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(serde_json::Value::Null))
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|v| Unique(v.into()))
                    .ok_or_else(|| E::custom("number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(v) = a.next_element::<Unique>()? {
                    values.push(v.0);
                }
                Ok(Unique(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if values.contains_key(&k) {
                        return Err(serde::de::Error::custom("duplicate-key"));
                    }
                    values.insert(k, a.next_value::<Unique>()?.0);
                }
                Ok(Unique(values.into()))
            }
        }
        d.deserialize_any(V)
    }
}
