//! The closed CI report model (issue #103).
//!
//! One [`CiReport`] binds the exact provenance pins, the underlying
//! command result, the policy evaluation, every check and suite row, and
//! the source diagnostics of one headless CI run. The wire shape is the
//! closed `contracts/ci-report.schema.v0.6.3.json` contract; serialization
//! is deterministic (byte-sorted object keys via sorted BTreeMaps, fixed
//! field order through the typed model, compact JSON, one trailing LF).

use std::collections::BTreeMap;

use serde::Serialize;

use crate::diagnostics::Diagnostic;
use crate::result::Status;

pub use super::version::{IDENTITY, MAX_DIAGNOSTICS, REPORT_VERSION, SCHEMA_VERSION};

/// The value-state wrapper of one provenance leaf (issue #121
/// precedent): `known` carries the complete pin, `unknown` carries only
/// a closed reason and never a fabricated value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum ValueState {
    /// The exact pin was read and validated.
    Known {
        /// The exact wire value (digest, revision, version, or flag).
        value: KnownValue,
    },
    /// The pin could not be read; the reason is closed and no value is
    /// invented.
    Unknown {
        /// Why the pin is unavailable.
        reason: UnknownReason,
    },
}

impl ValueState {
    /// A known SHA-256 digest pin.
    pub fn known_digest(digest: impl std::fmt::Display) -> Self {
        Self::Known {
            value: KnownValue::Text(digest.to_string()),
        }
    }

    /// A known lowercase-hex revision pin (40 or 64 hex).
    pub fn known_revision(revision: impl Into<String>) -> Self {
        Self::Known {
            value: KnownValue::Revision(revision.into()),
        }
    }

    /// A known version pin.
    pub fn known_version(version: impl std::fmt::Display) -> Self {
        Self::Known {
            value: KnownValue::Version(version.to_string()),
        }
    }

    /// A known boolean pin.
    pub fn known_flag(flag: bool) -> Self {
        Self::Known {
            value: KnownValue::Flag(flag),
        }
    }

    /// An unknown pin with its closed reason.
    pub fn unknown(reason: UnknownReason) -> Self {
        Self::Unknown { reason }
    }
}

/// One exact provenance wire value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(untagged)]
pub enum KnownValue {
    /// A 40/64 lowercase-hex Git revision.
    Revision(String),
    /// A lowercase semver spelling.
    Version(String),
    /// A boolean fact (the dirty flag).
    Flag(bool),
    /// A `sha256:<64 hex>` digest spelling.
    Text(String),
}

/// Why one provenance leaf is unknown; closed and path-free.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnknownReason {
    /// Git is not a repository at this root.
    NotARepository,
    /// Git is installed but the invocation failed or timed out.
    GitUnavailable,
    /// The file exists but could not be read within bounds.
    Unreadable,
    /// The bytes exist but violate their contract.
    Invalid,
    /// The file does not exist (a missing lock, for example).
    Absent,
    /// The pin has no meaning for this invocation shape.
    NotApplicable,
}

impl UnknownReason {
    /// The stable kebab-case wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotARepository => "not-a-repository",
            Self::GitUnavailable => "git-unavailable",
            Self::Unreadable => "unreadable",
            Self::Invalid => "invalid",
            Self::Absent => "absent",
            Self::NotApplicable => "not-applicable",
        }
    }
}

/// The Git provenance block: the exact commit, the dirty flag, and the
/// working-set digest of the sorted relative-path/content inventory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GitProvenance {
    /// The resolved HEAD revision.
    pub commit: ValueState,
    /// Whether the working tree carries modifications.
    pub dirty: ValueState,
    /// The digest over the sorted relative path/content-digest inventory.
    #[serde(rename = "workingSetDigest")]
    pub working_set_digest: ValueState,
}

/// One exact version/digest pair (model, IR, or lock).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InputProvenance {
    /// The exact contract version.
    pub version: ValueState,
    /// The exact payload digest.
    pub digest: ValueState,
}

/// One resolved profile pin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProfileProvenance {
    /// The stable profile id (`validation-profile.default`).
    pub id: String,
    /// The exact profile contract version.
    pub version: ValueState,
    /// The exact effective profile digest.
    pub digest: ValueState,
}

/// One locked adapter pin.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdapterProvenance {
    /// The stable adapter id.
    pub id: String,
    /// The exact adapter version.
    pub version: ValueState,
    /// The exact package digest.
    pub digest: ValueState,
}

/// The full provenance block of one report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Provenance {
    /// The Git facts.
    pub git: GitProvenance,
    /// The canonical model pin.
    pub model: InputProvenance,
    /// The typed IR pin.
    pub ir: InputProvenance,
    /// The committed lock pin.
    pub lock: InputProvenance,
    /// Every effective profile pin, in id order.
    pub profiles: Vec<ProfileProvenance>,
    /// Every locked adapter pin, in id order.
    pub adapters: Vec<AdapterProvenance>,
}

/// The producer block.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Producer {
    /// The producer id.
    pub id: &'static str,
    /// The exact product version.
    pub version: &'static str,
}

/// The invocation shape of one run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Invocation {
    /// The closed command name.
    pub command: CommandName,
    /// The closed mode token (`full`, `changed`, `default`, `strict`,
    /// `check`, or the readiness phase).
    pub mode: String,
    /// The selected targets, sorted.
    pub targets: Vec<String>,
    /// The selected modules, sorted.
    pub modules: Vec<String>,
    /// Whether the run demanded the full locked inventory.
    pub locked: bool,
    /// The explicit evaluated policy date, when a policy was evaluated.
    #[serde(rename = "asOf", skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
}

/// The closed command vocabulary of the CI surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandName {
    /// `lekalo validate`.
    Validate,
    /// `lekalo generate --check`.
    GenerateCheck,
    /// `lekalo verify`.
    Verify,
    /// `lekalo readiness --phase PHASE`.
    Readiness,
}

impl CommandName {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validate => "validate",
            Self::GenerateCheck => "generate-check",
            Self::Verify => "verify",
            Self::Readiness => "readiness",
        }
    }
}

/// One outcome projection: the domain status plus its stable exit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Outcome {
    /// The domain status spelling.
    pub status: &'static str,
    /// The stable process exit of that status.
    #[serde(rename = "exitCode")]
    pub exit_code: u8,
}

impl Outcome {
    /// Project one domain status.
    pub fn of(status: Status) -> Self {
        Self {
            status: status.as_str(),
            exit_code: status.exit_code(),
        }
    }
}

/// The verdict of one gated run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Every required check passed and no degradation remains.
    Ready,
    /// Optional/declared absences or findings remain visible.
    Degraded,
    /// A required check failed or never reached a terminal evaluation.
    Blocked,
}

impl Verdict {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Blocked => "blocked",
        }
    }
}

/// The authoritative gated outcome of one run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Evaluation {
    /// The effective status the producing CLI must exit with.
    pub status: &'static str,
    /// The effective exit code.
    #[serde(rename = "exitCode")]
    pub exit_code: u8,
    /// The derived verdict.
    pub verdict: Verdict,
    /// Whether the declared plan was fully covered.
    pub coverage: Coverage,
    /// Whether every declared check reached a terminal evaluation.
    pub complete: bool,
}

/// The closed coverage vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Coverage {
    /// Every declared check ran to a terminal outcome.
    Complete,
    /// An explicitly optional check was unavailable or skipped.
    Incomplete,
    /// The plan itself could not be resolved.
    Unknown,
}

impl Coverage {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Incomplete => "incomplete",
            Self::Unknown => "unknown",
        }
    }
}

/// The closed failure-class vocabulary (the research table): what kind
/// of observation produced a non-pass row.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FailureClass {
    /// An evaluated assertion did not hold.
    Assertion,
    /// A static-analysis gate reported findings.
    StaticAnalysis,
    /// The application under test could not boot.
    Boot,
    /// A declared component never became available.
    MissingComponent,
    /// A component is present but incompatible.
    Incompatible,
    /// Provider/runner infrastructure failed (timeout, spawn, teardown).
    Infrastructure,
    /// A security/custody refusal.
    Security,
    /// The CI policy itself refused or promoted the row.
    Policy,
    /// Evidence was malformed, stale, or missing.
    EvidenceInvalid,
    /// Report/output publication failed.
    Output,
    /// The row passed; no failure class applies.
    None,
}

impl FailureClass {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Assertion => "assertion",
            Self::StaticAnalysis => "static-analysis",
            Self::Boot => "boot",
            Self::MissingComponent => "missing-component",
            Self::Incompatible => "incompatible",
            Self::Infrastructure => "infrastructure",
            Self::Security => "security",
            Self::Policy => "policy",
            Self::EvidenceInvalid => "evidence-invalid",
            Self::Output => "output",
            Self::None => "none",
        }
    }
}

/// The closed source-outcome vocabulary: what the underlying component
/// observed, before any policy translation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(clippy::enum_variant_names)]
pub enum SourceOutcome {
    /// The check ran and passed.
    Pass,
    /// The check ran and failed.
    Fail,
    /// A needed component/infrastructure was unavailable.
    Unavailable,
    /// A declared permanent absence (capability not implemented).
    Unsupported,
    /// The check ran with visible degradation.
    Degraded,
    /// A security/policy refusal.
    Denied,
    /// The run was cancelled before a terminal outcome.
    Cancelled,
    /// Planned but never reached (the wire spelling is `not-run`).
    #[serde(rename = "not-run")]
    NotRun,
}

impl SourceOutcome {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Unavailable => "unavailable",
            Self::Unsupported => "unsupported",
            Self::Degraded => "degraded",
            Self::Denied => "denied",
            Self::Cancelled => "cancelled",
            Self::NotRun => "not-run",
        }
    }
}

/// The closed effective-outcome vocabulary: what the policy decided.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EffectiveOutcome {
    /// Passed.
    Pass,
    /// Required failure (nonzero exit).
    Fail,
    /// Optional absence/findings surface as a warning (exit stays 0).
    Warn,
    /// Explicitly skipped under policy (exit stays 0).
    Skip,
    /// Required absence promoted to an error (nonzero exit).
    Error,
}

impl EffectiveOutcome {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Warn => "warn",
            Self::Skip => "skip",
            Self::Error => "error",
        }
    }

    /// Whether this outcome keeps the exit at zero.
    pub const fn is_exit_neutral(self) -> bool {
        matches!(self, Self::Pass | Self::Warn | Self::Skip)
    }
}

/// One check row of the report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CheckRow {
    /// The stable check id (a component id, a doctor check id, or the
    /// closed CI gate ids).
    pub id: String,
    /// Whether this check fails the run when it does not pass.
    pub required: bool,
    /// What the source observed.
    #[serde(rename = "sourceOutcome")]
    pub source_outcome: SourceOutcome,
    /// The failure class of a non-pass observation.
    #[serde(rename = "failureClass")]
    pub failure_class: FailureClass,
    /// What the policy decided.
    #[serde(rename = "effectiveOutcome")]
    pub effective_outcome: EffectiveOutcome,
    /// Sorted indexes into the diagnostics array.
    #[serde(rename = "diagnosticIndexes")]
    pub diagnostic_indexes: Vec<usize>,
    /// The closed registered reason token (a diagnostic id), or empty.
    pub detail: String,
}

/// One case row inside a suite.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CaseRow {
    /// The stable case id (`scenario.step[ordinal]`, a catalog check id,
    /// or a native gate case id).
    pub id: String,
    /// Whether this case fails the suite when it does not pass.
    pub required: bool,
    /// What the source observed.
    #[serde(rename = "sourceOutcome")]
    pub source_outcome: SourceOutcome,
    /// The failure class of a non-pass observation.
    #[serde(rename = "failureClass")]
    pub failure_class: FailureClass,
    /// Sorted indexes into the diagnostics array.
    #[serde(rename = "diagnosticIndexes")]
    pub diagnostic_indexes: Vec<usize>,
    /// The closed detail token (`expectation-mismatch`, a safe code), or
    /// empty.
    pub detail: String,
    /// The policy-effective outcome, computed once by the builder and
    /// serialized so every consumer (JSON, JUnit, Markdown) projects the
    /// same decision. Never recomputed downstream.
    #[serde(rename = "effectiveOutcome")]
    pub effective_outcome: EffectiveOutcome,
}

/// The shared policy table: one effective outcome per (required,
/// source outcome, failure class) under one policy. Both check rows and
/// case rows evaluate through this single function so the JSON, the
/// evaluation verdict, and the JUnit projection can never disagree.
pub(crate) fn evaluate_outcome(
    required: bool,
    source_outcome: SourceOutcome,
    failure_class: FailureClass,
    policy: super::build_policy::CiPolicyTable,
) -> EffectiveOutcome {
    use super::build_policy::AbsenceRule;
    match source_outcome {
        SourceOutcome::Pass => EffectiveOutcome::Pass,
        SourceOutcome::Fail => EffectiveOutcome::Fail,
        SourceOutcome::Denied => EffectiveOutcome::Fail,
        SourceOutcome::Cancelled => EffectiveOutcome::Fail,
        SourceOutcome::Degraded => {
            if required {
                EffectiveOutcome::Error
            } else {
                EffectiveOutcome::Warn
            }
        }
        SourceOutcome::Unsupported => {
            if failure_class == FailureClass::MissingComponent && !required {
                match policy.optional_absence {
                    AbsenceRule::Warn => EffectiveOutcome::Warn,
                    AbsenceRule::Error => EffectiveOutcome::Error,
                    AbsenceRule::Skip => EffectiveOutcome::Skip,
                }
            } else {
                EffectiveOutcome::Error
            }
        }
        SourceOutcome::Unavailable | SourceOutcome::NotRun => {
            if required {
                EffectiveOutcome::Error
            } else if failure_class == FailureClass::Infrastructure {
                // A provider/runner infrastructure failure is a
                // distinguishable failure, never an optional absence:
                // it blocks under every policy.
                EffectiveOutcome::Fail
            } else {
                match policy.optional_absence {
                    AbsenceRule::Warn => EffectiveOutcome::Warn,
                    AbsenceRule::Error => EffectiveOutcome::Error,
                    AbsenceRule::Skip => EffectiveOutcome::Skip,
                }
            }
        }
    }
}

/// One suite of case rows (scenario, conformance, native, script).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Suite {
    /// The stable suite id.
    pub id: String,
    /// The closed suite kind.
    pub kind: SuiteKind,
    /// The target the suite ran against, when scoped.
    pub target: Option<String>,
    /// The case rows in execution order (assertion order is semantic and
    /// is never reordered).
    pub cases: Vec<CaseRow>,
}

/// The closed suite-kind vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SuiteKind {
    /// Durable scenario run records.
    Scenario,
    /// Adapter-conformance catalog rows.
    Conformance,
    /// Native build/test gate receipts.
    Native,
    /// Explicitly supplied script-gate observations.
    Script,
    /// Per-target adapter validation rows.
    Adapter,
}

impl SuiteKind {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scenario => "scenario",
            Self::Conformance => "conformance",
            Self::Native => "native",
            Self::Script => "script",
            Self::Adapter => "adapter",
        }
    }
}

/// The publication decision over the #120 privacy vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Publication {
    /// The closed classification label of a CI report artifact.
    pub classification: &'static str,
    /// Whether the report may be published by the caller.
    pub decision: PublicationDecision,
}

/// The closed publication decision vocabulary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PublicationDecision {
    /// The report is a `ci-derived` artifact and may be uploaded by the
    /// caller's own workflow decision.
    Allowed,
    /// A redaction/custody rule withholds the bytes; the run still
    /// reports its outcome, but the artifact must not be published.
    PublicationUnavailable,
}

/// The one CI report document.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CiReport {
    /// The exact wire discriminator.
    #[serde(rename = "schema_version")]
    pub schema_version: &'static str,
    /// The contract identity.
    pub identity: &'static str,
    /// The producer block.
    pub producer: Producer,
    /// The invocation shape.
    pub invocation: Invocation,
    /// The exact provenance pins.
    pub provenance: Provenance,
    /// The underlying command result (preserved, never rewritten).
    #[serde(rename = "commandResult")]
    pub command_result: Outcome,
    /// The authoritative gated outcome.
    pub evaluation: Evaluation,
    /// The check rows in id order.
    pub checks: Vec<CheckRow>,
    /// The suites in id order.
    pub suites: Vec<Suite>,
    /// Every referenced diagnostic index, sorted and unique.
    #[serde(rename = "diagnosticIndexes")]
    pub diagnostic_indexes: Vec<usize>,
    /// The source diagnostics in normalized order.
    pub diagnostics: Vec<SerializableDiagnostic>,
    /// The publication decision.
    pub publication: Publication,
}

/// A serializable mirror of the closed diagnostic wire item: the typed
/// core diagnostic serialized through its own closed projection.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SerializableDiagnostic {
    inner: Diagnostic,
}

impl SerializableDiagnostic {
    /// Wrap one normalized diagnostic.
    pub fn new(diagnostic: Diagnostic) -> Self {
        Self { inner: diagnostic }
    }
}

impl CiReport {
    /// Derive the report digest over the exact emitted bytes (compact
    /// canonical form with one trailing LF, no final newline inside the
    /// digest input per the house digest rule).
    pub fn digest(&self) -> String {
        let bytes = self.to_json_string();
        crate::digest::sha256_hex(bytes.as_bytes())
    }

    /// The digest spelling with its algorithm prefix.
    pub fn digest_spelling(&self) -> String {
        format!("sha256:{}", self.digest())
    }

    /// The exact compact JSON bytes with byte-sorted keys and one
    /// trailing LF. Canonical: serde_json's object writer emits struct
    /// fields in declaration order and BTreeMap entries in key order;
    /// the model sorts every free-form map before construction.
    pub fn to_json_string(&self) -> String {
        let value = serde_json::to_value(self).expect("the ci report serializes");
        let sorted = sort_json_keys(value);
        let mut bytes = serde_json::to_string(&sorted).expect("sorted value serializes");
        bytes.push('\n');
        bytes
    }

    /// Validate the cross-field invariants the JSON Schema cannot
    /// express; a violation is a developer fault (fail closed).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.diagnostics.len() > MAX_DIAGNOSTICS {
            return Err("too many diagnostics");
        }
        let referenced: std::collections::BTreeSet<usize> = self
            .checks
            .iter()
            .flat_map(|check| check.diagnostic_indexes.iter().copied())
            .chain(
                self.suites
                    .iter()
                    .flat_map(|suite| suite.cases.iter())
                    .flat_map(|case| case.diagnostic_indexes.iter().copied()),
            )
            .collect();
        for index in &referenced {
            if *index >= self.diagnostics.len() {
                return Err("dangling diagnostic index");
            }
        }
        let declared: Vec<usize> = self.diagnostic_indexes.clone();
        if declared != referenced.into_iter().collect::<Vec<_>>() {
            return Err("diagnosticIndexes disagree with the referenced set");
        }
        // Sorted checks by id, unique.
        for window in self.checks.windows(2) {
            if window[0].id >= window[1].id {
                return Err("checks are not sorted/unique by id");
            }
        }
        // Suites sorted by id, unique.
        for window in self.suites.windows(2) {
            if window[0].id >= window[1].id {
                return Err("suites are not sorted/unique by id");
            }
        }
        // Count coherence: a fail/error row must exist when the
        // evaluation blocks.
        let blocking = self
            .checks
            .iter()
            .map(|row| row.effective_outcome)
            .chain(
                self.suites
                    .iter()
                    .flat_map(|suite| suite.cases.iter())
                    .map(|case| case.effective_outcome),
            )
            .any(|outcome| matches!(outcome, EffectiveOutcome::Fail | EffectiveOutcome::Error));
        if blocking != (self.evaluation.verdict == Verdict::Blocked) {
            return Err("verdict disagrees with the blocking rows");
        }
        Ok(())
    }

    /// The full provenance and evaluation blocks for the summary line.
    pub fn pinned_summary(&self) -> String {
        let git = match &self.provenance.git.commit {
            ValueState::Known { value } => match value {
                KnownValue::Revision(text) => text.clone(),
                _ => String::new(),
            },
            ValueState::Unknown { reason } => format!("git:{}", reason.as_str()),
        };
        format!(
            "{} {} ({}) commit {}",
            self.invocation.command.as_str(),
            self.evaluation.verdict.as_str(),
            self.evaluation.status,
            git,
        )
    }
}

/// The bounded data projection the report carries for one diagnostic:
/// the typed `DataObject` is already closed and bounded; it serializes
/// through the diagnostic itself.
pub type DiagnosticData = BTreeMap<String, serde_json::Value>;

/// Recursively byte-sort every object's keys: the canonical wire form
/// (review F16) is byte-sorted JSON, independent of the model's field
/// declaration order. Arrays keep their order (assertion execution
/// order and cause chains are semantic).
fn sort_json_keys(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> = map
                .into_iter()
                .map(|(key, value)| (key, sort_json_keys(value)))
                .collect();
            entries.sort_by(|left, right| left.0.as_bytes().cmp(right.0.as_bytes()));
            let mut sorted = serde_json::Map::new();
            for (key, value) in entries {
                sorted.insert(key, value);
            }
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_json_keys).collect())
        }
        other => other,
    }
}
