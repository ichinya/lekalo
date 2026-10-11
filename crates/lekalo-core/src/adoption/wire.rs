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
    /// Whether this is the version-discriminator refusal (the exit-5
    /// class through every consumer).
    pub fn is_version(&self) -> bool {
        matches!(self, Self::Version(_))
    }

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

/// Decode raw bytes into the raw closed JSON value: the byte bound,
/// UTF-8/JSON syntax with duplicate-key refusal, the exact
/// `schema_version` discriminator, and the top-level required keys —
/// the raw value is what nested presence checks run over (X8/R7).
pub(crate) fn decode_value(
    bytes: &[u8],
    expected_schema_version: &'static str,
    required_keys: &'static [&'static str],
) -> Result<Json, WireError> {
    let value = unique_json(bytes)?;
    let object = value.as_object().ok_or(WireError::Shape)?;
    match object.get("schema_version").and_then(Json::as_str) {
        Some(actual) if actual == expected_schema_version => {}
        Some(actual) => return Err(WireError::Version(actual.to_owned())),
        None => return Err(WireError::Shape),
    }
    for key in required_keys {
        if !object.contains_key(*key) {
            return Err(WireError::Shape);
        }
    }
    Ok(value)
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
const KINDS_WRITE_ACTION_DELETABLE: [&str; 4] = ["create", "replace", "delete", "binding"];
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

/// The bounded non-empty token grammar (`$defs/token`): minLength 1,
/// maxLength 256.
fn check_token(token: &str) -> Result<(), WireError> {
    if (1..=256).contains(&token.len()) {
        Ok(())
    } else {
        Err(WireError::Shape)
    }
}

/// A required-but-nullable digest member: present-or-null sha256.
fn check_digest_value(digest: &str) -> Result<(), WireError> {
    if version::is_sha256(digest) {
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

/// The exact closed identity literal of one document family: the Ajv
/// schemas' `identity` const, enforced on the Rust side with the same
/// fail-closed refusals (X8 parity).
fn check_identity(identity: &str, expected: &str) -> Result<(), WireError> {
    if identity == expected {
        Ok(())
    } else {
        Err(WireError::Shape)
    }
}

/// The closed post-decode vocabulary and bounds every document must
/// hold; the Ajv schema gate is the second independent implementation.
trait ClosedValidate {
    /// The exact member keys every accepted document carries; the
    /// Ajv schemas' `required` lists. Decode checks them over the
    /// raw object before typed deserialization, so a missing key
    /// refuses as a Shape error exactly like the schema (X8).
    const REQUIRED_KEYS: &'static [&'static str];
    fn validate(&self) -> Result<(), WireError>;
    /// The nested required-but-nullable members the raw object must
    /// carry even when their value is `null` (X8/R7): `Option`-typed
    /// serde members default a missing key to `None`, so presence is
    /// checked over the raw JSON exactly where the schema demands it.
    fn validate_raw(_value: &Json) -> Result<(), WireError> {
        Ok(())
    }
}

/// Whether the raw object at `path` carries the final member (its
/// value may be `null`); `false` when any path step is absent or not
/// an object.
fn raw_member_present(value: &Json, path: &[&str]) -> bool {
    let Some((last, steps)) = path.split_last() else {
        return false;
    };
    let mut current = value;
    for step in steps {
        current = match current.as_object().and_then(|object| object.get(*step)) {
            Some(next) => next,
            None => return false,
        };
    }
    current
        .as_object()
        .is_some_and(|object| object.contains_key(*last))
}

/// Whether every element of the raw array at `path` carries `key`
/// (value may be `null`); an absent array refuses, an empty one holds
/// vacuously.
fn raw_each_member_present(value: &Json, path: &[&str], key: &str) -> bool {
    let Some((last, steps)) = path.split_last() else {
        return false;
    };
    // A nullable parent (or a null array) holds vacuously; a present
    // non-array does not carry the member anywhere.
    let parent = match raw_at(value, steps) {
        Some(parent) => parent,
        // An absent optional ancestor holds vacuously.
        None => return true,
    };
    if parent.is_null() {
        return true;
    }
    let container = match parent.get(*last) {
        Some(container) => container,
        // An absent optional container holds vacuously: no element can
        // violate a per-element presence rule.
        None => return true,
    };
    if container.is_null() {
        return true;
    }
    let Some(items) = container.as_array() else {
        return false;
    };
    items.iter().all(|item| {
        item.as_object()
            .is_some_and(|object| object.contains_key(key))
    })
}

/// Whether every element of the raw array at `path` carries an object
/// `member` that itself carries `key` (both nullable-valued).
fn raw_each_nested_member_present(value: &Json, path: &[&str], member: &str, key: &str) -> bool {
    let Some((last, steps)) = path.split_last() else {
        return false;
    };
    let parent = match raw_at(value, steps) {
        Some(parent) => parent,
        // An absent optional ancestor holds vacuously.
        None => return true,
    };
    if parent.is_null() {
        return true;
    }
    let container = match parent.get(*last) {
        Some(container) => container,
        // An absent optional container holds vacuously: no element
        // can violate a per-element presence rule.
        None => return true,
    };
    if container.is_null() {
        return true;
    }
    let Some(items) = container.as_array() else {
        return false;
    };
    items.iter().all(|item| {
        item.as_object()
            .and_then(|object| object.get(member))
            .and_then(|nested| nested.as_object())
            .is_some_and(|nested| nested.contains_key(key))
    })
}

/// Walk object steps to the raw value at `path`.
fn raw_at<'a>(value: &'a Json, path: &[&str]) -> Option<&'a Json> {
    let mut current = value;
    for step in path {
        current = current.as_object().and_then(|object| object.get(*step))?;
    }
    Some(current)
}

/// The shared `pins` raw presence (X8/R7): the nullable-but-required
/// `lock`, `observed_index`, `adapter`, and `sources` members must be
/// carried, and a present `adapter` object must carry `digest`.
fn raw_pins_present(value: &Json) -> Result<(), WireError> {
    let pins_path: &[&str] = &["pins"];
    for key in ["model", "lock", "observed_index", "adapter", "sources"] {
        if !raw_member_present(value, &[pins_path[0], key]) {
            return Err(WireError::Shape);
        }
    }
    if let Some(adapter) = raw_at(value, &["pins", "adapter"]) {
        if !adapter.is_null()
            && !adapter
                .as_object()
                .is_some_and(|object| object.contains_key("digest"))
        {
            return Err(WireError::Shape);
        }
    }
    Ok(())
}

/// The shared write-entry raw presence (X8/R7): `before`, `after`, and
/// `content` are required-but-nullable on every write of a plan,
/// patch, or receipt.
fn raw_writes_present(value: &Json, array: &str) -> Result<(), WireError> {
    for key in ["path", "action", "before", "after", "content"] {
        if !raw_each_member_present(value, &[array], key) {
            return Err(WireError::Shape);
        }
    }
    Ok(())
}

impl ClosedValidate for AnalysisDocument {
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "project",
        "scope",
        "target",
        "profile",
        "pins",
        "module",
        "definitions",
        "bindings",
        "contracts",
        "unknowns",
        "ambiguities",
        "discrepancies",
    ];
    fn validate_raw(value: &Json) -> Result<(), WireError> {
        raw_pins_present(value)?;
        if !raw_member_present(value, &["module", "native_root"])
            || !raw_each_member_present(value, &["definitions"], "provenance")
            || !raw_each_nested_member_present(value, &["definitions"], "provenance", "native")
            || !raw_each_nested_member_present(value, &["definitions"], "provenance", "path")
            || !raw_each_nested_member_present(value, &["definitions"], "provenance", "line")
        {
            return Err(WireError::Shape);
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::ANALYSIS_IDENTITY)?;
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
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "plan",
        "project",
        "analysis",
        "selections",
        "writes",
        "pins",
        "unresolved",
        "rollback",
    ];
    fn validate_raw(value: &Json) -> Result<(), WireError> {
        raw_pins_present(value)?;
        raw_writes_present(value, "writes")?;
        for key in ["definitions", "binding"] {
            if !raw_member_present(value, &["selections", key]) {
                return Err(WireError::Shape);
            }
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::CONFIRMATION_PLAN_IDENTITY)?;
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
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "patch",
        "origin",
        "scope",
        "files",
    ];
    fn validate_raw(value: &Json) -> Result<(), WireError> {
        raw_writes_present(value, "files")?;
        for key in ["target", "profile"] {
            if !raw_member_present(value, &["scope", key]) {
                return Err(WireError::Shape);
            }
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::PATCH_IDENTITY)?;
        check_writes(&self.files, true)?;
        check_member(
            &self.origin.kind,
            &["supplied-reviewed", "adapter-generated"],
        )?;
        if !version::is_native_id(&self.origin.identity) {
            return Err(WireError::Shape);
        }
        if !version::is_semantic_id(&self.scope.symbol) {
            return Err(WireError::Shape);
        }
        if self.files.is_empty() {
            return Err(WireError::Shape);
        }
        Ok(())
    }
}

impl ClosedValidate for RefactorPlan {
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "plan",
        "project",
        "analysis",
        "selection",
        "patch",
        "verification",
        "recommendations",
        "unsupported",
        "read_scope",
        "mutation_scope",
        "preservation",
        "pins",
        "rollback",
    ];
    fn validate_raw(value: &Json) -> Result<(), WireError> {
        raw_pins_present(value)?;
        if !raw_each_member_present(value, &["recommendations"], "uncertainty") {
            return Err(WireError::Shape);
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::REFACTOR_PLAN_IDENTITY)?;
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
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "variant",
        "verification",
        "spec",
        "plan",
        "tool",
        "baseline_source",
        "checks",
        "confinement",
        "custody",
    ];
    fn validate_raw(value: &Json) -> Result<(), WireError> {
        // The nullable-but-nullable digest members are required keys
        // exactly like the schema demands (X8/R7): a missing `custody`
        // or `tool.digest` refuses even though the typed value would be
        // `None`.
        if let Some(tool) = raw_at(value, &["tool"]) {
            if !tool.is_null()
                && !tool
                    .as_object()
                    .is_some_and(|object| object.contains_key("digest"))
            {
                return Err(WireError::Shape);
            }
        }
        for side in ["baseline", "candidate"] {
            if !raw_each_member_present(value, &[side, "checks"], "exit") {
                return Err(WireError::Shape);
            }
        }
        if let Some(custody) = raw_at(value, &["custody"]) {
            if !custody.is_null()
                && !custody.as_object().is_some_and(|object| {
                    object.contains_key("run") && object.contains_key("measured_ms")
                })
            {
                return Err(WireError::Shape);
            }
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::VERIFICATION_IDENTITY)?;
        check_member(&self.variant, &KINDS_VARIANT)?;
        // The verification id is a bounded non-empty token ($defs/token),
        // never an empty string (X8/R7 parity).
        check_token(&self.verification)?;
        // The nullable digest members carry the exact sha256 grammar
        // whenever present (X8/R7).
        if let Some(spec) = &self.spec {
            check_digest_value(spec)?;
        }
        if let Some(plan) = &self.plan {
            check_digest_value(plan)?;
        }
        if let Some(baseline_source) = &self.baseline_source {
            check_digest_value(baseline_source)?;
        }
        if self.confinement.len() > 8 {
            return Err(WireError::Shape);
        }
        if self.checks.len() > version::MAX_CHECKS {
            return Err(WireError::Shape);
        }
        // Every check id is a bounded token, unique within the spec.
        let mut seen_ids = std::collections::BTreeSet::new();
        for check in &self.checks {
            check_member(&check.kind, &KINDS_CHECK)?;
            if !version::is_scope_id(&check.id) || !seen_ids.insert(check.id.as_str()) {
                return Err(WireError::Shape);
            }
            if check.expect_stdout.len() > 64
                || check
                    .expect_stdout
                    .iter()
                    .any(|expected| expected.len() > 1024)
            {
                return Err(WireError::Shape);
            }
        }
        for side in [&self.baseline, &self.candidate].into_iter().flatten() {
            if side.checks.len() > version::MAX_CHECKS {
                return Err(WireError::Shape);
            }
            for outcome in &side.checks {
                check_member(&outcome.status, &KINDS_OUTCOME_STATUS)?;
                // The schema's check-outcome bounds: a bounded non-empty
                // id token, exit within 0..=255, and a bounded detail
                // (X8/R7 parity).
                check_token(&outcome.id)?;
                if outcome.detail.len() > 1024 || outcome.exit.is_some_and(|exit| exit > 255) {
                    return Err(WireError::Shape);
                }
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
            if tool.program.is_empty() || tool.program.len() > 256 {
                return Err(WireError::Shape);
            }
            if tool.args.len() > 64 || tool.args.iter().any(|arg| arg.len() > 256) {
                return Err(WireError::Shape);
            }
        }
        if let Some(custody) = &self.custody {
            if !version::is_sha256(&custody.run)
                || custody.measured_ms == 0
                || custody.measured_ms > 60_000_000
            {
                return Err(WireError::Shape);
            }
        }
        Ok(())
    }
}

impl ClosedValidate for AdoptionReceipt {
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "receipt",
        "phase",
        "project",
        "plan",
        "outcome",
        "writes",
        "journal",
        "snapshots",
        "verification",
        "pins",
        "analysis",
    ];
    fn validate_raw(value: &Json) -> Result<(), WireError> {
        raw_pins_present(value)?;
        raw_writes_present(value, "writes")?;
        Ok(())
    }
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::RECEIPT_IDENTITY)?;
        check_member(&self.phase, &KINDS_PHASE)?;
        check_member(&self.outcome, &KINDS_RECEIPT_OUTCOME)?;
        check_writes(&self.writes, true)?;
        check_pins(&self.pins)?;
        Ok(())
    }
}

impl ClosedValidate for CoverageDocument {
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "project",
        "scope",
        "revision",
        "pins",
        "rows",
        "counts",
        "gaps",
    ];
    fn validate_raw(value: &Json) -> Result<(), WireError> {
        // A present `pins` object follows the shared pin presence; the
        // row members are required-but-nullable exactly like the
        // schema's rows definition (X8/R7).
        if let Some(pins) = raw_at(value, &["pins"]) {
            if !pins.is_null() {
                let wrapped = serde_json::json!({ "pins": pins.clone() });
                raw_pins_present(&wrapped)?;
            }
        }
        for key in ["origin", "contract", "binding", "verification"] {
            if !raw_each_member_present(value, &["rows"], key) {
                return Err(WireError::Shape);
            }
        }
        Ok(())
    }
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::COVERAGE_IDENTITY)?;
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
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "upstream",
        "coverage",
        "panels",
        "gaps",
    ];
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::STATUS_REPORT_IDENTITY)?;
        for panel in &self.panels {
            if !version::is_semantic_id(&panel.symbol) {
                return Err(WireError::Shape);
            }
            check_member(&panel.mode, &KINDS_EFFECTIVE)?;
            check_member(&panel.state, &KINDS_ROW_STATE)?;
            check_member(
                &panel.freshness,
                &["fresh", "unqualified", "stale", "unknown"],
            )?;
        }
        Ok(())
    }
}

impl ClosedValidate for ImpactReport {
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "upstream",
        "coverage",
        "boundaries",
        "gaps",
    ];
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::IMPACT_REPORT_IDENTITY)?;
        for boundary in &self.boundaries {
            if !version::is_semantic_id(&boundary.symbol) {
                return Err(WireError::Shape);
            }
            check_member(&boundary.mode, &KINDS_EFFECTIVE)?;
            check_member(
                &boundary.freshness,
                &["fresh", "unqualified", "stale", "unknown"],
            )?;
        }
        Ok(())
    }
}

impl ClosedValidate for TraceReport {
    const REQUIRED_KEYS: &'static [&'static str] = &[
        "schema_version",
        "identity",
        "manifest",
        "query",
        "coverage",
        "verification",
        "uncovered",
        "stale",
    ];
    fn validate(&self) -> Result<(), WireError> {
        check_identity(&self.identity, version::TRACE_REPORT_IDENTITY)?;
        for digest in &self.verification {
            if !version::is_sha256(digest) {
                return Err(WireError::Shape);
            }
        }
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
    /// The deterministic whole-tree state digest at plan time (F3/R2):
    /// the complete consumed input closure — every project file except
    /// VCS, build, and lekalo runtime homes. Verify and apply re-derive
    /// it before any run and before installation, so a drifted tree —
    /// including files an argv-pinned runner only reaches through its
    /// own loader — never executes or installs on stale evidence.
    #[serde(default)]
    pub tree: Option<String>,
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
    /// refuses an execution that lacks it. Specifications carry the
    /// typed `null` (the closed member is required while nullable —
    /// X8/R7 parity: emitted documents always spell the member).
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
    /// The producing scan revision the join was derived from (typed
    /// null without scan evidence) — the first freshness input (X11).
    pub revision: Option<String>,
    /// The exact consumed input pins of the join (typed null for a
    /// hand-authored document): a consumer can tell a fresh panel from
    /// a stale one by re-fingerprinting this set (X11).
    pub pins: Option<Pins>,
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
    /// The typed freshness qualification of the underlying coverage
    /// join, identical to the status panels' axis: a witness never
    /// presents a stale attachment as plain current (X11).
    pub freshness: String,
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
    ($name:ident, $ty:ty, $version:expr, $keys:expr) => {
        /// Decode one closed document from raw bytes: the raw closed
        /// value is presence-checked for every required-but-nullable
        /// member (X8/R7) before typed deserialization, then the typed
        /// document runs the closed vocabulary and bounds.
        pub fn $name(bytes: &[u8]) -> Result<$ty, WireError> {
            let value = decode_value(bytes, $version, $keys)?;
            <$ty>::validate_raw(&value)?;
            let document: $ty = serde_json::from_value(value).map_err(|_| WireError::Shape)?;
            ClosedValidate::validate(&document)?;
            Ok(document)
        }
    };
}

decode_entry!(
    analysis_of_bytes,
    AnalysisDocument,
    version::ANALYSIS_SCHEMA_VERSION,
    AnalysisDocument::REQUIRED_KEYS
);
decode_entry!(
    confirmation_plan_of_bytes,
    ConfirmationPlan,
    version::CONFIRMATION_PLAN_SCHEMA_VERSION,
    ConfirmationPlan::REQUIRED_KEYS
);
decode_entry!(
    patch_of_bytes,
    PatchDocument,
    version::PATCH_SCHEMA_VERSION,
    PatchDocument::REQUIRED_KEYS
);
decode_entry!(
    refactor_plan_of_bytes,
    RefactorPlan,
    version::REFACTOR_PLAN_SCHEMA_VERSION,
    RefactorPlan::REQUIRED_KEYS
);
decode_entry!(
    verification_of_bytes,
    VerificationDocument,
    version::VERIFICATION_SCHEMA_VERSION,
    VerificationDocument::REQUIRED_KEYS
);
decode_entry!(
    execution_of_bytes,
    VerificationDocument,
    version::VERIFICATION_SCHEMA_VERSION,
    VerificationDocument::REQUIRED_KEYS
);
decode_entry!(
    receipt_of_bytes,
    AdoptionReceipt,
    version::RECEIPT_SCHEMA_VERSION,
    AdoptionReceipt::REQUIRED_KEYS
);
decode_entry!(
    coverage_of_bytes,
    CoverageDocument,
    version::COVERAGE_SCHEMA_VERSION,
    CoverageDocument::REQUIRED_KEYS
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

#[cfg(test)]
mod parity_tests {
    use super::*;

    /// The complete valid specification skeleton every probe mutates:
    /// every required-but-nullable member spelled, exactly like the
    /// Ajv schemas demand.
    fn spec_skeleton(extra: &str) -> String {
        format!(
            r#"{{"schema_version":"lekalo/adoption-verification/v0.8.0","identity":"dev.lekalo.adoption-verification@0.8.0","variant":"specification","verification":"x","spec":null,"plan":null,"tool":{{"program":"node","args":[],"digest":null}},"baseline_source":null,"checks":[],"confinement":["bounded-output"],"baseline":null,"candidate":null,"oracle":null,"conclusion":null,"custody":null{extra}}}"#
        )
    }

    /// X8: the closed Rust admission is schema-equivalent for the
    /// required-key classes — a missing `tool`, `baseline_source`, or
    /// `custody` key decodes as a Shape error exactly like the Ajv
    /// schema's `required` list, and foreign identities stay
    /// version-refusals.
    #[test]
    fn missing_required_option_keys_are_shape_errors() {
        let base = br#"{
            "schema_version": "lekalo/adoption-verification/v0.8.0",
            "identity": "dev.lekalo.adoption-verification@0.8.0",
            "variant": "specification",
            "verification": "x",
            "spec": null,
            "plan": null,
            "checks": [],
            "confinement": ["bounded-output"]
        }"#;
        // tool/baseline_source/baseline/candidate/oracle/conclusion/custody all
        // missing: the raw-object required-key check refuses before the
        // typed deserialization ever sees the document.
        assert_eq!(
            verification_of_bytes(base).unwrap_err(),
            WireError::Shape,
            "missing required keys must refuse"
        );
    }

    #[test]
    fn complete_document_with_nulls_decodes() {
        let bytes = spec_skeleton("").into_bytes();
        let document = verification_of_bytes(&bytes).expect("decodes");
        assert_eq!(document.variant, "specification");
        assert!(document.tool.is_some());
        assert!(document.custody.is_none());
    }

    /// A minimal refactor plan whose pin set omits the
    /// required-but-nullable 'lock' member (R7).
    fn pins_absent_plan() -> String {
        r#"{"schema_version":"lekalo/adoption-refactor-plan/v0.8.0","identity":"dev.lekalo.adoption-refactor-plan@0.8.0","plan":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","project":"taskboard","analysis":"sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","selection":{"kind":"symbol","id":"taskboard.task"},"patch":null,"verification":null,"recommendations":[],"unsupported":[],"read_scope":[],"mutation_scope":[],"preservation":[],"pins":{"model":"sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","sources":[]},"rollback":{"journal":"","snapshots":""}}"#.to_owned()
    }

    /// A minimal patch whose single write omits the
    /// required-but-nullable 'before' member (R7).
    fn write_member_absent_patch() -> String {
        r#"{"schema_version":"lekalo/adoption-patch/v0.8.0","identity":"dev.lekalo.adoption-patch@0.8.0","patch":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","origin":{"kind":"supplied-reviewed","identity":"probe"},"scope":{"symbol":"taskboard.task","target":null,"profile":null},"files":[{"path":"src/a.mjs","action":"create","after":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","content":"x"}]}"#.to_owned()
    }

    /// R7: every nullable-but-required member admits absence on the
    /// Rust side no longer — the decoder and the schema agree in both
    /// directions.
    #[test]
    fn nullable_but_required_members_refuse_absence() {
        // (document, is-verification?) pairs: each mutation removes one
        // member the closed 0.8.0 schemas require while allowing null.
        let cases: Vec<(String, bool)> = vec![
            (
                spec_skeleton("").replacen(r#","custody":null"#, "", 1),
                true,
            ),
            (
                spec_skeleton("").replacen(r#""args":[],"digest":null}"#, r#""args":[]}"#, 1),
                true,
            ),
            (
                spec_skeleton(
                    r#","baseline":{"checks":[{"id":"x","status":"passed","detail":"ok"}]}"#,
                )
                .replacen(r#","baseline":null"#, "", 1),
                true,
            ),
            (pins_absent_plan(), false),
            (write_member_absent_patch(), false),
        ];
        for (document, verification_family) in cases {
            let bytes = document.as_bytes();
            let error = if verification_family {
                verification_of_bytes(bytes).unwrap_err()
            } else if document.contains("adoption-refactor-plan") {
                refactor_plan_of_bytes(bytes).unwrap_err()
            } else {
                patch_of_bytes(bytes).unwrap_err()
            };
            assert_eq!(error, WireError::Shape, "absence must refuse: {document}");
        }
    }

    /// R7: the digest grammars and token bounds the schema spells are
    /// enforced on the Rust side too — a non-digest `spec`/`plan`/`baseline_source`
    /// and an empty `verification` token refuse.
    #[test]
    fn digest_grammars_and_token_bounds_match_the_schema() {
        for (name, from, to) in [
            (
                "spec-not-a-digest",
                r#""spec":null"#,
                r#""spec":"not-a-digest""#,
            ),
            (
                "plan-not-a-digest",
                r#""plan":null"#,
                r#""plan":"not-a-digest""#,
            ),
            (
                "baseline-not-a-digest",
                r#""baseline_source":null"#,
                r#""baseline_source":"not-a-digest""#,
            ),
            (
                "empty-verification",
                r#""verification":"x""#,
                "\"verification\":\"\"",
            ),
        ] {
            let document = spec_skeleton("").replace(from, to);
            let parsed = verification_of_bytes(document.as_bytes());
            let error = match parsed {
                Err(error) => error,
                Ok(_) => panic!("{name} unexpectedly decoded: {document}"),
            };
            assert_eq!(error, WireError::Shape, "{name} must refuse");
        }
    }

    /// R7: emitted documents always spell the required-but-nullable
    /// members, so a spec round-trips through the closed decoder.
    #[test]
    fn emitted_specifications_carry_the_required_null_members() {
        let document = VerificationDocument {
            schema_version: version::VERIFICATION_SCHEMA_VERSION.to_owned(),
            identity: version::VERIFICATION_IDENTITY.to_owned(),
            variant: "specification".to_owned(),
            verification: "x".to_owned(),
            spec: None,
            plan: None,
            tool: None,
            baseline_source: None,
            checks: Vec::new(),
            confinement: vec!["bounded-output".to_owned()],
            baseline: None,
            candidate: None,
            oracle: None,
            conclusion: None,
            custody: None,
        };
        let bytes = serde_json::to_vec(&document).expect("serializes");
        let text = String::from_utf8(bytes.clone()).expect("utf8");
        assert!(text.contains(r#""custody":null"#), "custody is spelled");
        verification_of_bytes(&bytes).expect("round-trips");
    }
}
