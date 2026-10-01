//! Observation validation, normalization, and record construction
//! (issue #121).
//!
//! The pipeline is fail-closed in one direction: raw observation bytes
//! are duplicate-key-scanned, structurally decoded into the closed
//! typed model, grammar-checked field by field (the safe grammars
//! cannot express absolute paths, URLs, or free-form text), bound to
//! the embedded diagnostic registry, cross-checked (token totals,
//! coverage denominators, source bindings, status contradictions), and
//! only then normalized into the canonical record shape with the exact
//! frozen #120 policy and authority references. Every refusal carries a
//! fixed detail token; no rejected value is ever echoed.

use std::collections::BTreeMap;
use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value as Json;

use super::clock::{self, Instant};
use super::types::{
    AssertionSet, AssertionsRef, AuthorityRef, ClassificationRef, Comparability, CoreProvenance,
    CoreProvenanceIn, CountIn, CoverageState, DiagnosticSeverity, DiagnosticSummary, GateSummary,
    GitProvenance, GitProvenanceIn, HarnessProvenance, HarnessProvenanceIn, LockProvenance,
    LockProvenanceIn, MeasurementField, Metrics, ModelProvenance, ModelProvenanceIn, Observation,
    Outcome, PolicyRef, PrivacyBlock, PrivacyOrigin, PrivacyProvenance, ProfileProvenance,
    ProfileProvenanceIn, Provenance, RecordScope, RepeatLink, RunRecord, SetScope, Tokens,
};
use super::value::Vs;
use super::version;
use crate::diagnostics::registry::DiagnosticRegistry;
use crate::digest::sha256_hex;
use crate::privacy::canonical;
use crate::privacy::refs;

/// One fixed-token validation refusal. `code` selects the stable
/// diagnostic (`history.input-invalid` or `history.unsafe-field` or
/// `history.version-unsupported`); `detail` is a closed token that
/// never quotes the rejected value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Violation {
    pub(crate) code: &'static str,
    pub(crate) detail: &'static str,
}

impl Violation {
    const fn new(code: &'static str, detail: &'static str) -> Self {
        Self { code, detail }
    }

    /// A closed-contract or semantic-rule violation.
    const fn invalid(detail: &'static str) -> Self {
        Self::new(super::codes::INPUT_INVALID, detail)
    }

    /// A grammar violation (the safe-field guard).
    const fn unsafe_field(detail: &'static str) -> Self {
        Self::new(super::codes::UNSAFE_FIELD, detail)
    }

    /// A version outside the accepted registry.
    const fn version(detail: &'static str) -> Self {
        Self::new(super::codes::VERSION_UNSUPPORTED, detail)
    }
}

/// The exact authority digest with the digest-algorithm prefix. The
/// frozen bare-hex constant lives in [`refs::AUTHORITY_RAW_SHA256`].
const AUTHORITY_DIGEST_REF: &str =
    "sha256:7ae6454ea20f7b61202d368411ef9bff4e70af96f1f2a408c209d84fe9722f80";

/// Duplicate-key-aware raw JSON scan: a parsed-value representation
/// cannot see duplicate object keys, so the bytes are scanned first and
/// any duplicate key refuses the whole document.
pub(crate) fn parse_no_duplicate_keys(bytes: &[u8]) -> Result<Json, Violation> {
    let text = std::str::from_utf8(bytes).map_err(|_| Violation::invalid("encoding"))?;
    let mut parser = RawParser {
        bytes: text.as_bytes(),
        position: 0,
    };
    parser.skip_whitespace();
    let value = parser.parse_value(0)?;
    parser.skip_whitespace();
    if parser.position != parser.bytes.len() {
        return Err(Violation::invalid("encoding"));
    }
    Ok(value)
}

/// One recursive-descent JSON value scanner with per-object duplicate
/// detection. Bounds: nesting depth and document length are already
/// bounded by the caller.
struct RawParser<'a> {
    bytes: &'a [u8],
    position: usize,
}

const MAX_DEPTH: usize = 64;

impl<'a> RawParser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.position += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), Violation> {
        if self.peek() == Some(byte) {
            self.position += 1;
            Ok(())
        } else {
            Err(Violation::invalid("encoding"))
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<Json, Violation> {
        if depth > MAX_DEPTH {
            return Err(Violation::invalid("bound"));
        }
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.parse_object(depth),
            Some(b'[') => self.parse_array(depth),
            Some(b'"') => Ok(Json::String(self.parse_string()?)),
            Some(b't') => self.parse_literal("true", Json::Bool(true)),
            Some(b'f') => self.parse_literal("false", Json::Bool(false)),
            Some(b'n') => self.parse_literal("null", Json::Null),
            Some(_) => self.parse_number(),
            None => Err(Violation::invalid("encoding")),
        }
    }

    fn parse_literal(&mut self, text: &str, value: Json) -> Result<Json, Violation> {
        if self.bytes[self.position..].starts_with(text.as_bytes()) {
            self.position += text.len();
            Ok(value)
        } else {
            Err(Violation::invalid("encoding"))
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<Json, Violation> {
        self.expect(b'{')?;
        let mut map = serde_json::Map::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.position += 1;
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            if map.contains_key(&key) {
                return Err(Violation::invalid("duplicate-key"));
            }
            self.skip_whitespace();
            self.expect(b':')?;
            let value = self.parse_value(depth + 1)?;
            map.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    return Ok(Json::Object(map));
                }
                _ => return Err(Violation::invalid("encoding")),
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<Json, Violation> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Ok(Json::Array(items));
        }
        loop {
            let value = self.parse_value(depth + 1)?;
            items.push(value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(Violation::invalid("encoding")),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, Violation> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(Violation::invalid("encoding")),
                Some(b'"') => {
                    self.position += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.position += 1;
                    let escape = self.peek().ok_or(Violation::invalid("encoding"))?;
                    self.position += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hex = self
                                .bytes
                                .get(self.position..self.position + 4)
                                .ok_or(Violation::invalid("encoding"))?;
                            let code = u32::from_str_radix(
                                std::str::from_utf8(hex)
                                    .map_err(|_| Violation::invalid("encoding"))?,
                                16,
                            )
                            .map_err(|_| Violation::invalid("encoding"))?;
                            self.position += 4;
                            // Surrogate pairs decode through the char::REPLACEMENT
                            //_CHARACTER path of serde_json's own rules: a lone
                            // surrogate is invalid, a pair combines.
                            if (0xD800..0xDC00).contains(&code) {
                                if self.peek() != Some(b'\\') {
                                    return Err(Violation::invalid("encoding"));
                                }
                                self.position += 1;
                                if self.peek() != Some(b'u') {
                                    return Err(Violation::invalid("encoding"));
                                }
                                self.position += 1;
                                let low_hex = self
                                    .bytes
                                    .get(self.position..self.position + 4)
                                    .ok_or(Violation::invalid("encoding"))?;
                                let low = u32::from_str_radix(
                                    std::str::from_utf8(low_hex)
                                        .map_err(|_| Violation::invalid("encoding"))?,
                                    16,
                                )
                                .map_err(|_| Violation::invalid("encoding"))?;
                                self.position += 4;
                                if !(0xDC00..0xE000).contains(&low) {
                                    return Err(Violation::invalid("encoding"));
                                }
                                let combined = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                                out.push(
                                    char::from_u32(combined)
                                        .ok_or(Violation::invalid("encoding"))?,
                                );
                            } else if (0xDC00..0xE000).contains(&code) {
                                return Err(Violation::invalid("encoding"));
                            } else {
                                out.push(
                                    char::from_u32(code).ok_or(Violation::invalid("encoding"))?,
                                );
                            }
                        }
                        _ => return Err(Violation::invalid("encoding")),
                    }
                }
                Some(byte) if byte < 0x20 => return Err(Violation::invalid("encoding")),
                Some(_) => {
                    // Consume one UTF-8 encoded scalar.
                    let rest = &self.bytes[self.position..];
                    let text =
                        std::str::from_utf8(rest).map_err(|_| Violation::invalid("encoding"))?;
                    let ch = text.chars().next().ok_or(Violation::invalid("encoding"))?;
                    if ch == '"' || ch == '\\' {
                        return Err(Violation::invalid("encoding"));
                    }
                    out.push(ch);
                    self.position += ch.len_utf8();
                }
            }
        }
    }

    fn parse_number(&mut self) -> Result<Json, Violation> {
        let start = self.position;
        if self.peek() == Some(b'-') {
            self.position += 1;
        }
        let mut any = false;
        while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
            self.position += 1;
            any = true;
        }
        if !any {
            return Err(Violation::invalid("encoding"));
        }
        if self.peek() == Some(b'.') {
            self.position += 1;
            let mut fraction = false;
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.position += 1;
                fraction = true;
            }
            if !fraction {
                return Err(Violation::invalid("encoding"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.position += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.position += 1;
            }
            let mut exponent = false;
            while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                self.position += 1;
                exponent = true;
            }
            if !exponent {
                return Err(Violation::invalid("encoding"));
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|_| Violation::invalid("encoding"))?;
        serde_json::from_str(text).map_err(|_| Violation::invalid("encoding"))
    }
}

/// Decode the closed observation envelope.
pub(crate) fn parse_observation(bytes: &[u8]) -> Result<Observation, Violation> {
    let value = parse_no_duplicate_keys(bytes)?;
    let observation = Observation::deserialize(&value).map_err(|_| Violation::invalid("shape"))?;
    if observation.schema_version != version::OBSERVATION_SCHEMA_VERSION
        || observation.identity != version::OBSERVATION_IDENTITY
    {
        return Err(Violation::version("observation-identity"));
    }
    Ok(observation)
}

// ---------------------------------------------------------------------------
// Safe field grammars
// ---------------------------------------------------------------------------

/// A lowercase bounded identifier token: the only spelling free-form
/// ids may take. It cannot contain separators of paths (`/`, `\`),
/// schemes (`:`), userinfo (`@`), whitespace, quotes, or control
/// bytes, so absolute paths, URLs, and raw text are not expressible.
pub(crate) fn is_token(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > 128 {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    bytes[1..].iter().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    })
}

/// A stable semantic id: dotted lowercase segments.
pub(crate) fn is_semantic_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > 256 {
        return false;
    }
    let mut segment_start = true;
    let mut segments = 0usize;
    let mut dots = 0usize;
    for byte in bytes {
        match byte {
            b'.' => {
                if segment_start {
                    return false;
                }
                segment_start = true;
                dots += 1;
            }
            b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' => {
                if segment_start {
                    if matches!(byte, b'0'..=b'9' | b'_' | b'-') {
                        return false;
                    }
                    segments += 1;
                }
                segment_start = false;
            }
            _ => return false,
        }
    }
    !segment_start && dots >= 1 && segments >= 2
}

/// A `sha256:<64 lowercase hex>` digest.
pub(crate) fn is_sha256(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 71
        && bytes.starts_with(b"sha256:")
        && bytes[7..]
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// A full Git commit id (SHA-1 or SHA-256 spelling).
pub(crate) fn is_git_commit(text: &str) -> bool {
    let bytes = text.as_bytes();
    (bytes.len() == 40 || bytes.len() == 64)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// A strict semver spelling (no leading zeroes, optional prerelease).
pub(crate) fn is_semver(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    let core_end = bytes.iter().position(|byte| *byte == b'-');
    let (core, prerelease) = match core_end {
        Some(index) => (&bytes[..index], Some(&bytes[index + 1..])),
        None => (bytes, None),
    };
    let mut parts = core.split(|byte| *byte == b'.');
    for _ in 0..3 {
        let Some(part) = parts.next() else {
            return false;
        };
        if !is_numeric_part(part, false) {
            return false;
        }
    }
    if parts.next().is_some() {
        return false;
    }
    match prerelease {
        None => true,
        Some(pre) => {
            !pre.is_empty()
                && pre
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
        }
    }
}

fn is_numeric_part(part: &[u8], allow_zero_alone: bool) -> bool {
    if part.is_empty() {
        return false;
    }
    if part.iter().any(|byte| !byte.is_ascii_digit()) {
        return false;
    }
    if part.len() > 1 && part[0] == b'0' {
        return allow_zero_alone && part == b"0";
    }
    true
}

/// A nonnegative decimal string with at most 12 fractional digits.
pub(crate) fn is_decimal(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() || bytes.len() > 40 {
        return false;
    }
    let (integral, fractional) = match bytes.iter().position(|byte| *byte == b'.') {
        Some(index) => (&bytes[..index], Some(&bytes[index + 1..])),
        None => (bytes, None),
    };
    if !is_numeric_part(integral, false) {
        return false;
    }
    match fractional {
        None => true,
        Some(fraction) => {
            !fraction.is_empty()
                && fraction.len() <= 12
                && fraction.iter().all(|byte| byte.is_ascii_digit())
        }
    }
}

/// A coverage ratio in the closed `[0, 1]` decimal spelling.
pub(crate) fn is_ratio(text: &str) -> bool {
    if !is_decimal(text) {
        return false;
    }
    let integral = text.split('.').next().unwrap_or(text);
    integral == "0" || integral == "1"
}

/// An ISO 4217-style uppercase currency token.
pub(crate) fn is_currency(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 3 && bytes.iter().all(|byte| byte.is_ascii_uppercase())
}

/// The conservative secret-material scan over every bounded token.
/// Tokens are structurally safe already; this catches encoded blobs and
/// credential-looking spellings even when they fit the grammar.
pub(crate) fn token_has_secret_material(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    for marker in [
        "secret",
        "password",
        "private-key",
        "apikey",
        "api-key",
        "bearer",
        "credential",
        "begin-",
    ] {
        if lowered.contains(marker) {
            return true;
        }
    }
    is_jwt_shape(text)
}

/// A `header.payload.signature` base64url blob shape.
fn is_jwt_shape(text: &str) -> bool {
    let mut segments = text.split('.');
    let first = segments.next().unwrap_or_default();
    let second = segments.next().unwrap_or_default();
    let third = segments.next().unwrap_or_default();
    if segments.next().is_some() {
        return false;
    }
    let blob = |segment: &str| {
        segment.len() >= 8
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    };
    blob(first) && blob(second) && blob(third)
}

/// Check one bounded token leaf of the record.
fn check_token(text: &str) -> Result<(), Violation> {
    if !is_token(text) {
        return Err(Violation::unsafe_field("token"));
    }
    if token_has_secret_material(text) {
        return Err(Violation::unsafe_field("secret-material"));
    }
    Ok(())
}

/// Check one required `Vs<String>` leaf against its safe grammar.
fn check_vs_required(
    text: &Vs<String>,
    grammar: fn(&str) -> bool,
    detail: &'static str,
) -> Result<(), Violation> {
    if let Vs::Known(value) = text {
        if !grammar(value) {
            return Err(Violation::unsafe_field(detail));
        }
    }
    Ok(())
}

/// Check one optional `Vs<String>` leaf against its safe grammar.
fn check_vs(
    text: &Option<Vs<String>>,
    grammar: fn(&str) -> bool,
    detail: &'static str,
) -> Result<(), Violation> {
    if let Some(Vs::Known(value)) = text {
        if !grammar(value) {
            return Err(Violation::unsafe_field(detail));
        }
    }
    Ok(())
}

/// Check one optional `Vs<String>` leaf that must carry a safe token
/// spelling, including the conservative secret-material scan.
fn check_vs_token(text: &Option<Vs<String>>) -> Result<(), Violation> {
    if let Some(Vs::Known(value)) = text {
        check_token(value)?;
    }
    Ok(())
}

/// A known u64 stays inside the JSON-safe integer range.
fn check_u64(value: u64) -> Result<(), Violation> {
    if value > 9_007_199_254_740_991 {
        return Err(Violation::invalid("bound"));
    }
    Ok(())
}

/// Check one `Option<Vs<u64>>` leaf inside the JSON-safe range.
fn check_count(value: &CountIn) -> Result<(), Violation> {
    if let Some(Vs::Known(value)) = value {
        check_u64(*value)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Exhaustive source-outcome mapping
// ---------------------------------------------------------------------------

/// The exhaustive source-outcome mapping. The original spelling stays
/// in the summary; the mapped verdict feeds the record status. A
/// degraded run is a warn with incomplete coverage; blocked and
/// security verdicts fail; missing evidence is infrastructure; an
/// unsupported run stays unsupported and can never pass.
pub(crate) fn map_source_outcome(outcome: super::types::SourceOutcome) -> (Outcome, CoverageState) {
    use super::types::SourceOutcome as Source;
    match outcome {
        Source::Passed => (Outcome::Pass, CoverageState::Complete),
        Source::Failed => (Outcome::Fail, CoverageState::Complete),
        Source::Degraded => (Outcome::Warn, CoverageState::Incomplete),
        Source::Blocked => (Outcome::Fail, CoverageState::Incomplete),
        Source::Security => (Outcome::Fail, CoverageState::Incomplete),
        Source::Missing => (Outcome::Infrastructure, CoverageState::Unknown),
        Source::Unsupported => (Outcome::Unsupported, CoverageState::Incomplete),
        Source::Infrastructure => (Outcome::Infrastructure, CoverageState::Incomplete),
    }
}

// ---------------------------------------------------------------------------
// Full observation validation
// ---------------------------------------------------------------------------

/// Validate the whole observation against the closed semantic rules and
/// return the internally consistent pieces the record build needs.
///
/// The `known_metric` closure reports whether the measurement source's
/// target leaf carries a known value in the normalized metrics block.
pub(crate) fn validate_observation(observation: &Observation) -> Result<(), Violation> {
    validate_run_id(observation.run_id.as_deref())?;
    clock::parse(&observation.timestamp).ok_or(Violation::invalid("timestamp"))?;
    if crate::privacy::vocab::DataSensitivity::parse(&observation.data_sensitivity).is_none() {
        return Err(Violation::invalid("sensitivity"));
    }

    // Semantic ids: bounded, grammar-safe, duplicate-free.
    let mut semantic_ids = HashSet::new();
    for id in &observation.operation.affected_semantic_ids {
        if !is_semantic_id(id) {
            return Err(Violation::unsafe_field("semantic-id"));
        }
        if !semantic_ids.insert(id.as_str()) {
            return Err(Violation::invalid("duplicate-semantic-id"));
        }
    }

    // Provenance leaves: grammar-check the known values only.
    validate_provenance(&observation.provenance)?;

    // Metrics: bounds, decimals, ratios, denominators, token totals.
    validate_metrics(&observation.metrics)?;

    // Measurement sources: unique per field, bound to a known leaf.
    let mut fields = HashSet::new();
    for source in &observation.measurement_sources {
        if !fields.insert(source.field) {
            return Err(Violation::invalid("duplicate-source-field"));
        }
        check_token(&source.source_id)?;
        check_vs_required(&source.source_version, is_semver, "semver")?;
        if !source_targets_known_metric(source.field, &observation.metrics) {
            return Err(Violation::invalid("unknown-metric-source"));
        }
    }

    // Gate summaries: unique ids, bounded counts, mapped verdicts.
    let mut summary_ids = HashSet::new();
    for summary in &observation.test_gate_summaries {
        if !summary_ids.insert((summary.kind, summary.id.as_str())) {
            return Err(Violation::invalid("duplicate-summary-id"));
        }
        check_token(&summary.id)?;
        for count in [
            &summary.passed,
            &summary.failed,
            &summary.unsupported,
            &summary.infrastructure,
        ] {
            check_count(count)?;
        }
        if summary
            .evidence_ref
            .as_deref()
            .is_some_and(|reference| !is_hex_token32(reference))
        {
            return Err(Violation::unsafe_field("token"));
        }
    }

    // Diagnostics: registered stable codes only, registry severity,
    // positive counts. Details/spans/messages are not representable.
    let registry = DiagnosticRegistry::embedded().map_err(|_| Violation::invalid("registry"))?;
    for diagnostic in &observation.diagnostics {
        let Some(entry) = registry.entry(&diagnostic.code) else {
            return Err(Violation::invalid("unregistered-code"));
        };
        if entry.lifecycle() != crate::diagnostics::registry::Lifecycle::Active {
            return Err(Violation::invalid("unregistered-code"));
        }
        let expected = match entry.default_severity() {
            crate::diagnostics::types::Severity::Info => DiagnosticSeverity::Info,
            crate::diagnostics::types::Severity::Warning => DiagnosticSeverity::Warning,
            crate::diagnostics::types::Severity::Error => DiagnosticSeverity::Error,
        };
        if diagnostic.severity != expected {
            return Err(Violation::invalid("severity-mismatch"));
        }
        check_u64(diagnostic.count)?;
    }

    // Assertions: unique ids, grammar-safe subjects, safe refs.
    if let Some(assertions) = &observation.assertions {
        let mut assertion_ids = HashSet::new();
        for row in &assertions.rows {
            if !assertion_ids.insert(row.assertion_id.as_str()) {
                return Err(Violation::invalid("duplicate-assertion-id"));
            }
            check_token(&row.assertion_id)?;
            if let Some(subject) = &row.subject_semantic_id {
                if !is_semantic_id(subject) {
                    return Err(Violation::unsafe_field("semantic-id"));
                }
            }
            if row
                .evidence_ref
                .as_deref()
                .is_some_and(|reference| !is_hex_token32(reference))
            {
                return Err(Violation::unsafe_field("token"));
            }
        }
    }

    // Repeat parent: opaque id grammar (liveness is a store check).
    if let Some(parent) = &observation.repeat_parent_run_id {
        if !is_hex_token32(parent) {
            return Err(Violation::unsafe_field("token"));
        }
    }

    // Status contradictions: a passing run can never carry a mapped
    // failing or infrastructure summary; an unsupported run can never
    // carry a passing one.
    for summary in &observation.test_gate_summaries {
        let (mapped, _) = map_source_outcome(summary.source_outcome);
        let contradiction = match observation.status.outcome {
            Outcome::Pass => matches!(mapped, Outcome::Fail | Outcome::Infrastructure),
            Outcome::Unsupported => matches!(mapped, Outcome::Pass | Outcome::Warn),
            _ => false,
        };
        if contradiction {
            return Err(Violation::invalid("contradiction"));
        }
    }

    Ok(())
}

/// The opaque 32-hex run/scope token grammar.
pub(crate) fn is_hex_token32(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 32
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Validate a caller-supplied run id (the exactly-once retry seam).
pub(crate) fn validate_run_id(run_id: Option<&str>) -> Result<(), Violation> {
    if let Some(run_id) = run_id {
        if !is_hex_token32(run_id) {
            return Err(Violation::unsafe_field("token"));
        }
    }
    Ok(())
}

fn validate_provenance(provenance: &super::types::ProvenanceIn) -> Result<(), Violation> {
    if let Some(git) = &provenance.git {
        check_vs(&git.commit, is_git_commit, "commit")?;
        // Any known boolean is a safe dirty flag.
        let _ = &git.dirty;
        check_vs(&git.working_set_digest, is_sha256, "sha256")?;
    }
    if let Some(model) = &provenance.model {
        check_vs_token(&model.revision)?;
        check_vs(&model.digest, is_sha256, "sha256")?;
        check_vs(&model.ir_digest, is_sha256, "sha256")?;
    }
    if let Some(lock) = &provenance.lock {
        check_vs(&lock.version, is_semver, "semver")?;
        check_vs(&lock.digest, is_sha256, "sha256")?;
    }
    if let Some(core) = &provenance.core {
        check_vs(&core.version, is_semver, "semver")?;
        check_vs_token(&core.build_revision)?;
        check_vs(&core.build_digest, is_sha256, "sha256")?;
    }
    for adapter in &provenance.adapters {
        check_token(&adapter.id)?;
        check_vs_required(&adapter.version, is_semver, "semver")?;
        check_vs_required(&adapter.manifest_digest, is_sha256, "sha256")?;
        check_vs_required(&adapter.bundle_digest, is_sha256, "sha256")?;
    }
    if let Some(profile) = &provenance.profile {
        check_vs_token(&profile.id)?;
        check_vs(&profile.version, is_semver, "semver")?;
        check_vs(&profile.digest, is_sha256, "sha256")?;
    }
    if let Some(harness) = &provenance.harness {
        check_vs_token(&harness.id)?;
        check_vs(&harness.version, is_semver, "semver")?;
        check_vs_token(&harness.model_id)?;
        check_vs_token(&harness.model_revision)?;
    }
    Ok(())
}

fn validate_metrics(metrics: &super::types::MetricsIn) -> Result<(), Violation> {
    for count in [
        &metrics.duration_ms,
        &metrics.files_read,
        &metrics.files_changed,
        &metrics.tool_calls,
        &metrics.retry_count,
        &metrics.replan_count,
    ] {
        check_count(count)?;
    }
    if let Some(tokens) = &metrics.tokens {
        for count in [
            &tokens.input,
            &tokens.output,
            &tokens.reasoning,
            &tokens.total,
        ] {
            check_count(count)?;
        }
        // The total is never inferred — but when every operand is
        // known, an inconsistent total is a contradiction, not a guess.
        if let (
            Some(Vs::Known(input)),
            Some(Vs::Known(output)),
            Some(Vs::Known(reasoning)),
            Some(Vs::Known(total)),
        ) = (
            &tokens.input,
            &tokens.output,
            &tokens.reasoning,
            &tokens.total,
        ) {
            let sum = input.saturating_add(*output).saturating_add(*reasoning);
            if sum != *total {
                return Err(Violation::invalid("total-token-inconsistency"));
            }
        }
    }
    if let Some(cost) = &metrics.cost {
        check_vs(&cost.amount, is_decimal, "decimal")?;
        check_vs(&cost.currency, is_currency, "currency")?;
        // Reported/estimated are closed labels.
        let _ = &cost.basis;
    }
    if let Some(context) = &metrics.context {
        for count in [
            &context.bytes,
            &context.estimated_tokens,
            &context.included_facts,
            &context.candidate_facts,
        ] {
            check_count(count)?;
        }
        check_vs(&context.coverage_ratio, is_ratio, "ratio")?;
        // json/markdown are closed labels.
        let _ = &context.representation;
        check_vs(&context.estimator_version, is_token, "token")?;
        // A known ratio requires a known nonzero denominator.
        if matches!(context.coverage_ratio, Some(Vs::Known(_))) {
            let denominator_known_nonzero = matches!(
                context.candidate_facts,
                Some(Vs::Known(candidate)) if candidate > 0
            );
            if !denominator_known_nonzero {
                return Err(Violation::invalid("coverage-ratio-denominator"));
            }
        }
    }
    Ok(())
}

/// Whether the measurement source targets a leaf with a known value in
/// the normalized metrics (one source per known metric).
fn source_targets_known_metric(field: MeasurementField, metrics: &super::types::MetricsIn) -> bool {
    fn is_known<T>(value: &Option<Vs<T>>) -> bool {
        matches!(value, Some(Vs::Known(_)))
    }
    match field {
        MeasurementField::DurationMs => is_known(&metrics.duration_ms),
        MeasurementField::FilesRead => is_known(&metrics.files_read),
        MeasurementField::FilesChanged => is_known(&metrics.files_changed),
        MeasurementField::ToolCalls => is_known(&metrics.tool_calls),
        MeasurementField::RetryCount => is_known(&metrics.retry_count),
        MeasurementField::ReplanCount => is_known(&metrics.replan_count),
        MeasurementField::TokensInput => metrics
            .tokens
            .as_ref()
            .is_some_and(|tokens| is_known(&tokens.input)),
        MeasurementField::TokensOutput => metrics
            .tokens
            .as_ref()
            .is_some_and(|tokens| is_known(&tokens.output)),
        MeasurementField::TokensReasoning => metrics
            .tokens
            .as_ref()
            .is_some_and(|tokens| is_known(&tokens.reasoning)),
        MeasurementField::TokensTotal => metrics
            .tokens
            .as_ref()
            .is_some_and(|tokens| is_known(&tokens.total)),
        MeasurementField::CostAmount => metrics
            .cost
            .as_ref()
            .is_some_and(|cost| is_known(&cost.amount)),
        MeasurementField::ContextBytes => metrics
            .context
            .as_ref()
            .is_some_and(|context| is_known(&context.bytes)),
        MeasurementField::ContextEstimatedTokens => metrics
            .context
            .as_ref()
            .is_some_and(|context| is_known(&context.estimated_tokens)),
        MeasurementField::ContextIncludedFacts => metrics
            .context
            .as_ref()
            .is_some_and(|context| is_known(&context.included_facts)),
        MeasurementField::ContextCandidateFacts => metrics
            .context
            .as_ref()
            .is_some_and(|context| is_known(&context.candidate_facts)),
        MeasurementField::ContextCoverageRatio => metrics
            .context
            .as_ref()
            .is_some_and(|context| is_known(&context.coverage_ratio)),
    }
}

// ---------------------------------------------------------------------------
// Normalization and record construction
// ---------------------------------------------------------------------------

fn normalize_vs<T>(leaf: Option<Vs<T>>) -> Vs<T> {
    Vs::normalize(leaf)
}

fn normalize_provenance(provenance: &super::types::ProvenanceIn) -> Provenance {
    let default_git = GitProvenanceIn::default();
    let git = provenance.git.as_ref().unwrap_or(&default_git);
    let default_model = ModelProvenanceIn::default();
    let model = provenance.model.as_ref().unwrap_or(&default_model);
    let default_lock = LockProvenanceIn::default();
    let lock = provenance.lock.as_ref().unwrap_or(&default_lock);
    let default_core = CoreProvenanceIn::default();
    let core = provenance.core.as_ref().unwrap_or(&default_core);
    let default_profile = ProfileProvenanceIn::default();
    let profile = provenance.profile.as_ref().unwrap_or(&default_profile);
    let default_harness = HarnessProvenanceIn::default();
    let harness = provenance.harness.as_ref().unwrap_or(&default_harness);
    Provenance {
        git: GitProvenance {
            commit: normalize_vs(git.commit.clone()),
            dirty: normalize_vs(git.dirty.clone()),
            working_set_digest: normalize_vs(git.working_set_digest.clone()),
        },
        model: ModelProvenance {
            revision: normalize_vs(model.revision.clone()),
            digest: normalize_vs(model.digest.clone()),
            ir_digest: normalize_vs(model.ir_digest.clone()),
        },
        lock: LockProvenance {
            version: normalize_vs(lock.version.clone()),
            digest: normalize_vs(lock.digest.clone()),
        },
        core: CoreProvenance {
            version: normalize_vs(core.version.clone()),
            build_revision: normalize_vs(core.build_revision.clone()),
            build_digest: normalize_vs(core.build_digest.clone()),
        },
        adapters: provenance.adapters.clone(),
        profile: ProfileProvenance {
            id: normalize_vs(profile.id.clone()),
            version: normalize_vs(profile.version.clone()),
            digest: normalize_vs(profile.digest.clone()),
        },
        harness: HarnessProvenance {
            id: normalize_vs(harness.id.clone()),
            version: normalize_vs(harness.version.clone()),
            model_id: normalize_vs(harness.model_id.clone()),
            model_revision: normalize_vs(harness.model_revision.clone()),
        },
    }
}

fn normalize_metrics(metrics: &super::types::MetricsIn) -> Metrics {
    let default_tokens = super::types::TokensIn::default();
    let tokens = metrics.tokens.as_ref().unwrap_or(&default_tokens);
    let default_cost = super::types::CostIn::default();
    let cost = metrics.cost.as_ref().unwrap_or(&default_cost);
    let default_context = super::types::ContextIn::default();
    let context = metrics.context.as_ref().unwrap_or(&default_context);
    Metrics {
        duration_ms: normalize_vs(metrics.duration_ms.clone()),
        files_read: normalize_vs(metrics.files_read.clone()),
        files_changed: normalize_vs(metrics.files_changed.clone()),
        tool_calls: normalize_vs(metrics.tool_calls.clone()),
        retry_count: normalize_vs(metrics.retry_count.clone()),
        replan_count: normalize_vs(metrics.replan_count.clone()),
        tokens: Tokens {
            input: normalize_vs(tokens.input.clone()),
            output: normalize_vs(tokens.output.clone()),
            reasoning: normalize_vs(tokens.reasoning.clone()),
            total: normalize_vs(tokens.total.clone()),
        },
        cost: super::types::Cost {
            amount: normalize_vs(cost.amount.clone()),
            currency: normalize_vs(cost.currency.clone()),
            basis: normalize_vs(cost.basis.clone()),
        },
        context: super::types::Context {
            bytes: normalize_vs(context.bytes.clone()),
            estimated_tokens: normalize_vs(context.estimated_tokens.clone()),
            included_facts: normalize_vs(context.included_facts.clone()),
            candidate_facts: normalize_vs(context.candidate_facts.clone()),
            coverage_ratio: normalize_vs(context.coverage_ratio.clone()),
            representation: normalize_vs(context.representation.clone()),
            estimator_version: normalize_vs(context.estimator_version.clone()),
        },
    }
}

/// Deduplicate diagnostics by code (summing counts), sort by code.
fn normalize_diagnostics(
    diagnostics: &[DiagnosticSummary],
) -> Result<Vec<DiagnosticSummary>, Violation> {
    let mut merged: BTreeMap<String, DiagnosticSummary> = BTreeMap::new();
    for diagnostic in diagnostics {
        match merged.get_mut(&diagnostic.code) {
            Some(existing) => {
                if existing.severity != diagnostic.severity {
                    return Err(Violation::invalid("severity-mismatch"));
                }
                existing.count = existing.count.saturating_add(diagnostic.count);
            }
            None => {
                merged.insert(diagnostic.code.clone(), diagnostic.clone());
            }
        }
    }
    Ok(merged.into_values().collect())
}

/// Map and preserve gate summaries in stable `(kind, id)` order, with
/// every count leaf normalized so the record spelling carries the
/// full value-state shape (absent means unknown, never null).
fn normalize_summaries(summaries: &[GateSummary]) -> Vec<GateSummary> {
    let mut mapped: Vec<GateSummary> = summaries
        .iter()
        .map(|summary| GateSummary {
            id: summary.id.clone(),
            kind: summary.kind,
            source_outcome: summary.source_outcome,
            passed: Some(Vs::normalize(summary.passed.clone())),
            failed: Some(Vs::normalize(summary.failed.clone())),
            unsupported: Some(Vs::normalize(summary.unsupported.clone())),
            infrastructure: Some(Vs::normalize(summary.infrastructure.clone())),
            coverage_state: summary.coverage_state,
            evidence_ref: summary.evidence_ref.clone(),
        })
        .collect();
    mapped
        .sort_by(|left, right| (left.kind, left.id.as_str()).cmp(&(right.kind, right.id.as_str())));
    mapped
}

/// The exact input fingerprint over the full provenance pin tuple. Two
/// runs of the same declared inputs produce the same fingerprint; a
/// changed pin produces a changed fingerprint; any unknown required
/// pin makes exactness impossible.
pub(crate) fn input_fingerprint(provenance: &Provenance) -> String {
    let value = serde_json::to_value(provenance).expect("provenance serializes");
    fingerprint_provenance_json(&value)
}

/// The input fingerprint of a stored record: recomputed from its
/// canonical provenance block with the identical rule, so a parent run
/// without its own repeat link still exposes its pins to a child.
pub(crate) fn fingerprint_from_record(record: &Json) -> Option<String> {
    let provenance = record.get("provenance")?;
    Some(fingerprint_provenance_json(provenance))
}

/// The shared fingerprint computation over the provenance JSON: the
/// pin tuple `{git, model, lock, core, adapters (id-sorted), profile,
/// harness}` in canonical form.
fn fingerprint_provenance_json(provenance: &Json) -> String {
    let mut pins = serde_json::Map::new();
    for section in ["git", "model", "lock", "core", "profile", "harness"] {
        pins.insert(
            section.to_owned(),
            provenance.get(section).cloned().unwrap_or(Json::Null),
        );
    }
    let mut adapters: Vec<Json> = provenance
        .get("adapters")
        .and_then(Json::as_array)
        .cloned()
        .unwrap_or_default();
    adapters.sort_by_key(|adapter| {
        adapter
            .get("id")
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_owned()
    });
    pins.insert("adapters".to_owned(), Json::Array(adapters));
    format!(
        "sha256:{}",
        sha256_hex(canonical::canonical(&Json::Object(pins)).as_bytes())
    )
}

/// Whether every required repeat pin is known in the provenance.
fn pins_complete(provenance: &Provenance) -> bool {
    let known_string = |value: &Vs<String>| matches!(value, Vs::Known(_));
    let known_bool = |value: &Vs<bool>| matches!(value, Vs::Known(_));
    known_string(&provenance.git.commit)
        && known_bool(&provenance.git.dirty)
        && known_string(&provenance.git.working_set_digest)
        && known_string(&provenance.model.revision)
        && known_string(&provenance.model.digest)
        && known_string(&provenance.model.ir_digest)
        && known_string(&provenance.lock.version)
        && known_string(&provenance.lock.digest)
        && known_string(&provenance.core.version)
        && provenance.adapters.iter().all(|adapter| {
            known_string(&adapter.version)
                && known_string(&adapter.manifest_digest)
                && known_string(&adapter.bundle_digest)
        })
        && known_string(&provenance.profile.id)
        && known_string(&provenance.profile.version)
        && known_string(&provenance.profile.digest)
        && known_string(&provenance.harness.id)
        && known_string(&provenance.harness.version)
        && known_string(&provenance.harness.model_id)
        && known_string(&provenance.harness.model_revision)
}

/// Resolve the repeat link against the live parent record.
///
/// `parent_fingerprint` is `Some` only when the parent run exists in
/// the same scope and carries its own recorded fingerprint.
pub(crate) fn resolve_repeat(
    provenance: &Provenance,
    parent: Option<&str>,
    parent_fingerprint: Option<&str>,
) -> Result<Option<RepeatLink>, Violation> {
    let Some(parent_run_id) = parent else {
        return Ok(None);
    };
    let fingerprint = input_fingerprint(provenance);
    let comparability = match parent_fingerprint {
        Some(parent_fingerprint) if pins_complete(provenance) => {
            if parent_fingerprint == fingerprint {
                Comparability::Exact
            } else {
                Comparability::Changed
            }
        }
        _ => Comparability::Incomplete,
    };
    Ok(Some(RepeatLink {
        parent_run_id: parent_run_id.to_owned(),
        input_fingerprint: fingerprint,
        comparability,
    }))
}

/// Build the durable run record and its separate assertion set.
///
/// `recorded_at` is store-authored; `run_id`/`set_id` are generated or
/// supplied; `repository_id`/`tenant_scope_id`/`repository_role` are
/// store-selected and cannot be overridden by the harness.
pub(crate) struct BuiltRecord {
    pub(crate) record: RunRecord,
    pub(crate) assertions: Option<AssertionSet>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_record(
    observation: &Observation,
    run_id: &str,
    set_id: &str,
    recorded_at: Instant,
    repository_id: &str,
    tenant_scope_id: &str,
    repository_role: &str,
    repeat: Option<RepeatLink>,
) -> Result<BuiltRecord, Violation> {
    let provenance = normalize_provenance(&observation.provenance);
    let metrics = normalize_metrics(&observation.metrics);
    let diagnostics = normalize_diagnostics(&observation.diagnostics)?;
    let summaries = normalize_summaries(&observation.test_gate_summaries);

    let assertion_set = observation
        .assertions
        .as_ref()
        .map(|assertions| AssertionSet {
            schema_version: version::RUN_ASSERTIONS_SCHEMA_VERSION,
            identity: version::RUN_ASSERTIONS_IDENTITY,
            artifact_kind: version::RUN_ASSERTIONS_ARTIFACT_KIND,
            set_id: set_id.to_owned(),
            run_id: run_id.to_owned(),
            scope: SetScope {
                repository_id: repository_id.to_owned(),
                tenant_scope_id: tenant_scope_id.to_owned(),
            },
            policy_ref: PolicyRef::frozen(),
            authority_ref: AuthorityRef::frozen(),
            data_sensitivity: observation.data_sensitivity.clone(),
            export_disposition: "local-private",
            rows: assertions.rows.clone(),
        });
    let assertions_ref = assertion_set.as_ref().map(|set| {
        let value = serde_json::to_value(set).expect("assertion set serializes");
        // The digest binds the exact stored bytes: canonical form with
        // one trailing LF, identical to the backend's persistence rule.
        let mut bytes = canonical::canonical(&value).into_bytes();
        bytes.push(b'\n');
        AssertionsRef {
            set_id: set.set_id.clone(),
            digest: format!("sha256:{}", sha256_hex(&bytes)),
            count: u64::try_from(set.rows.len()).expect("row bound"),
        }
    });

    let record = RunRecord {
        schema_version: version::RUN_RECORD_SCHEMA_VERSION,
        identity: version::RUN_RECORD_IDENTITY,
        artifact_kind: version::RUN_RECORD_ARTIFACT_KIND,
        run_id: run_id.to_owned(),
        timestamp: observation.timestamp.clone(),
        recorded_at: clock::render_millis(recorded_at),
        scope: RecordScope {
            repository_id: repository_id.to_owned(),
            tenant_scope_id: tenant_scope_id.to_owned(),
            repository_role: repository_role.to_owned(),
        },
        pilot: observation.pilot,
        provenance,
        operation: observation.operation.clone(),
        status: observation.status,
        metrics,
        measurement_sources: observation.measurement_sources.clone(),
        test_gate_summaries: summaries,
        diagnostics,
        assertions_ref,
        repeat,
        privacy: PrivacyBlock {
            data_sensitivity: observation.data_sensitivity.clone(),
            export_disposition: "local-private",
            policy_ref: PolicyRef::frozen(),
            authority_ref: AuthorityRef::frozen(),
            classification_contract_ref: ClassificationRef::frozen(),
            provenance: PrivacyProvenance {
                origin: PrivacyOrigin::Local,
                repository_role: repository_role.to_owned(),
                derived: false,
                source_refs: assertion_set
                    .as_ref()
                    .map(|set| vec![set.set_id.clone()])
                    .unwrap_or_default(),
            },
            export_eligibility: "ineligible",
        },
    };
    Ok(BuiltRecord {
        record,
        assertions: assertion_set,
    })
}

impl PolicyRef {
    fn frozen() -> Self {
        Self {
            policy_id: refs::POLICY_ID,
            version: refs::POLICY_VERSION,
            digest: refs::POLICY_DIGEST,
        }
    }
}

impl AuthorityRef {
    fn frozen() -> Self {
        Self {
            contract_id: refs::AUTHORITY_CONTRACT_ID,
            version: refs::AUTHORITY_VERSION,
            digest: AUTHORITY_DIGEST_REF,
        }
    }
}

impl ClassificationRef {
    fn frozen() -> Self {
        Self {
            contract_id: refs::CLASSIFICATION_CONTRACT_ID,
            version: refs::DECISION_FAMILY_VERSION,
        }
    }
}
