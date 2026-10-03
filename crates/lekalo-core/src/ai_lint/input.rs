//! Strict decoding, source custody and bounded input admission.
use super::{diagnostic::failure, wire::*};
use crate::{project_fs::Fs, result::DomainResult};
use serde::{de::DeserializeOwned, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
pub const VERSION: &str = "0.6.4";
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_ROWS: usize = 10_000;
pub const MAX_WORK: usize = 10_000_000;
pub fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
pub fn canonical<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(&serde_json::to_value(value).expect("typed wire")).expect("canonical wire")
}
pub fn hash<T: Serialize>(value: &T) -> String {
    digest(&canonical(value))
}
pub fn is_digest(s: &str) -> bool {
    s.len() == 71
        && s.starts_with("sha256:")
        && s[7..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:/#@\\-".contains(&b))
        && s.as_bytes()[0].is_ascii_alphanumeric()
}
pub fn prose(s: &str) -> bool {
    !s.trim().is_empty() && s.chars().count() <= 2000 && !s.chars().any(char::is_control)
}
pub fn sorted<T: Ord>(items: &[T]) -> bool {
    items.windows(2).all(|p| p[0] < p[1])
}
pub fn header(family: &str) -> (String, String) {
    (
        format!("lekalo/ai-lint-{family}/v{VERSION}"),
        format!("dev.lekalo.ai-lint-{family}@{VERSION}"),
    )
}
pub fn check_header(family: &str, schema: &str, identity: &str) -> Result<(), DomainResult> {
    let expected = header(family);
    if schema != expected.0 || identity != expected.1 {
        return Err(failure("ai-lint.version-unsupported", "contract"));
    }
    Ok(())
}
pub fn parse<T: DeserializeOwned + Serialize>(bytes: &[u8]) -> Result<T, DomainResult> {
    if bytes.len() > MAX_BYTES {
        return Err(failure("ai-lint.input-invalid", "input-limit"));
    }
    let value: T = serde_json::from_slice(bytes)
        .map_err(|_| failure("ai-lint.input-invalid", "closed-shape"))?;
    validate_tree(&serde_json::to_value(&value).expect("typed wire"), "", 0)?;
    Ok(value)
}
fn validate_tree(v: &serde_json::Value, key: &str, depth: usize) -> Result<(), DomainResult> {
    let invalid = || failure("ai-lint.input-invalid", "wire-bound");
    if depth > 64 {
        return Err(invalid());
    }
    match v {
        serde_json::Value::Null => return Err(invalid()),
        serde_json::Value::Array(a) => {
            let cap =
                if ["witness", "activation", "locations", "guards", "candidates"].contains(&key) {
                    32
                } else {
                    MAX_ROWS
                };
            if a.len() > cap {
                return Err(invalid());
            }
            for x in a {
                validate_tree(x, "", depth + 1)?;
            }
        }
        serde_json::Value::Object(o) => {
            for (k, x) in o {
                validate_tree(x, k, depth + 1)?;
            }
        }
        serde_json::Value::String(s) => {
            if ["message", "alternative", "reason"].contains(&key) {
                if !prose(s) {
                    return Err(invalid());
                }
            } else if !token(s) {
                return Err(invalid());
            }
        }
        serde_json::Value::Number(n)
            if n.as_i64()
                .map_or(true, |n| !(-10_000_000..=10_000_000).contains(&n)) =>
        {
            return Err(invalid())
        }
        _ => {}
    }
    Ok(())
}
pub fn registry_ref() -> RegistryRef {
    RegistryRef {
        version: crate::diagnostics::version::REGISTRY_VERSION.into(),
        digest: digest(crate::diagnostics::registry::REGISTRY_BYTES),
    }
}
pub fn parse_config(bytes: &[u8]) -> Result<Config, DomainResult> {
    let config: Config = parse(bytes)?;
    check_header("config", &config.schema_version, &config.identity)?;
    if config.registry_ref != registry_ref()
        || config.profiles.is_empty()
        || !sorted(&config.profiles.iter().map(|p| &p.id).collect::<Vec<_>>())
    {
        return Err(failure("ai-lint.input-invalid", "config-pin-order"));
    }
    for p in &config.profiles {
        if !["off", "advisory", "ci"].contains(&p.id.as_str()) || p.recipe != "ai-readability/1" {
            return Err(failure("ai-lint.version-unsupported", "profile-recipe"));
        }
        if !sorted(&p.rules.iter().map(|r| &r.id).collect::<Vec<_>>())
            || !sorted(&p.gate.required_coverage)
            || p.gate.minimum_confidence < Confidence::High
        {
            return Err(failure("ai-lint.input-invalid", "profile-selection"));
        }
        for r in &p.rules {
            if !super::RULES.contains(&r.id.as_str())
                || r.severity.known().is_some_and(|s| s != "info")
                || (p.id == "off" && r.enabled)
            {
                return Err(failure("ai-lint.input-invalid", "profile-rule"));
            }
        }
        if p.gate
            .required_coverage
            .iter()
            .any(|r| !p.rules.iter().any(|s| &s.id == r && s.enabled))
        {
            return Err(failure("ai-lint.input-invalid", "required-disabled-rule"));
        }
    }
    let mut groups = BTreeSet::new();
    let mut members = BTreeSet::new();
    for g in &config.related_writer_groups {
        if !prose(&g.reason)
            || !sorted(&g.operations)
            || g.operations.len() < 2
            || !groups.insert((&g.resource, &g.field, &g.owner))
            || g.operations
                .iter()
                .any(|op| !members.insert((&g.resource, &g.field, op)))
        {
            return Err(failure("ai-lint.input-invalid", "writer-group"));
        }
    }
    for d in &config.permitted_defaults {
        if !sorted(&d.targets) || d.targets.len() < 2 || !prose(&d.reason) {
            return Err(failure("ai-lint.input-invalid", "permitted-default"));
        }
    }
    Ok(config)
}
pub fn read_source(fs: &Fs, path: &str) -> Result<Vec<u8>, DomainResult> {
    if crate::project_fs::path_violation(path).is_some() || path.len() > 512 {
        return Err(failure("ai-lint.input-invalid", "source-path"));
    }
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    fs.read_file_opt(dir, name, MAX_BYTES)
        .map_err(|_| failure("ai-lint.input-invalid", "source-read"))?
        .ok_or_else(|| failure("ai-lint.input-invalid", "source-missing"))
}
pub fn position(text: &str, byte: usize) -> Option<(u64, u64)> {
    if !text.is_char_boundary(byte) {
        return None;
    }
    let before = &text[..byte];
    Some((
        before.bytes().filter(|b| *b == b'\n').count() as u64 + 1,
        before.rsplit('\n').next()?.chars().count() as u64 + 1,
    ))
}
pub fn admit_evidence(
    e: &Evidence,
    fs: &Fs,
    model: &str,
    ir: &str,
    scope: &[String],
) -> Result<(), DomainResult> {
    check_header("evidence", &e.schema_version, &e.identity)?;
    if e.producer.recipe != "ai-readability/1" {
        return Err(failure("ai-lint.version-unsupported", "detector-recipe"));
    }
    if !is_digest(&e.producer.artifact_digest)
        || !is_digest(&e.input_manifest_digest)
        || e.scope != scope
        || !sorted(scope)
        || e.target == "model"
    {
        return Err(failure("ai-lint.input-invalid", "evidence-scope"));
    }
    if e.pins.model.known().is_some_and(|v| v != model)
        || e.pins.ir.known().is_some_and(|v| v != ir)
    {
        return Err(failure("ai-lint.input-invalid", "evidence-model-pin"));
    }
    for pin in [
        &e.pins.model,
        &e.pins.ir,
        &e.pins.observed,
        &e.pins.profile,
        &e.pins.capabilities,
    ] {
        if pin.known().is_some_and(|v| !is_digest(v)) {
            return Err(failure("ai-lint.input-invalid", "evidence-pin"));
        }
    }
    let mut sources = std::collections::BTreeMap::new();
    let mut paths = BTreeSet::new();
    let mut total = 0usize;
    for source in &e.sources {
        if !is_digest(&source.fingerprint)
            || source.id != hash(&(&source.path, &source.fingerprint))
            || !paths.insert(&source.path)
            || sources.contains_key(&source.id)
        {
            return Err(failure("ai-lint.input-invalid", "source-identity"));
        }
        let bytes = read_source(fs, &source.path)?;
        total = total
            .checked_add(bytes.len())
            .ok_or_else(|| failure("ai-lint.input-invalid", "source-limit"))?;
        if total > MAX_BYTES
            || source.bytes != bytes.len() as u64
            || digest(&bytes) != source.fingerprint
        {
            return Err(failure("ai-lint.input-invalid", "source-fingerprint"));
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| failure("ai-lint.input-invalid", "source-utf8"))?;
        sources.insert(&source.id, text);
    }
    if hash(&e.sources) != e.input_manifest_digest {
        return Err(failure("ai-lint.input-invalid", "manifest-digest"));
    }
    let mut spans = BTreeSet::new();
    for s in &e.locations {
        let text = sources
            .get(&s.source)
            .ok_or_else(|| failure("ai-lint.input-invalid", "span-source"))?;
        if !spans.insert(&s.id)
            || s.id != hash(&(&s.source, s.start, s.end))
            || s.start >= s.end
            || s.end > text.len() as u64
            || position(text, s.start as usize) != Some((s.line, s.column))
            || position(text, s.end as usize) != Some((s.end_line, s.end_column))
        {
            return Err(failure("ai-lint.input-invalid", "span-range"));
        }
    }
    let mut records = BTreeSet::new();
    for r in &e.records {
        if !records.insert(&r.id)
            || r.id != hash(&(r.kind, &r.subject, &r.native_id, &r.locations))
            || r.locations.is_empty()
            || !sorted(&r.locations)
            || r.locations.iter().any(|l| !spans.contains(l))
            || r.activation
                .iter()
                .any(|s| s.locations.is_empty() || s.locations.iter().any(|l| !spans.contains(l)))
            || r.claim == Claim::VerifiedBehavior
            || (matches!(
                r.kind,
                Kind::Effect
                    | Kind::Reflection
                    | Kind::Dispatch
                    | Kind::Convention
                    | Kind::StringReference
            ) && r.claim != Claim::PossibleBehavior)
        {
            return Err(failure("ai-lint.input-invalid", "record-proof"));
        }
        if !sorted(&r.candidates.iter().map(|c| &c.identity).collect::<Vec<_>>()) {
            return Err(failure("ai-lint.input-invalid", "candidate-identity"));
        }
        if r.kind == Kind::Effect
            && r.activation.iter().map(|s| s.role).collect::<Vec<_>>()
                != [
                    StepRole::Binding,
                    StepRole::Trigger,
                    StepRole::Registration,
                    StepRole::Callback,
                    StepRole::Effect,
                ]
        {
            return Err(failure("ai-lint.input-invalid", "activation-chain"));
        }
    }
    let mut coverage = BTreeSet::new();
    for c in &e.coverage {
        validate_coverage(c)?;
        if !super::RULES.contains(&c.rule.as_str())
            || c.target != e.target
            || !scope.contains(&c.scope)
            || !coverage.insert((&c.rule, &c.scope))
            || c.state == CoverageState::Disabled
        {
            return Err(failure("ai-lint.input-invalid", "coverage-scope"));
        }
    }
    Ok(())
}
pub fn validate_coverage(c: &Coverage) -> Result<(), DomainResult> {
    let valid = match (c.eligible.known(), c.examined.known()) {
        (Some(a), Some(b)) => b <= a && (c.state != CoverageState::Complete || a == b),
        _ => c.state != CoverageState::Complete,
    };
    if !valid || (c.state == CoverageState::Complete && !c.limitations.is_empty()) {
        return Err(failure("ai-lint.input-invalid", "coverage-arithmetic"));
    }
    Ok(())
}
pub fn date(s: &str) -> bool {
    let parts = s.split('-').collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0].len() != 4
        || parts[1].len() != 2
        || parts[2].len() != 2
        || !parts.iter().all(|s| s.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        parts[0].parse::<u32>(),
        parts[1].parse::<usize>(),
        parts[2].parse::<u32>(),
    ) else {
        return false;
    };
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    y > 0
        && (1..=12).contains(&m)
        && d > 0
        && d <= [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ][m - 1]
}
pub fn parse_waivers(
    bytes: &[u8],
    config_ref: &str,
    as_of: Option<&str>,
) -> Result<Waivers, DomainResult> {
    let w: Waivers = parse(bytes).map_err(|_| failure("ai-lint.waiver-invalid", "closed-shape"))?;
    check_header("waivers", &w.schema_version, &w.identity)?;
    let mut ids = BTreeSet::new();
    let mut scopes = BTreeSet::new();
    if w.config_ref != config_ref {
        return Err(failure("ai-lint.waiver-invalid", "config-pin"));
    }
    for e in &w.entries {
        if !super::RULES.contains(&e.rule_id.as_str())
            || !ids.insert(&e.id)
            || !scopes.insert((&e.rule_id, &e.subject, &e.target, &e.condition_digest))
            || !is_digest(&e.condition_digest)
            || !prose(&e.reason)
            || !token(&e.owner)
            || e.source_digest.known().is_some_and(|s| !is_digest(s))
            || e.expires_on
                .known()
                .is_some_and(|s| !date(s) || as_of.is_none())
        {
            return Err(failure("ai-lint.waiver-invalid", "scope-reason-date"));
        }
    }
    Ok(w)
}
