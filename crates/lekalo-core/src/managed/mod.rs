//! Managed ownership, deterministic generation planning, and recoverable
//! apply/rollback custody (issue #41).
//!
//! The lane is explicit end to end: Model and pinned semantic attachments
//! determine behavior, adapters determine rendering, and only reviewed,
//! confirmed plans mutate project bytes. Core logic is pure and ordered;
//! the only filesystem writes happen inside the confirmed transaction in
//! [`apply`] and [`rollback`].

pub mod apply;
pub mod canonical;
pub mod coverage;
pub mod diagnostic;
pub mod equivalence;
pub mod fs;
pub mod history;
pub mod plan;
pub mod receipts;
pub mod rollback;
#[cfg(test)]
mod tests;
pub mod wire;

use crate::result::{DomainResult, Status};
use serde::{de::DeserializeOwned, Serialize};

pub use diagnostic::*;
pub use wire::*;

/// The one-byte-order canonical serialization of a typed payload: key-sorted
/// compact JSON without insignificant whitespace.
pub fn canonical_bytes<T: Serialize>(value: &T) -> String {
    let dynamic = serde_json::to_value(value).expect("typed wire");
    serde_json::to_string(&dynamic).expect("canonical JSON")
}

/// `sha256:` pin over exact bytes.
pub fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::digest::sha256_hex(bytes))
}

/// `sha256:` pin over the canonical serialization of a typed payload.
pub fn hash<T: Serialize>(value: &T) -> String {
    digest(canonical_bytes(value).as_bytes())
}

/// The stable generation identity: the digest over the pinned input closure
/// and the complete output inventory only. Plan action, before-state, and
/// custody links never enter it.
pub fn generation_digest(record: &GenerationRecord) -> String {
    inventory_digest(
        record.input_closure.iter(),
        record.outputs.iter().map(|output| {
            (
                output.path.as_str(),
                &output.role,
                output.owner_module.as_str(),
                output.owner_target.as_str(),
                output.digest.as_str(),
            )
        }),
    )
}

/// The digest over one closure and one output inventory (plan or record).
pub fn inventory_digest<'a>(
    inputs: impl Iterator<Item = &'a InputPin>,
    outputs: impl Iterator<Item = (&'a str, &'a ArtifactRole, &'a str, &'a str, &'a str)>,
) -> String {
    let inventory = serde_json::json!({
        "inputClosure": inputs.collect::<Vec<_>>(),
        "outputs": outputs.map(|(path, role, owner_module, owner_target, digest)| serde_json::json!({
            "path": path,
            "role": role,
            "ownerModule": owner_module,
            "ownerTarget": owner_target,
            "digest": digest,
        })).collect::<Vec<_>>(),
    });
    digest(inventory.to_string().as_bytes())
}

/// The plan identity: the canonical digest over the whole payload with the
/// self digest field removed. No timestamps, host paths, or random data.
pub fn plan_digest<T: Serialize>(payload: &T) -> String {
    let mut dynamic = serde_json::to_value(payload).expect("typed wire");
    let serde_json::Value::Object(ref mut fields) = dynamic else {
        unreachable!("plans are objects");
    };
    fields.remove("planDigest");
    digest(dynamic.to_string().as_bytes())
}

/// The receipt key of every managed success payload.
pub const PAYLOAD_KEY: &str = "managed";

/// Render a typed success receipt on the status-owned stream.
pub fn render<T: Serialize>(value: &T) -> DomainResult {
    let json = serde_json::json!({"status": "valid", PAYLOAD_KEY: value}).to_string();
    let human = serde_json::to_string_pretty(value).expect("typed output");
    DomainResult::validation(json, human, Vec::new())
}

/// Render a typed receipt that carries one managed failure alongside the
/// evidence (used by denied resolutions and failed equivalence verdicts).
pub fn render_with_failure<T: Serialize>(
    value: &T,
    id: &str,
    data: &[(&str, &str)],
) -> DomainResult {
    let status = diagnostic::status_of(id);
    let json = serde_json::json!({"status": status.as_str(), PAYLOAD_KEY: value}).to_string();
    let human = serde_json::to_string_pretty(value).expect("typed output");
    match status {
        Status::Denied => DomainResult::denied_json(json, human, diagnostic(id, data)),
        _ => DomainResult::invalid(diagnostic(id, data)),
    }
}

pub fn header(family: &str) -> (String, String) {
    (
        format!("lekalo/{family}/v{VERSION}"),
        format!("dev.lekalo.{family}@{VERSION}"),
    )
}

/// Whether `text` is a bounded safe identifier token.
pub fn token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 256
        && text
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.:/@-".contains(&c))
}

/// Whether `text` is a `sha256:<64 lowercase hex>` pin.
pub fn pin(text: &str) -> bool {
    text.len() == 71
        && text.starts_with("sha256:")
        && text[7..]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Whether `text` is a project-relative coordinate: no leading separator,
/// no `..`, no backslash, no drive or device spelling, bounded segments.
pub fn rel_path(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 512
        && !text.contains('\\')
        && !text.contains("..")
        && text.split('/').all(|segment| {
            !segment.is_empty() && {
                let first = segment.as_bytes()[0];
                first.is_ascii_alphanumeric()
                    && segment.bytes().all(|c| {
                        c.is_ascii_alphanumeric()
                            || c == b'_'
                            || c == b'.'
                            || c == b'@'
                            || c == b'-'
                    })
            }
        })
}

/// Whether `rows` are strictly sorted and duplicate-free.
pub fn sorted<T: Ord>(rows: &[T]) -> bool {
    rows.windows(2).all(|window| window[0] < window[1])
}

/// Whether `text` is a bounded prose note: no control characters.
fn prose(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 2000
        && !text
            .chars()
            .any(|c| c.is_ascii_control() || c == '\u{7f}')
}

/// Published nested-reference constraints: the closed `Reference` shape.
fn reference(reference: &Reference) -> bool {
    token(&reference.id) && token(&reference.version) && pin(&reference.digest)
}

/// Published nested-adapter constraints: the closed `Adapter` shape.
fn adapter(adapter: &Adapter) -> bool {
    token(&adapter.entry) && pin(&adapter.package_digest) && token(&adapter.protocol)
}

/// Published nested-assignment constraints: references, roots, extensions.
fn target_assignment(target: &TargetAssignment) -> bool {
    token(&target.target)
        && rel_path(&target.generated_root)
        && reference(&target.profile_ref)
        && reference(&target.implementation_ref)
        && target.extensions.len() <= 64
        && sorted(&target.extensions.iter().map(|e| &e.symbol).collect::<Vec<_>>())
        && target.extensions.iter().all(|extension| {
            token(&extension.symbol) && token(&extension.binding) && pin(&extension.contract_digest)
        })
}

/// Published sample bounds: at most 32 scalar fields per side.
fn sample(sample: &Sample) -> bool {
    sample.input.len() <= 32 && sample.output.len() <= 32
}

/// Published requirement constraints beyond ordering: kinds, witness pins,
/// projection bounds, sample bounds, and the closed prose note.
fn requirement(requirement: &Requirement) -> bool {
    token(&requirement.id)
        && requirement.projection.len() <= 64
        && sorted(&requirement.projection.iter().map(|p| &p.to).collect::<Vec<_>>())
        && requirement
            .projection
            .iter()
            .all(|p| token(&p.from) && token(&p.to))
        && requirement.witness.value().map_or(true, |witness| pin(witness))
        && requirement.detail.as_ref().map_or(true, |detail| prose(detail))
        && requirement.sample.value().map_or(true, sample)
}

/// Whether the ordered projection contains duplicates.
pub fn has_duplicates<'a, T: Ord + 'a>(mut rows: impl Iterator<Item = &'a T>) -> bool {
    let mut seen = std::collections::BTreeSet::new();
    rows.any(|row| !seen.insert(row))
}

fn invalid(reason: &str) -> DomainResult {
    diagnostic::failure(INPUT_INVALID, &detail(reason))
}

/// Strict decode: bounded size, duplicate-key refusal, exact family
/// identity, then closed typed parsing.
pub fn decode<T: DeserializeOwned>(bytes: &[u8], family: &str) -> Result<T, DomainResult> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid("input-bound"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("utf8"))?;
    let index = crate::loader::source::LineIndex::new(text);
    crate::loader::frontends::json::parse(text, &index).map_err(|_| invalid("strict-json"))?;
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid("json"))?;
    let (schema, identity) = header(family);
    if value.get("schemaVersion").and_then(|v| v.as_str()) != Some(schema.as_str())
        || value.get("identity").and_then(|v| v.as_str()) != Some(identity.as_str())
    {
        return Err(diagnostic::failure(
            VERSION_UNSUPPORTED,
            &detail("contract-version"),
        ));
    }
    serde_json::from_value(value).map_err(|_| invalid("closed-wire"))
}

/// Validate the closed ownership document.
pub fn parse_ownership(bytes: &[u8]) -> Result<Ownership, DomainResult> {
    let document: Ownership = decode(bytes, "managed-ownership")?;
    if document.project.is_empty() || !token(&document.project) {
        return Err(invalid("project"));
    }
    if document.modules.is_empty()
        || document.modules.len() > 64
        || !sorted(
            &document
                .modules
                .iter()
                .map(|m| &m.module)
                .collect::<Vec<_>>(),
        )
    {
        return Err(invalid("module-order"));
    }
    let mut roots: Vec<((String, String), String)> = Vec::new();
    let mut seen_keys = std::collections::BTreeSet::new();
    let mut seen_roots = std::collections::BTreeSet::new();
    for module in &document.modules {
        if !token(&module.module) || module.targets.is_empty() || module.targets.len() > 8 {
            return Err(invalid("module"));
        }
        if !sorted(&module.targets.iter().map(|t| &t.target).collect::<Vec<_>>()) {
            return Err(invalid("target-order"));
        }
        for target in &module.targets {
            if !target_assignment(target) {
                return Err(invalid("generated-root"));
            }
            let key = (module.module.clone(), target.target.clone());
            if !seen_keys.insert(key) || !seen_roots.insert(target.generated_root.clone()) {
                return Err(invalid("duplicate-assignment"));
            }
            roots.push((
                (module.module.clone(), target.target.clone()),
                target.generated_root.clone(),
            ));
        }
    }
    // Generated roots may not nest: two overlapping roots would give one
    // file two owners. Sorting alone never proves independence — a sibling
    // spelled between a root and its descendant (e.g. `gen` < `gen-x` <
    // `gen/inner`) hides the nesting from adjacent pairs — so every root is
    // prefix-checked against every later root.
    roots.sort_by(|left, right| left.1.cmp(&right.1));
    for (index, (_, candidate)) in roots.iter().enumerate() {
        let prefix = format!("{candidate}/");
        if roots[index + 1..].iter().any(|(_, other)| other.starts_with(&prefix)) {
            return Err(invalid("root-overlap"));
        }
    }
    Ok(document)
}

/// Validate the closed coverage document structure (semantic admission is
/// [`coverage::admit`]).
pub fn parse_coverage(bytes: &[u8]) -> Result<Coverage, DomainResult> {
    let document: Coverage = decode(bytes, "managed-coverage")?;
    if !token(&document.module) || !token(&document.target) {
        return Err(invalid("subject"));
    }
    if !reference(&document.recipe) || !adapter(&document.adapter) {
        return Err(invalid("coverage-pins"));
    }
    if document.requirements.len() > 4096
        || !sorted(
            &document
                .requirements
                .iter()
                .map(|r| &r.id)
                .collect::<Vec<_>>(),
        )
        || !document.requirements.iter().all(requirement)
    {
        return Err(invalid("requirement-order"));
    }
    for requirement in &document.requirements {
        if let Some(witness) = requirement.witness.value() {
            if !pin(witness) {
                return Err(invalid("witness"));
            }
        }
        if let Some(sample) = requirement.sample.value() {
            let inputs: Vec<&String> = sample.input.keys().collect();
            let outputs: Vec<&String> = sample.output.keys().collect();
            if !sorted(&inputs) || !sorted(&outputs) {
                return Err(invalid("sample-order"));
            }
        }
    }
    if document.extensions.len() > 64
        || !sorted(
            &document
                .extensions
                .iter()
                .map(|e| &e.symbol)
                .collect::<Vec<_>>(),
        )
        || !document.extensions.iter().all(|check| {
            token(&check.symbol) && token(&check.binding) && pin(&check.contract_digest)
        })
    {
        return Err(invalid("extension-order"));
    }
    Ok(document)
}

/// Validate the closed generation plan: closed structure, action/output
/// consistency, and the exact self digest over the payload without it.
pub fn verify_plan(bytes: &[u8]) -> Result<GenerationPlan, DomainResult> {
    let document: GenerationPlan = decode(bytes, "managed-generation-plan")?;
    if document.modules.is_empty()
        || document.targets.is_empty()
        || document.modules.len() > 64
        || document.targets.len() > 8
        || document.input_closure.len() > 4096
        || document.extensions.len() > 64
        || !sorted(&document.modules)
        || !sorted(&document.targets)
    {
        return Err(invalid("plan-scope"));
    }
    if document.input_closure.len() > 4096
        || !sorted(
            &document
                .input_closure
                .iter()
                .map(|p| &p.path)
                .collect::<Vec<_>>(),
        )
        || document
            .input_closure
            .iter()
            .any(|p| !rel_path(&p.path) || !pin(&p.digest))
    {
        return Err(invalid("input-closure"));
    }
    if !pin(&document.coverage_digest) {
        return Err(invalid("coverage-digest"));
    }
    if !sorted(&document.modules) || document.modules.iter().any(|m| !token(m)) {
        return Err(invalid("plan-modules"));
    }
    if document.targets.iter().any(|t| !token(t)) {
        return Err(invalid("plan-targets"));
    }
    if document.outputs.is_empty()
        || document.outputs.len() > 8192
        || !sorted(&document.outputs.iter().map(|o| &o.path).collect::<Vec<_>>())
    {
        return Err(invalid("output-order"));
    }
    for output in &document.outputs {
        if !rel_path(&output.path)
            || !token(&output.owner_module)
            || !token(&output.owner_target)
            || output
                .before_digest
                .value()
                .map_or(false, |before| !pin(before))
        {
            return Err(invalid("output"));
        }
        match (output.action, &output.before_digest, &output.after_digest) {
            (OutputAction::Create, State::Absent, State::Known { .. })
            | (OutputAction::Replace, State::Known { .. }, State::Known { .. })
            | (OutputAction::Unchanged, State::Known { .. }, State::Known { .. }) => {}
            (OutputAction::Delete, State::Known { .. }, State::Absent) => {}
            _ => return Err(invalid("output-action")),
        }
        if let Some(before) = output.before_digest.value() {
            if !pin(before) {
                return Err(invalid("before-digest"));
            }
        }
        if let Some(after) = output.after_digest.value() {
            if !pin(after) {
                return Err(invalid("after-digest"));
            }
        }
        if let (OutputAction::Unchanged, Some(before), Some(after)) = (
            output.action,
            output.before_digest.value(),
            output.after_digest.value(),
        ) {
            if before != after {
                return Err(invalid("unchanged-digest"));
            }
        }
    }
    if !sorted(
        &document
            .extensions
            .iter()
            .map(|e| &e.symbol)
            .collect::<Vec<_>>(),
    ) {
        return Err(invalid("extension-order"));
    }
    if !pin(&document.plan_digest) || plan_digest(&document) != document.plan_digest {
        return Err(invalid("plan-digest"));
    }
    Ok(document)
}

/// Validate a committed generation record.
pub fn parse_record(bytes: &[u8]) -> Result<GenerationRecord, DomainResult> {
    let document: GenerationRecord = decode(bytes, "managed-generation-record")?;
    if !pin(&document.plan_digest) || !pin(&document.generation_digest) {
        return Err(invalid("record-digest"));
    }
    if document.input_closure.len() > 4096
        || !sorted(
            &document
                .input_closure
                .iter()
                .map(|p| &p.path)
                .collect::<Vec<_>>(),
        )
        || document
            .input_closure
            .iter()
            .any(|p| !rel_path(&p.path) || !pin(&p.digest))
    {
        return Err(invalid("input-closure"));
    }
    if document.outputs.is_empty()
        || document.outputs.len() > 8192
        || !sorted(&document.outputs.iter().map(|o| &o.path).collect::<Vec<_>>())
    {
        return Err(invalid("output-order"));
    }
    for output in &document.outputs {
        if !rel_path(&output.path) || !pin(&output.digest) {
            return Err(invalid("record-output"));
        }
        for reference in [&output.before_ref, &output.after_ref] {
            if let Some(reference) = reference.value() {
                let hex = reference.strip_prefix("sha256:").unwrap_or("");
                if hex.len() != 64
                    || !hex
                        .bytes()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                {
                    return Err(invalid("object-ref"));
                }
            }
        }
    }
    if !pin(&document.ownership_digest) {
        return Err(invalid("ownership-digest"));
    }
    if let Some(lock) = document.lock_pin.value() {
        if !pin(lock) {
            return Err(invalid("lock-pin"));
        }
    }
    Ok(document)
}

/// Validate a transaction journal entry.
pub fn parse_transaction(bytes: &[u8]) -> Result<Transaction, DomainResult> {
    let document: Transaction = decode(bytes, "managed-transaction")?;
    if !token(&document.transaction_id) {
        return Err(invalid("transaction-id"));
    }
    if document.affected_files.len() > 8192
        || !sorted(&document.affected_files)
        || document.affected_files.iter().any(|p| !rel_path(p))
    {
        return Err(invalid("affected-files"));
    }
    if document.pending.len() > 8192
        || !sorted(
            &document
                .pending
                .iter()
                .map(|p| (&p.path, &p.op))
                .collect::<Vec<_>>(),
        )
        || document.pending.iter().any(|p| !rel_path(&p.path))
    {
        return Err(invalid("pending-order"));
    }
    Ok(document)
}

/// Validate a rollback plan, including its self digest.
pub fn verify_rollback_plan(bytes: &[u8]) -> Result<RollbackPlan, DomainResult> {
    let document: RollbackPlan = decode(bytes, "managed-rollback-plan")?;
    if !pin(&document.destination_generation_digest) || !pin(&document.manifest_digest) {
        return Err(invalid("rollback-destination"));
    }
    if document.restore.len() > 8192
        || !sorted(&document.restore.iter().map(|r| &r.path).collect::<Vec<_>>())
        || document
            .restore
            .iter()
            .any(|r| !rel_path(&r.path) || !pin(&r.digest) || !pin(&r.object_ref))
    {
        return Err(invalid("restore-order"));
    }
    if document.delete.len() > 8192
        || !sorted(&document.delete.iter().map(|d| &d.path).collect::<Vec<_>>())
        || document
            .delete
            .iter()
            .any(|d| !rel_path(&d.path) || !pin(&d.digest))
    {
        return Err(invalid("delete-order"));
    }
    if document
        .restore
        .iter()
        .any(|r| document.delete.iter().any(|d| d.path == r.path))
    {
        return Err(invalid("restore-delete-overlap"));
    }
    if !pin(&document.plan_digest) || plan_digest(&document) != document.plan_digest {
        return Err(invalid("plan-digest"));
    }
    Ok(document)
}

/// Validate an equivalence evidence document.
pub fn parse_equivalence(bytes: &[u8]) -> Result<Equivalence, DomainResult> {
    let document: Equivalence = decode(bytes, "managed-equivalence")?;
    if !token(&document.module) || !token(&document.target) || !pin(&document.implementation_digest)
    {
        return Err(invalid("equivalence-subject"));
    }
    if let Some(reference) = document.reference_digest.value() {
        if !pin(reference) {
            return Err(invalid("reference-digest"));
        }
    }
    if document.scenarios.is_empty()
        || document.scenarios.len() > 4096
        || document.gaps.len() > 4096
        || !sorted(
            &document
                .scenarios
                .iter()
                .map(|s| &s.scenario)
                .collect::<Vec<_>>(),
        )
        || document
            .scenarios
            .iter()
            .any(|s| !token(&s.scenario) || !pin(&s.digest))
    {
        return Err(invalid("scenario-order"));
    }
    if !sorted(&document.gaps) || document.gaps.iter().any(|g| !token(g)) {
        return Err(invalid("gap-order"));
    }
    Ok(document)
}
