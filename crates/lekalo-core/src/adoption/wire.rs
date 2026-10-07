//! Closed wire decoding, canonical bytes, and digest recipes of the
//! #136 adoption documents.
//!
//! [`decode`] is the single entry from raw bytes to any typed adoption
//! document: the byte bound, UTF-8, JSON syntax with duplicate-key
//! refusal, the closed member sets, identifier/digest/grammar rules, and
//! bounds each fail closed before any service runs. Self digests use the
//! content-address recipe: compact recursively key-sorted UTF-8 over the
//! document without its self-digest member, prefixed by the
//! family-specific `lekalo.<family>.v0.8.0` domain.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use super::diagnostic;
use super::version;

/// Why one closed adoption document could not be decoded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WireError {
    /// Over the byte bound.
    Size,
    /// Invalid UTF-8, JSON syntax, or a duplicate key.
    Encoding,
    /// A closed member, grammar, or bound was violated.
    Shape,
    /// The `schema_version` discriminator is not the accepted 0.8.0 one.
    Version(String),
}

impl WireError {
    /// The refusing result: version errors keep their exit-5 class.
    pub fn domain_result(&self) -> crate::result::DomainResult {
        match self {
            Self::Version(_) => crate::result::DomainResult::unsupported_version(self.set()),
            _ => crate::result::DomainResult::invalid(self.set()),
        }
    }

    /// The refusing diagnostic set.
    pub fn set(&self) -> crate::diagnostics::DiagnosticSet {
        match self {
            Self::Size => diagnostic::input_invalid_set("document", "over-limit"),
            Self::Encoding => diagnostic::input_invalid_set("document", "malformed-json"),
            Self::Shape => diagnostic::input_invalid_set("document", "closed-shape"),
            Self::Version(actual) => diagnostic::version_unsupported_set(
                actual,
                &format!("lekalo/adoption-*/v{}", version::VERSION),
            ),
        }
    }
}

/// Recursive duplicate-key refusal before typed decoding;
/// `serde_json::Value` alone silently keeps the last duplicate.
pub fn unique_json(bytes: &[u8]) -> Result<Json, WireError> {
    if bytes.len() > version::MAX_DOCUMENT_BYTES {
        return Err(WireError::Size);
    }
    let mut de = serde_json::Deserializer::from_slice(bytes);
    let Unique(value) = Unique::deserialize(&mut de).map_err(|_| WireError::Encoding)?;
    de.end().map_err(|_| WireError::Encoding)?;
    Ok(value)
}

struct Unique(Json);

impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = Unique;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("unique-key JSON")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|number| Unique(number.into()))
                    .ok_or_else(|| serde::de::Error::custom("number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Json::Null))
            }
            fn visit_none<E: serde::de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Json::Null))
            }
            fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<Unique, D::Error> {
                d.deserialize_any(Visitor)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(next) = a.next_element::<Unique>()? {
                    values.push(next.0);
                }
                Ok(Unique(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = a.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate-key"));
                    }
                    values.insert(key, a.next_value::<Unique>()?.0);
                }
                Ok(Unique(values.into()))
            }
        }
        d.deserialize_any(Visitor)
    }
}

/// Decode raw bytes into one closed typed document.
pub(crate) fn decode<T: serde::de::DeserializeOwned>(
    bytes: &[u8],
    expected_schema_version: &'static str,
) -> Result<T, WireError> {
    let value = unique_json(bytes)?;
    let object = value.as_object().ok_or(WireError::Shape)?;
    match object.get("schema_version").and_then(Json::as_str) {
        Some(actual) if actual == expected_schema_version => {}
        Some(actual) => return Err(WireError::Version(actual.to_owned())),
        None => return Err(WireError::Shape),
    }
    serde_json::from_value(value).map_err(|_| WireError::Shape)
}

/// Compact recursively key-sorted JSON with no insignificant whitespace.
pub fn canonical_json(value: &Json) -> String {
    match value {
        Json::Array(items) => {
            let inner: Vec<String> = items.iter().map(canonical_json).collect();
            format!("[{}]", inner.join(","))
        }
        Json::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let inner: Vec<String> = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(key).expect("key"),
                        canonical_json(&map[key])
                    )
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        other => serde_json::to_string(other).expect("scalar"),
    }
}

/// The family-specific content-address digest: SHA-256 over the domain
/// prefix and the canonical bytes of `value` (which must already exclude
/// its own self-digest member).
pub fn digest(family: &str, value: &Json) -> String {
    let prefix = format!("lekalo.{family}.v{}\n", version::VERSION);
    let mut input = prefix.into_bytes();
    input.extend_from_slice(canonical_json(value).as_bytes());
    format!("sha256:{}", crate::digest::sha256_hex(&input))
}

/// Remove one top-level member and return the canonical remainder, used
/// exactly for self-digest inputs.
pub fn without_member(value: &Json, member: &str) -> Json {
    let mut copy = value.clone();
    if let Json::Object(ref mut map) = copy {
        map.remove(member);
    }
    copy
}

// ---------------------------------------------------------------------------
// Closed shared members
// ---------------------------------------------------------------------------

const KINDS_DEFINITION: [&str; 6] = [
    "command",
    "query",
    "value-object",
    "scalar",
    "event",
    "policy",
];
const KINDS_SCOPE: [&str; 3] = ["symbol", "module", "use-case"];
const KINDS_EFFECTIVE: [&str; 5] = [
    "observed",
    "contracted-legacy",
    "contracted-new",
    "unbound",
    "conflicting",
];
const CONFIDENCE: [&str; 5] = ["exact", "high", "medium", "low", "unknown"];
const KINDS_DISCREPANCY: [&str; 3] = ["binding", "definition", "evidence"];
const KINDS_ROW_STATE: [&str; 3] = ["current", "stale", "unknown"];
const KINDS_ORIGIN: [&str; 2] = ["legacy", "new"];
const KINDS_DEFINITION_STATUS: [&str; 2] = ["proposed", "confirmed"];
const KINDS_BINDING_STATE: [&str; 4] = ["explicit", "confirmed", "inferred", "stale"];
const KINDS_WRITE_ACTION: [&str; 2] = ["create", "replace"];
const KINDS_WRITE_ACTION_DELETABLE: [&str; 3] = ["create", "replace", "delete"];
const KINDS_VARIANT: [&str; 2] = ["specification", "execution"];
const KINDS_CHECK: [&str; 3] = ["native-test", "behavior-oracle", "semantic"];
const KINDS_OUTCOME_STATUS: [&str; 3] = ["passed", "failed", "unavailable"];
const KINDS_CONCLUSION: [&str; 3] = ["preserved", "failed", "unavailable"];
const KINDS_PHASE: [&str; 3] = ["confirmation", "apply", "rollback"];
const KINDS_RECEIPT_OUTCOME: [&str; 4] =
    ["completed", "failed", "rolled-back", "recovery-required"];

fn check_member(value: &str, allowed: &[&str]) -> Result<(), WireError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(WireError::Shape)
    }
}

fn check_path(path: &str) -> Result<(), WireError> {
    if version::is_rel_path(path) {
        Ok(())
    } else {
        Err(WireError::Shape)
    }
}

fn check_digest(digest: &Option<String>) -> Result<(), WireError> {
    match digest {
        Some(digest) if version::is_sha256(digest) => Ok(()),
        None => Ok(()),
        Some(_) => Err(WireError::Shape),
    }
}

fn check_writes(writes: &[WriteEntry], deletable: bool) -> Result<(), WireError> {
    if writes.len() > version::MAX_WRITES {
        return Err(WireError::Shape);
    }
    let actions: &[&str] = if deletable {
        &KINDS_WRITE_ACTION_DELETABLE
    } else {
        &KINDS_WRITE_ACTION
    };
    for write in writes {
        check_path(&write.path)?;
        check_member(&write.action, actions)?;
        check_digest(&write.before)?;
        check_digest(&write.after)?;
        if let Some(content) = &write.content {
            if content.len() > version::MAX_FILE_BYTES {
                return Err(WireError::Shape);
            }
        }
    }
    Ok(())
}

fn check_pins(pins: &Pins) -> Result<(), WireError> {
    if !version::is_sha256(&pins.model) {
        return Err(WireError::Shape);
    }
    check_digest(&pins.lock)?;
    check_digest(&pins.observed_index)?;
    if pins.sources.len() > version::MAX_SOURCE_FILES {
        return Err(WireError::Shape);
    }
    for source in &pins.sources {
        check_path(&source.path)?;
        if !version::is_sha256(&source.sha256) {
            return Err(WireError::Shape);
        }
    }
    Ok(())
}

fn check_scope(scope: &Scope) -> Result<(), WireError> {
    check_member(&scope.kind, &KINDS_SCOPE)?;
    if !version::is_scope_id(&scope.id) {
        return Err(WireError::Shape);
    }
    Ok(())
}

/// The closed post-decode vocabulary and bounds every document must
/// hold; the Ajv schema gate is the second independent implementation.
trait ClosedValidate {
    fn validate(&self) -> Result<(), WireError>;
}

impl ClosedValidate for AnalysisDocument {
    fn validate(&self) -> Result<(), WireError> {
        check_scope(&self.scope)?;
        check_pins(&self.pins)?;
        if self.definitions.len() > version::MAX_DEFINITIONS {
            return Err(WireError::Shape);
        }
        for definition in &self.definitions {
            if !version::is_semantic_id(&definition.id) {
                return Err(WireError::Shape);
            }
            check_member(&definition.kind, &KINDS_DEFINITION)?;
            check_member(&definition.status, &KINDS_DEFINITION_STATUS)?;
            check_member(&definition.confidence, &CONFIDENCE)?;
            check_member(
                &definition.provenance.source,
                &["observed-index", "authored"],
            )?;
        }
        for binding in &self.bindings {
            if !version::is_native_id(&binding.proposal) || !version::is_native_id(&binding.native)
            {
                return Err(WireError::Shape);
            }
            check_member(&binding.state, &KINDS_BINDING_STATE)?;
            check_member(&binding.confidence, &CONFIDENCE)?;
        }
        for ambiguity in &self.ambiguities {
            if ambiguity.candidates.len() < 2
                || ambiguity.candidates.len() > version::MAX_CANDIDATES
            {
                return Err(WireError::Shape);
            }
        }
        for discrepancy in &self.discrepancies {
            check_member(&discrepancy.kind, &KINDS_DISCREPANCY)?;
        }
        Ok(())
    }
}

impl ClosedValidate for ConfirmationPlan {
    fn validate(&self) -> Result<(), WireError> {
        check_pins(&self.pins)?;
        check_writes(&self.writes, false)?;
        if self.selections.definitions.is_empty()
            || self.selections.definitions.len() > version::MAX_DEFINITIONS
        {
            return Err(WireError::Shape);
        }
        Ok(())
    }
}

impl ClosedValidate for PatchDocument {
    fn validate(&self) -> Result<(), WireError> {
        check_writes(&self.files, true)?;
        check_member(
            &self.origin.kind,
            &["supplied-reviewed", "adapter-generated"],
        )?;
        if self.files.is_empty() {
            return Err(WireError::Shape);
        }
        Ok(())
    }
}

impl ClosedValidate for RefactorPlan {
    fn validate(&self) -> Result<(), WireError> {
        check_scope(&self.selection)?;
        check_pins(&self.pins)?;
        for recommendation in &self.recommendations {
            check_member(
                &recommendation.kind,
                &["contract", "api", "effect", "policy", "test", "binding"],
            )?;
            check_member(&recommendation.risk, &["low", "medium", "high"])?;
            for file in &recommendation.files {
                check_path(file)?;
            }
        }
        Ok(())
    }
}

impl ClosedValidate for VerificationDocument {
    fn validate(&self) -> Result<(), WireError> {
        check_member(&self.variant, &KINDS_VARIANT)?;
        if self.checks.len() > version::MAX_CHECKS {
            return Err(WireError::Shape);
        }
        for check in &self.checks {
            check_member(&check.kind, &KINDS_CHECK)?;
        }
        for side in [&self.baseline, &self.candidate].into_iter().flatten() {
            if side.checks.len() > version::MAX_CHECKS {
                return Err(WireError::Shape);
            }
            for outcome in &side.checks {
                check_member(&outcome.status, &KINDS_OUTCOME_STATUS)?;
            }
        }
        if let Some(oracle) = &self.oracle {
            check_member(&oracle.status, &KINDS_OUTCOME_STATUS)?;
        }
        if let Some(conclusion) = &self.conclusion {
            check_member(conclusion, &KINDS_CONCLUSION)?;
        }
        for token in &self.confinement {
            // The disposable-copy token may bind the exact candidate
            // revision (`disposable-copy:<sha256>`); the bare form and
            // the bound form are the only accepted spellings.
            if token == "disposable-copy"
                || token == "bounded-output"
                || token == "no-network"
                || token
                    .strip_prefix("disposable-copy:")
                    .is_some_and(version::is_sha256)
            {
                continue;
            }
            return Err(WireError::Shape);
        }
        if let Some(tool) = &self.tool {
            tool_digest_or_null(&tool.digest)?;
        }
        if let Some(custody) = &self.custody {
            if !version::is_sha256(&custody.run) {
                return Err(WireError::Shape);
            }
        }
        Ok(())
    }
}

impl ClosedValidate for AdoptionReceipt {
    fn validate(&self) -> Result<(), WireError> {
        check_member(&self.phase, &KINDS_PHASE)?;
        check_member(&self.outcome, &KINDS_RECEIPT_OUTCOME)?;
        check_writes(&self.writes, true)?;
        check_pins(&self.pins)?;
        Ok(())
    }
}

impl ClosedValidate for CoverageDocument {
    fn validate(&self) -> Result<(), WireError> {
        check_scope(&self.scope)?;
        if self.rows.len() > version::MAX_ROWS {
            return Err(WireError::Shape);
        }
        for row in &self.rows {
            if !version::is_semantic_id(&row.symbol) {
                return Err(WireError::Shape);
            }
            check_member(&row.effective, &KINDS_EFFECTIVE)?;
            check_member(&row.state, &KINDS_ROW_STATE)?;
            if let Some(origin) = &row.origin {
                check_member(origin, &KINDS_ORIGIN)?;
            }
        }
        Ok(())
    }
}

impl ClosedValidate for StatusReport {
    fn validate(&self) -> Result<(), WireError> {
        for panel in &self.panels {
            check_member(&panel.mode, &KINDS_EFFECTIVE)?;
            check_member(&panel.state, &KINDS_ROW_STATE)?;
            check_member(&panel.freshness, &["fresh", "stale", "unknown"])?;
        }
        Ok(())
    }
}

impl ClosedValidate for ImpactReport {
    fn validate(&self) -> Result<(), WireError> {
        for boundary in &self.boundaries {
            check_member(&boundary.mode, &KINDS_EFFECTIVE)?;
        }
        Ok(())
    }
}

impl ClosedValidate for TraceReport {
    fn validate(&self) -> Result<(), WireError> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// The ten closed documents
// ---------------------------------------------------------------------------

/// The derived, read-only analysis proposal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisDocument {
    pub schema_version: String,
    pub identity: String,
    pub project: String,
    pub scope: Scope,
    pub target: Option<String>,
    pub profile: Option<String>,
    pub pins: Pins,
    pub module: ModuleProposal,
    pub definitions: Vec<AnalysisDefinition>,
    pub bindings: Vec<AnalysisBinding>,
    pub contracts: Vec<AnalysisContract>,
    pub unknowns: Vec<Gap>,
    pub ambiguities: Vec<Ambiguity>,
    pub discrepancies: Vec<Discrepancy>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub kind: String,
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleProposal {
    pub id: String,
    pub proposed: bool,
    pub native_root: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisDefinition {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub confidence: String,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub source: String,
    pub native: Option<String>,
    pub path: Option<String>,
    pub line: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisBinding {
    pub proposal: String,
    pub symbol: String,
    pub native: String,
    pub confidence: String,
    pub state: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnalysisContract {
    pub symbol: String,
    pub tests: Vec<String>,
    pub gates: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gap {
    pub subject: String,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ambiguity {
    pub symbol: String,
    pub candidates: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discrepancy {
    pub symbol: String,
    pub kind: String,
    pub detail: String,
}

/// The reviewed authorization boundary of one selective confirmation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmationPlan {
    pub schema_version: String,
    pub identity: String,
    pub plan: String,
    pub project: String,
    pub analysis: Option<String>,
    pub selections: ConfirmationSelections,
    pub writes: Vec<WriteEntry>,
    pub pins: Pins,
    pub unresolved: Vec<Unresolved>,
    pub rollback: RollbackHome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfirmationSelections {
    pub definitions: Vec<String>,
    pub binding: Option<BindingSelection>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingSelection {
    pub proposal: String,
    pub candidate: String,
    pub symbol: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unresolved {
    pub id: String,
    pub reason: String,
}

/// The closed candidate change set for maintained source files.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchDocument {
    pub schema_version: String,
    pub identity: String,
    pub patch: String,
    pub origin: PatchOrigin,
    pub scope: PatchScope,
    pub files: Vec<WriteEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchOrigin {
    pub kind: String,
    pub identity: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchScope {
    pub symbol: String,
    pub target: Option<String>,
    pub profile: Option<String>,
}

/// The exact revision-bound selective refactor plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefactorPlan {
    pub schema_version: String,
    pub identity: String,
    pub plan: String,
    pub project: String,
    pub analysis: String,
    pub selection: Scope,
    pub patch: Option<String>,
    pub verification: Option<String>,
    pub recommendations: Vec<Recommendation>,
    pub unsupported: Vec<UnsupportedAdvice>,
    pub read_scope: Vec<String>,
    pub mutation_scope: Vec<String>,
    pub preservation: Vec<Preservation>,
    pub pins: Pins,
    pub rollback: RollbackHome,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recommendation {
    pub id: String,
    pub kind: String,
    pub reason: String,
    pub files: Vec<String>,
    pub risk: String,
    pub verification: Vec<String>,
    pub uncertainty: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnsupportedAdvice {
    pub kind: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preservation {
    pub obligation: String,
    pub check: String,
}

/// The closed verification specification or execution evidence.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationDocument {
    pub schema_version: String,
    pub identity: String,
    pub variant: String,
    pub verification: String,
    pub spec: Option<String>,
    pub plan: Option<String>,
    pub tool: Option<ToolRef>,
    pub baseline_source: Option<String>,
    pub checks: Vec<CheckSpec>,
    pub confinement: Vec<String>,
    pub baseline: Option<CheckResults>,
    pub candidate: Option<CheckResults>,
    pub oracle: Option<OracleVerdict>,
    pub conclusion: Option<String>,
    /// Execution custody: set only by this module's real runs; apply
    /// refuses an execution that lacks it. Specifications never carry
    /// one (typed null on the wire for the shared closed member set).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custody: Option<super::verification::ExecutionCustody>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolRef {
    pub program: String,
    pub args: Vec<String>,
    pub digest: Option<String>,
}

/// The exact tool byte-digest grammar (present or typed null).
pub(crate) fn tool_digest_or_null(digest: &Option<String>) -> Result<(), WireError> {
    check_digest(digest)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckSpec {
    pub id: String,
    pub kind: String,
    pub expect_exit: u8,
    pub expect_stdout: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckResults {
    pub checks: Vec<CheckOutcome>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckOutcome {
    pub id: String,
    pub status: String,
    pub exit: Option<u32>,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleVerdict {
    pub status: String,
    pub detail: String,
}

/// The closed receipt of one confirmation, apply, or rollback phase.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdoptionReceipt {
    pub schema_version: String,
    pub identity: String,
    pub receipt: String,
    pub phase: String,
    pub project: String,
    pub plan: String,
    pub outcome: String,
    pub writes: Vec<WriteEntry>,
    pub journal: Option<String>,
    pub snapshots: Vec<String>,
    pub verification: Option<String>,
    pub pins: Pins,
    pub analysis: Option<String>,
}

/// The derived hybrid coverage join.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageDocument {
    pub schema_version: String,
    pub identity: String,
    pub project: String,
    pub scope: Scope,
    pub rows: Vec<CoverageRow>,
    pub counts: CoverageCounts,
    pub gaps: Vec<Gap>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageRow {
    pub symbol: String,
    pub effective: String,
    pub state: String,
    pub origin: Option<String>,
    pub contract: Option<String>,
    pub binding: Option<BindingSelection>,
    pub tests: Vec<String>,
    pub verification: Option<String>,
    pub gaps: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoverageCounts {
    pub observed: u64,
    #[serde(rename = "contracted-legacy")]
    pub contracted_legacy: u64,
    #[serde(rename = "contracted-new")]
    pub contracted_new: u64,
    pub unbound: u64,
    pub conflicting: u64,
}

/// The typed opt-in wrapper around one upstream status result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusReport {
    pub schema_version: String,
    pub identity: String,
    pub upstream: Json,
    pub coverage: String,
    pub panels: Vec<PanelRow>,
    pub gaps: Vec<Gap>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelRow {
    pub symbol: String,
    pub mode: String,
    pub state: String,
    pub freshness: String,
}

/// The typed opt-in wrapper around one upstream impact result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImpactReport {
    pub schema_version: String,
    pub identity: String,
    pub upstream: Json,
    pub coverage: String,
    pub boundaries: Vec<BoundaryWitness>,
    pub gaps: Vec<Gap>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoundaryWitness {
    pub symbol: String,
    pub mode: String,
    pub witness: String,
}

/// The typed opt-in wrapper around one trace manifest and query result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraceReport {
    pub schema_version: String,
    pub identity: String,
    pub manifest: Json,
    pub query: Json,
    pub coverage: String,
    pub verification: Vec<String>,
    pub uncovered: Vec<Gap>,
    pub stale: Vec<Gap>,
}

// ---------------------------------------------------------------------------
// Shared closed members
// ---------------------------------------------------------------------------

/// The closed source/model/lock/adapter/profile pin set shared by the
/// analysis, both plans, and every receipt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pins {
    pub model: String,
    pub lock: Option<String>,
    pub observed_index: Option<String>,
    pub adapter: Option<AdapterPin>,
    pub sources: Vec<SourcePin>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterPin {
    pub id: String,
    pub version: String,
    pub digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcePin {
    pub path: String,
    pub sha256: String,
}

/// One exact planned write. `content` carries the exact new file bytes
/// (absent for deletes); `before`/`after` are the exact digests.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteEntry {
    pub path: String,
    pub action: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub content: Option<String>,
}

/// The rollback home of one plan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RollbackHome {
    pub journal: String,
    pub snapshots: String,
}

// ---------------------------------------------------------------------------
// Typed decode entries
// ---------------------------------------------------------------------------

macro_rules! decode_entry {
    ($name:ident, $ty:ty, $version:expr) => {
        /// Decode one closed document from raw bytes.
        pub fn $name(bytes: &[u8]) -> Result<$ty, WireError> {
            let document = decode(bytes, $version)?;
            ClosedValidate::validate(&document)?;
            Ok(document)
        }
    };
}

decode_entry!(
    analysis_of_bytes,
    AnalysisDocument,
    version::ANALYSIS_SCHEMA_VERSION
);
decode_entry!(
    confirmation_plan_of_bytes,
    ConfirmationPlan,
    version::CONFIRMATION_PLAN_SCHEMA_VERSION
);
decode_entry!(patch_of_bytes, PatchDocument, version::PATCH_SCHEMA_VERSION);
decode_entry!(
    refactor_plan_of_bytes,
    RefactorPlan,
    version::REFACTOR_PLAN_SCHEMA_VERSION
);
decode_entry!(
    verification_of_bytes,
    VerificationDocument,
    version::VERIFICATION_SCHEMA_VERSION
);
decode_entry!(
    execution_of_bytes,
    VerificationDocument,
    version::VERIFICATION_SCHEMA_VERSION
);
decode_entry!(
    receipt_of_bytes,
    AdoptionReceipt,
    version::RECEIPT_SCHEMA_VERSION
);
decode_entry!(
    coverage_of_bytes,
    CoverageDocument,
    version::COVERAGE_SCHEMA_VERSION
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_keys_refuse_the_whole_document() {
        let bytes = br#"{"schema_version":"lekalo/adoption-analysis/v0.8.0","a":1,"a":2}"#;
        assert_eq!(unique_json(bytes).unwrap_err(), WireError::Encoding);
    }

    #[test]
    fn canonical_json_sorts_keys_recursively() {
        let value: Json = serde_json::from_str(r#"{"b":{"d":1,"c":2},"a":[2,1]}"#).expect("parses");
        assert_eq!(canonical_json(&value), r#"{"a":[2,1],"b":{"c":2,"d":1}}"#);
    }

    #[test]
    fn digests_are_domain_separated_and_deterministic() {
        let value: Json = serde_json::from_str(r#"{"x":1}"#).expect("parses");
        let first = digest("adoption-analysis", &value);
        let second = digest("adoption-analysis", &value);
        let other = digest("adoption-receipt", &value);
        assert_eq!(first, second);
        assert_ne!(first, other);
        assert!(version::is_sha256(&first));
    }

    #[test]
    fn wrong_discriminators_fail_closed_as_version_errors() {
        let bytes = br#"{"schema_version":"lekalo/adoption-analysis/v0.2.16"}"#;
        let error = analysis_of_bytes(bytes).unwrap_err();
        assert_eq!(
            error,
            WireError::Version("lekalo/adoption-analysis/v0.2.16".to_owned())
        );
    }

    #[test]
    fn execution_documents_never_decode_as_wrong_version_or_invalid() {
        // A wrong-version execution document keeps its exit-5 class
        // through the shared domain mapping (X9).
        let bytes = br#"{"schema_version":"lekalo/adoption-verification/v0.2.16"}"#;
        let error = execution_of_bytes(bytes).unwrap_err();
        assert!(matches!(error, WireError::Version(_)));
        assert_eq!(
            error.domain_result().status(),
            crate::result::Status::UnsupportedVersion
        );
    }

    #[test]
    fn the_committed_candidate_patch_digest_recipe_matches_node_parity() {
        // The Node gate computes the same content address over the same
        // canonical bytes; this pins the recipe against drift.
        let bytes =
            include_bytes!("../../../../tests/fixtures/adoption/refactor/candidate-patch.json");
        let document = patch_of_bytes(bytes).expect("decodes");
        let value = serde_json::to_value(&document).expect("serializes");
        let recomputed = digest("adoption-patch", &without_member(&value, "patch"));
        assert_eq!(recomputed, document.patch);
    }
}
