//! Wire normalization of the foreign/custom implementation attachment
//! (issue #30).
//!
//! [`from_value`] is the single entry from parsed JSON to the typed
//! [`ImplementationDocument`](super::ImplementationDocument). It fails
//! closed before any cross-contract processing: unknown or missing
//! members, wrong identities, malformed identifiers, bound violations,
//! kind/member incoherence, duplicates, and ambiguous selections each
//! return one typed registered diagnostic and no partial document.
//! Grammar enforcement mirrors the published JSON Schema exactly; the
//! target symbol reference additionally refuses every reserved
//! scheme-like spelling, so a binding can never carry a command, setup
//! script, or URL.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::implementation::diagnostic;
use crate::implementation::version::{self, MAX_CONTRACTS, MAX_EFFECT_REFS, MAX_TARGETS};
use crate::implementation::{
    HookContract, ImplementationDocument, ImplementationKind, ModelPin, TargetBinding,
};
use crate::ir;
use crate::lockfile::types::Sha256Digest;
use crate::scenario::id::SemanticId;

type WireResult<T> = Result<T, DiagnosticSet>;

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "modelRef",
    "irRef",
    "contracts",
];

/// The mandatory top-level members.
const TOP_LEVEL_REQUIRED: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "modelRef",
    "irRef",
    "contracts",
];

/// The closed hook-contract member set.
const CONTRACT_KEYS: &[&str] = &["symbol", "contract", "effects", "targets"];

/// The mandatory hook-contract members.
const CONTRACT_REQUIRED: &[&str] = &["symbol", "contract", "targets"];

/// The closed target-binding member set.
const TARGET_KEYS: &[&str] = &["target", "kind", "symbol", "port", "selected"];

/// The mandatory target-binding members.
const TARGET_REQUIRED: &[&str] = &["target", "kind"];

/// The closed scheme spellings a target symbol may never carry: a
/// binding declares a code reference, never a command, setup script, or
/// URL (checked case-insensitively before the first `:`).
const REFUSED_SCHEMES: &[&str] = &[
    "cargo",
    "cmd",
    "data",
    "deno",
    "dotnet",
    "eval",
    "exec",
    "file",
    "ftp",
    "go",
    "http",
    "https",
    "jar",
    "java",
    "javascript",
    "node",
    "npm",
    "npx",
    "perl",
    "powershell",
    "process",
    "pwsh",
    "python",
    "python3",
    "ruby",
    "run",
    "sh",
    "spawn",
    "ssh",
    "system",
    "zsh",
];

/// The bytes a target symbol may carry beyond the first position.
fn is_symbol_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'_' | b'@' | b'#' | b'.' | b':' | b'(' | b')' | b'/' | b'-' | b'\\'
        )
}

/// The closed target-symbol reference grammar: bounded, printable, no
/// whitespace or shell metacharacters, no traversal dots, and no
/// scheme-like prefix (`exec:`, `file:`, `https:`, ...). The returned
/// tag is a fixed refusal reason.
pub(crate) fn check_target_symbol(text: &str) -> Result<(), &'static str> {
    let bytes = text.as_bytes();
    if !(3..=256).contains(&bytes.len()) {
        return Err("length");
    }
    let first = bytes[0];
    if !(first.is_ascii_alphanumeric() || matches!(first, b'_' | b'@' | b'#')) {
        return Err("charset");
    }
    if !bytes[1..].iter().all(|&byte| is_symbol_byte(byte)) {
        return Err("charset");
    }
    if text.contains("..") {
        return Err("traversal");
    }
    if let Some(colon) = text.find(':') {
        let prefix = &text[..colon];
        let scheme_shaped = !prefix.is_empty()
            && prefix.len() <= 11
            && prefix.bytes().all(|byte| byte.is_ascii_lowercase());
        if scheme_shaped && REFUSED_SCHEMES.contains(&prefix) {
            return Err("scheme");
        }
    }
    Ok(())
}

/// One closed lowercase segment: `^[a-z][a-z0-9_]{0,62}$`.
pub(crate) fn grammar_segment(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// The closed operation-symbol grammar: the exact Model 1.0.0
/// `symbolId` pattern (two or three lowercase segments).
pub(crate) fn is_operation_symbol(text: &str) -> bool {
    if text.len() > 191 {
        return false;
    }
    let segments: Vec<&str> = text.split('.').collect();
    (2..=3).contains(&segments.len()) && segments.iter().all(|segment| grammar_segment(segment))
}

/// The closed hook-name grammar: dotted lowercase name plus a major
/// version (`schedule.calculator/v1`).
pub(crate) fn is_hook_name(text: &str) -> bool {
    if text.len() > 191 {
        return false;
    }
    let Some((name, major)) = text.rsplit_once("/v") else {
        return false;
    };
    !major.is_empty() && major.len() <= 9 && major.bytes().all(|byte| byte.is_ascii_digit()) && {
        let segments: Vec<&str> = name.split('.').collect();
        (2..=8).contains(&segments.len()) && segments.iter().all(|segment| grammar_segment(segment))
    }
}

/// The closed target-name grammar (the published `targetName` pattern).
pub(crate) fn is_target_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes[0].is_ascii_lowercase()
        && bytes[1..].iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_' || *byte == b'-'
        })
}

/// The closed port-name grammar.
pub(crate) fn is_port_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// Normalize one wire document into a validated attachment, or return
/// the typed rejection set with no partial document. Pure: no source,
/// model, cache, report, network, process, or target access of any kind.
pub fn from_value(json: &Json) -> WireResult<ImplementationDocument> {
    let object = as_object(json, "top-level")?;
    require_members(object, TOP_LEVEL_KEYS, TOP_LEVEL_REQUIRED)?;
    check_member(object, version::SCHEMA_VERSION, "schemaVersion")?;
    check_member(object, version::IDENTITY, "identity")?;

    let project_id = SemanticId::parse_root(text_member(object, "projectId")?)
        .map_err(|_| diagnostic::document_invalid("project-id", None))?;

    let model_ref = decode_model_ref(object)?;
    let ir_digest = decode_ir_ref(object)?;

    let contracts_value = object
        .get("contracts")
        .ok_or_else(|| diagnostic::document_invalid("contract-shape", None))?;
    let Json::Array(items) = contracts_value else {
        return Err(diagnostic::document_invalid("contract-shape", None));
    };
    if items.len() > MAX_CONTRACTS {
        return Err(diagnostic::document_invalid("bounds", None));
    }
    let mut contracts = Vec::with_capacity(items.len());
    for item in items {
        contracts.push(decode_contract(item)?);
    }
    contracts.sort_by(|left, right| {
        (left.symbol.as_str(), left.contract.as_str())
            .cmp(&(right.symbol.as_str(), right.contract.as_str()))
    });
    if contracts
        .windows(2)
        .any(|pair| pair[0].symbol == pair[1].symbol && pair[0].contract == pair[1].contract)
    {
        return Err(diagnostic::document_invalid("duplicate-contract", None));
    }
    check_selections(&contracts)?;

    Ok(ImplementationDocument {
        project_id,
        model_ref,
        ir_digest,
        contracts,
    })
}

/// Enforce explicit selection: two hook contracts may bind the same
/// operation on the same target only when exactly one of the competing
/// bindings carries `selected`; a selection with no competitor is
/// refused.
fn check_selections(contracts: &[HookContract]) -> WireResult<()> {
    let mut diagnostics: Vec<crate::diagnostics::Diagnostic> = Vec::new();
    for binding_pair in competing_pairs(contracts) {
        let selected = usize::from(binding_pair.0.selected) + usize::from(binding_pair.1.selected);
        if selected != 1 {
            diagnostic::push_selection_ambiguous(
                &mut diagnostics,
                pair_symbol(contracts, binding_pair.0),
                &binding_pair.0.target,
            );
        }
    }
    for contract in contracts {
        for binding in &contract.targets {
            let competitor = contracts.iter().any(|other| {
                !std::ptr::eq(other, contract)
                    && other.symbol == contract.symbol
                    && other
                        .targets
                        .iter()
                        .any(|candidate| candidate.target == binding.target)
            });
            if binding.selected && !competitor {
                diagnostic::push_selection_unknown(
                    &mut diagnostics,
                    &contract.symbol,
                    &binding.target,
                );
            }
        }
    }
    if diagnostics.is_empty() {
        return Ok(());
    }
    Err(diagnostic::finish(
        diagnostics,
        crate::result::Status::Invalid,
    ))
}

/// The operation symbol of one competing binding's hook contract.
fn pair_symbol<'a>(contracts: &'a [HookContract], binding: &TargetBinding) -> &'a str {
    contracts
        .iter()
        .find(|contract| {
            contract
                .targets
                .iter()
                .any(|candidate| std::ptr::eq(candidate, binding))
        })
        .map(|contract| contract.symbol.as_str())
        .unwrap_or_default()
}

/// Decode one hook contract with its target bindings.
fn decode_contract(item: &Json) -> WireResult<HookContract> {
    let object = as_object(item, "contract-shape")?;
    require_members(object, CONTRACT_KEYS, CONTRACT_REQUIRED)?;

    let symbol = text_member(object, "symbol")?.to_owned();
    if !is_operation_symbol(&symbol) {
        return Err(diagnostic::document_invalid(
            "operation-symbol",
            Some(&symbol),
        ));
    }
    let contract = text_member(object, "contract")?.to_owned();
    if !is_hook_name(&contract) {
        return Err(diagnostic::document_invalid("hook-name", Some(&contract)));
    }

    let effects = match object.get("effects") {
        None => Vec::new(),
        Some(Json::Array(items)) => {
            if items.len() > MAX_EFFECT_REFS {
                return Err(diagnostic::document_invalid("bounds", None));
            }
            let mut effects = Vec::with_capacity(items.len());
            for item in items {
                let text = item
                    .as_str()
                    .ok_or_else(|| diagnostic::document_invalid("effect-ref", None))?
                    .to_owned();
                if !is_operation_symbol(&text) {
                    return Err(diagnostic::document_invalid("effect-ref", Some(&text)));
                }
                effects.push(text);
            }
            effects.sort();
            effects.dedup();
            effects
        }
        Some(_) => return Err(diagnostic::document_invalid("effect-ref", None)),
    };

    let targets_value = object
        .get("targets")
        .ok_or_else(|| diagnostic::document_invalid("target-shape", None))?;
    let Json::Array(items) = targets_value else {
        return Err(diagnostic::document_invalid("target-shape", None));
    };
    if items.is_empty() || items.len() > MAX_TARGETS {
        return Err(diagnostic::document_invalid("bounds", None));
    }
    let mut targets = Vec::with_capacity(items.len());
    for item in items {
        targets.push(decode_target(item)?);
    }
    targets.sort_by(|left, right| left.target.cmp(&right.target));
    if targets
        .windows(2)
        .any(|pair| pair[0].target == pair[1].target)
    {
        return Err(diagnostic::document_invalid(
            "duplicate-target",
            Some(&symbol),
        ));
    }

    Ok(HookContract {
        symbol,
        contract,
        effects,
        targets,
    })
}

/// Decode one target binding and enforce kind/member coherence: a
/// foreign binding names a symbol, an external binding names a port,
/// and every other kind carries neither.
fn decode_target(item: &Json) -> WireResult<TargetBinding> {
    let object = as_object(item, "target-shape")?;
    require_members(object, TARGET_KEYS, TARGET_REQUIRED)?;

    let target = text_member(object, "target")?.to_owned();
    if !is_target_name(&target) {
        return Err(diagnostic::document_invalid("target-name", Some(&target)));
    }
    let kind = ImplementationKind::parse(text_member(object, "kind")?)
        .ok_or_else(|| diagnostic::document_invalid("kind", Some(&target)))?;

    let symbol = match object.get("symbol") {
        None => None,
        Some(value) => {
            let text = value
                .as_str()
                .ok_or_else(|| diagnostic::symbol_invalid(&target, "type"))?
                .to_owned();
            if let Err(reason) = check_target_symbol(&text) {
                return Err(diagnostic::symbol_invalid(&target, reason));
            }
            Some(text)
        }
    };
    let port = match object.get("port") {
        None => None,
        Some(value) => {
            let text = value
                .as_str()
                .ok_or_else(|| diagnostic::document_invalid("port-name", Some(&target)))?
                .to_owned();
            if !is_port_name(&text) {
                return Err(diagnostic::document_invalid("port-name", Some(&text)));
            }
            Some(text)
        }
    };
    let coherent = match kind {
        ImplementationKind::Foreign => symbol.is_some() && port.is_none(),
        ImplementationKind::External => symbol.is_none() && port.is_some(),
        ImplementationKind::Generated
        | ImplementationKind::Custom
        | ImplementationKind::Unsupported => symbol.is_none() && port.is_none(),
    };
    if !coherent {
        return Err(diagnostic::document_invalid("kind-members", Some(&target)));
    }

    let selected = match object.get("selected") {
        None => false,
        Some(Json::Bool(value)) => *value,
        Some(_) => return Err(diagnostic::document_invalid("selected", Some(&target))),
    };

    Ok(TargetBinding {
        target,
        kind,
        symbol,
        port,
        selected,
    })
}

/// Every pair of target bindings from distinct hook contracts of the
/// same operation that bind the same target, in sorted document order.
fn competing_pairs(contracts: &[HookContract]) -> Vec<(&TargetBinding, &TargetBinding)> {
    let mut pairs = Vec::new();
    for (index, left) in contracts.iter().enumerate() {
        for right in contracts.iter().skip(index + 1) {
            if left.symbol != right.symbol {
                continue;
            }
            for binding in &left.targets {
                for other in &right.targets {
                    if binding.target == other.target {
                        pairs.push((binding, other));
                    }
                }
            }
        }
    }
    pairs
}

/// Decode the bound Model pin.
fn decode_model_ref(object: &serde_json::Map<String, Json>) -> WireResult<ModelPin> {
    let value = object
        .get("modelRef")
        .ok_or_else(|| diagnostic::document_invalid("model-ref", None))?;
    let Some(inner) = value.as_object() else {
        return Err(diagnostic::document_invalid("model-ref", None));
    };
    if inner.len() != 2 {
        return Err(diagnostic::document_invalid("model-ref", None));
    }
    let version = text_member(inner, "modelVersion")?;
    let pin = match version {
        "0.1.0" => crate::scenario::ModelPin::V0_1_0,
        "1.0.0" => crate::scenario::ModelPin::V1_0_0,
        _ => return Err(diagnostic::document_invalid("model-ref", None)),
    };
    let digest = decode_digest(inner)?;
    Ok(ModelPin {
        version: pin,
        digest,
    })
}
/// Decode the bound IR digest reference.
fn decode_ir_ref(object: &serde_json::Map<String, Json>) -> WireResult<Sha256Digest> {
    let value = object
        .get("irRef")
        .ok_or_else(|| diagnostic::document_invalid("ir-ref", None))?;
    let Some(inner) = value.as_object() else {
        return Err(diagnostic::document_invalid("ir-ref", None));
    };
    if inner.len() != 2 || text_member(inner, "identity")? != ir::IDENTITY {
        return Err(diagnostic::document_invalid("ir-ref", None));
    }
    decode_digest(inner)
}

/// Decode one exact `sha256:<64 lowercase hex>` digest member.
fn decode_digest(object: &serde_json::Map<String, Json>) -> WireResult<Sha256Digest> {
    let text = text_member(object, "digest")?;
    Sha256Digest::parse(text).map_err(|_| diagnostic::document_invalid("digest", None))
}

/// Read one string member or fail the document.
fn text_member<'a>(object: &'a serde_json::Map<String, Json>, key: &str) -> WireResult<&'a str> {
    object
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::document_invalid("member-type", Some(key)))
}

/// Require a JSON object, reject unknown members, and require the
/// mandatory ones (closed surface: unknown fields never pass).
fn require_members(
    object: &serde_json::Map<String, Json>,
    allowed: &[&str],
    required: &[&str],
) -> WireResult<()> {
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(diagnostic::document_invalid("unknown-member", Some(key)));
        }
    }
    for key in required {
        if !object.contains_key(*key) {
            return Err(diagnostic::document_invalid("missing-member", Some(key)));
        }
    }
    Ok(())
}

/// Verify one exact wire discriminator member.
fn check_member(
    object: &serde_json::Map<String, Json>,
    expected: &str,
    key: &str,
) -> WireResult<()> {
    if text_member(object, key)? != expected {
        return Err(diagnostic::document_invalid("identity", Some(key)));
    }
    Ok(())
}

/// Reject a JSON object shape or fail the document.
fn as_object<'a>(
    value: &'a Json,
    reason: &'static str,
) -> WireResult<&'a serde_json::Map<String, Json>> {
    value
        .as_object()
        .ok_or_else(|| diagnostic::document_invalid(reason, None))
}
