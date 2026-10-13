//! Managed ownership, deterministic generation planning, and recoverable
//! apply/rollback custody (issue #41).
//!
//! The lane is explicit end to end: Model and pinned semantic attachments
//! determine behavior, adapters determine rendering, and only reviewed,
//! confirmed plans mutate project bytes. Core logic is pure and ordered;
//! the only filesystem writes happen inside the confirmed transaction in
//! [`apply`] and [`rollback`].

pub mod apply;
pub mod authority;
pub mod bindings;
pub mod canonical;
pub mod coverage;
pub mod custody;
pub mod diagnostic;
pub mod equivalence;
pub mod fs;
pub mod history;
pub mod plan;
pub mod receipts;
pub mod recovery;
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

/// The authenticated custody-record identity: the exact digest over the
/// whole canonical retained document with the self digest field zeroed
/// (the storage name stays the stable generation identity). Distinct from
/// the stable [`generation_digest`]: the stable identity names *what* was
/// generated, the record identity authenticates *the retained evidence*
/// that binds it to the plan, the ownership policy, and the lock revision
/// it was admitted under.
pub fn record_identity(record: &GenerationRecord) -> String {
    let mut document = serde_json::to_value(record).expect("typed record");
    if let serde_json::Value::Object(ref mut fields) = document {
        fields.remove("recordDigest");
    }
    digest(document.to_string().as_bytes())
}

/// Verify the authenticated record identity: the record must carry its own
/// exact custody digest (`recordDigest`), binding every pin the stable
/// generation identity deliberately ignores — the plan digest, the
/// ownership digest, the lock pin, and the previous-generation link.
/// A forged or tampered record never authenticates.
pub fn verify_record_identity(record: &GenerationRecord) -> Result<(), DomainResult> {
    if !pin(&record.record_digest) || record_identity(record) != record.record_digest {
        return Err(diagnostic::failure(
            diagnostic::ROLLBACK_REFUSED,
            &detail("record-identity"),
        ));
    }
    Ok(())
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
        && !text.chars().any(|c| c.is_ascii_control() || c == '\u{7f}')
}

/// Published nested-reference constraints: the closed `Reference` shape.
fn reference(reference: &Reference) -> bool {
    token(&reference.id) && token(&reference.version) && pin(&reference.digest)
}

/// Published nested-adapter constraints: the closed `Adapter` shape. The
/// entry is a project-relative coordinate (finding 10): no absolute
/// spelling, no traversal, no drive — the entry read is confined.
fn adapter(adapter: &Adapter) -> bool {
    rel_path(&adapter.entry) && pin(&adapter.package_digest) && token(&adapter.protocol)
}

/// Published nested-assignment constraints: references, roots, extensions.
fn target_assignment(target: &TargetAssignment) -> bool {
    token(&target.target)
        && rel_path(&target.generated_root)
        && reference(&target.profile_ref)
        && reference(&target.implementation_ref)
        && target.extensions.len() <= 64
        && sorted(
            &target
                .extensions
                .iter()
                .map(|e| &e.symbol)
                .collect::<Vec<_>>(),
        )
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
        && sorted(
            &requirement
                .projection
                .iter()
                .map(|p| &p.to)
                .collect::<Vec<_>>(),
        )
        && requirement
            .projection
            .iter()
            .all(|p| token(&p.from) && token(&p.to))
        && requirement
            .witness
            .value()
            .map_or(true, |witness| pin(witness))
        && requirement
            .detail
            .as_ref()
            .map_or(true, |detail| prose(detail))
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
    // Receipt-echoed caller values are screened (r6 codex F7): the
    // project id rides every managed receipt verbatim, so a value that
    // is spelling-canonical credential material — a GitHub-style token
    // prefix in any case spelling — never enters the receipt stream.
    if !credential_free(&document.project) {
        return Err(invalid("project-credential"));
    }
    if document.inputs.len() > 4095
        || !sorted(&document.inputs.iter().collect::<Vec<_>>())
        || !document
            .inputs
            .iter()
            .all(|path| rel_path(path) && credential_free(path))
    {
        return Err(invalid("input-selection"));
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
        if roots[index + 1..]
            .iter()
            .any(|(_, other)| other.starts_with(&prefix))
        {
            return Err(invalid("root-overlap"));
        }
    }
    Ok(document)
}

/// Whether one bounded string is free of spelling-canonical credential
/// material (r6 codex F7): a GitHub-style token — `gh[pousr]_` followed by
/// at least twenty identifier characters — in ANY case spelling of the
/// prefix. Managed receipts echo caller-owned string fields verbatim;
/// values that are credential-shaped refuse at parse time instead of
/// being echoed. Purely structural, never a secrecy claim.
pub fn credential_free(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        let mut from = 0usize;
        while let Some(found) = lowered[from..].find(prefix) {
            let at = from + found;
            from = at + prefix.len();
            let body = lowered[from..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .count();
            if body >= 20 {
                return false;
            }
        }
    }
    true
}

/// Validate the closed coverage document structure (semantic admission is
/// [`coverage::admit`]).
pub fn parse_coverage(bytes: &[u8]) -> Result<Coverage, DomainResult> {
    let document: Coverage = decode(bytes, "managed-coverage")?;
    if !token(&document.module) || !token(&document.target) {
        return Err(invalid("subject"));
    }
    // Receipt-echoed caller values are screened (r6 codex F7): module and
    // target ids ride plan/apply/equivalence receipts verbatim.
    if !credential_free(&document.module) || !credential_free(&document.target) {
        return Err(invalid("subject-credential"));
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
                .is_some_and(|before| !pin(before))
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

/// The reserved closure prefix of the derived authority pins (finding 8):
/// the canonical Model, typed IR, and lock revision custody evidence every
/// committed generation carries. Derived pins name no project file; they
/// are revalidated against the live derived authority at every consume.
pub const AUTHORITY_PIN_PREFIX: &str = ".lekalo/managed-authority/";

/// The derived authority name of a reserved pin path (`model`/`ir`/
/// `lock`), when `text` is one (recovery revalidates the pinned authority
/// closure against a live derivation).
pub fn authority_pin_name(text: &str) -> Option<&str> {
    let rest = text.strip_prefix(AUTHORITY_PIN_PREFIX)?;
    matches!(rest, "model" | "ir" | "lock").then_some(rest)
}

/// Whether `text` is a valid committed input-closure pin path: either a
/// project-relative coordinate or a reserved derived authority pin under
/// [`AUTHORITY_PIN_PREFIX`] (the canonical Model/IR/lock custody evidence;
/// derived, never read as a project file — finding 8).
fn input_pin_path(text: &str) -> bool {
    if let Some(rest) = text.strip_prefix(AUTHORITY_PIN_PREFIX) {
        return matches!(rest, "model" | "ir" | "lock");
    }
    rel_path(text)
}

/// Validate a committed generation record.
pub fn parse_record(bytes: &[u8]) -> Result<GenerationRecord, DomainResult> {
    let document: GenerationRecord = decode(bytes, "managed-generation-record")?;
    if !pin(&document.plan_digest) || !pin(&document.generation_digest) {
        return Err(invalid("record-digest"));
    }
    // The authenticated custody identity must be present and cover the
    // exact document (finding 3): ownership, lock, plan, and previous-
    // generation pins are bound through it.
    if !pin(&document.record_digest) || record_identity(&document) != document.record_digest {
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
            .any(|p| !input_pin_path(&p.path) || !pin(&p.digest))
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
    // The record must be in the public manifest's bijection at consume
    // time; here the retained inventory is structurally closed.
    if !pin(&document.ownership_digest) {
        return Err(invalid("ownership-digest"));
    }
    if let Some(lock) = document.lock_pin.value() {
        if !pin(lock) {
            return Err(invalid("lock-pin"));
        }
    } else {
        // Generation identity carries the exact lock revision: same IR +
        // lock is the determinism contract, so a record without its lock
        // pin is not a complete generation (finding 8).
        return Err(invalid("lock-pin"));
    }
    Ok(document)
}

/// Validate a transaction journal entry, including every nested pin (finding
/// 15): a journal whose plan/generation/manifest/conflict/before fields are
/// malformed is custody corruption, never a recoverable publication — the
/// schema's nested constraints are enforced at runtime too. The journal
/// identity is the emitted grammar only (`[a-z0-9-]+`, single path
/// component): the unresolved scanner reads exactly the top level of
/// `transactions/`, so a journal id carrying separators could only ever
/// name a file the scanner never looks at (r4 finding 11).
pub fn parse_transaction(bytes: &[u8]) -> Result<Transaction, DomainResult> {
    let document: Transaction = decode(bytes, "managed-transaction")?;
    if document.transaction_id.is_empty()
        || document.transaction_id.len() > 64
        || !document
            .transaction_id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err(invalid("transaction-id"));
    }
    if let Some(plan) = document.plan_digest.value() {
        if !pin(plan) {
            return Err(invalid("transaction-pin"));
        }
    }
    if let Some(generation) = document.generation_digest.value() {
        if !pin(&generation.generation_digest) {
            return Err(invalid("transaction-pin"));
        }
    }
    for manifest in [
        document.before_manifest_digest.value(),
        document.after_manifest_digest.value(),
        document.before_digest.value(),
    ]
    .into_iter()
    .flatten()
    {
        if !pin(manifest) {
            return Err(invalid("transaction-pin"));
        }
    }
    if let Some(conflict) = document.conflict.value() {
        if !token(&conflict.reason)
            || (!rel_path(&conflict.path) && conflict.path != custody::MANIFEST_LOGICAL)
        {
            return Err(invalid("transaction-conflict"));
        }
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
        || document
            .pending
            .iter()
            .any(|p| !rel_path(&p.path) && p.path != custody::MANIFEST_LOGICAL)
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
    // The executed-outcome receipts (r4 finding 2): one per scenario, no
    // more and no fewer, each pinning the digest of the observed output
    // of that scenario's retained executable test. The labels stay
    // caller-authored; the receipts are what the gate recomputes.
    if document.execution.len() != document.scenarios.len()
        || document.execution.len() > 4096
        || !sorted(
            &document
                .execution
                .iter()
                .map(|s| &s.scenario)
                .collect::<Vec<_>>(),
        )
        || document
            .execution
            .iter()
            .any(|s| !token(&s.scenario) || !pin(&s.observed))
        || document
            .execution
            .iter()
            .map(|s| s.scenario.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            != document
                .scenarios
                .iter()
                .map(|s| s.scenario.as_str())
                .collect::<std::collections::BTreeSet<_>>()
    {
        return Err(invalid("execution-order"));
    }
    if !sorted(&document.gaps) || document.gaps.iter().any(|g| !token(g)) {
        return Err(invalid("gap-order"));
    }
    Ok(document)
}
