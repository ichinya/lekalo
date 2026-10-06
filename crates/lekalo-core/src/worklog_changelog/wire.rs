//! The bounded parse seam from exact JSON bytes to the typed
//! composition attachment (issue #51).
//!
//! The seam fails closed before any semantic processing: unknown or
//! missing members, wrong identities, malformed identifiers, digests,
//! bounds, closed-vocabulary violations, and duplicate JSON keys each
//! return one typed registered diagnostic and no partial value.
//! Collections normalize to canonical order (operations by role id,
//! effects by kind then effect id, rules and scopes by their first
//! key) so equivalent documents compare equal.

use serde_json::Value as Json;

use super::types::CompositionId;
use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::types::{
    ApprovalBinding, CapabilityRequirement, Composition, ConsumedRef, Correlation,
    EffectDeclaration, IndependenceRule, OperationRole, PublicAllowlist, PublicField,
    RenderingPolicy, ReplayScope, SnapshotDecl,
};
use super::version;

/// The closed top-level member set.
const TOP_LEVEL_KEYS: &[&str] = &[
    "schemaVersion",
    "identity",
    "projectId",
    "closed",
    "modelRef",
    "irRef",
    "consumed",
    "operations",
    "independentEffects",
    "correlations",
    "approvals",
    "snapshots",
    "publicAllowlist",
    "replayScopes",
    "capabilityRequirements",
];

const OPERATION_KEYS: &[&str] = &[
    "operation",
    "effects",
    "commitBoundary",
    "defaultSelection",
    "replay",
];

const EFFECT_KEYS: &[&str] = &["kind", "effectId"];
const INDEPENDENCE_KEYS: &[&str] = &["operation", "mustNotEnable"];
const CORRELATION_KEYS: &[&str] = &["correlationId", "keyRef", "memberOperations"];
const APPROVAL_KEYS: &[&str] = &[
    "approvalId",
    "operation",
    "previewRevisionField",
    "previewDigestField",
    "tenantField",
    "destinationField",
    "authorityField",
    "policyRevisionField",
];
const SNAPSHOT_KEYS: &[&str] = &[
    "snapshotId",
    "entity",
    "revisionField",
    "digestField",
    "schemaVersionField",
    "immutability",
];
const ALLOWLIST_KEYS: &[&str] = &["dto", "fields", "excluded", "rendering"];
const PUBLIC_FIELD_KEYS: &[&str] = &["name", "type", "source"];
const RENDERING_KEYS: &[&str] = &["defaultMode", "urlPolicy"];
const URL_POLICY_KEYS: &[&str] = &["schemes", "rejectEncoded", "rejectControlChars"];
const REPLAY_KEYS: &[&str] = &[
    "operation",
    "scope",
    "keyRef",
    "duplicate",
    "sameKeyDifferentRequest",
    "payloadBinding",
];
const CAPABILITY_KEYS: &[&str] = &[
    "requirementId",
    "capability",
    "minimum",
    "reason",
    "operationRefs",
];
const CONSUMED_KEYS: &[&str] = &["family", "ref"];
const CONTRACT_REF_KEYS: &[&str] = &["identity", "version", "digest"];

/// Parse one composition document (exact UTF-8 JSON bytes) or return
/// the terminal rejection set. Duplicate JSON keys are rejected at
/// decode time, before any map insertion can hide them.
pub fn parse(bytes: &[u8]) -> Result<Composition, DiagnosticSet> {
    if bytes.len() > version::MAX_DOC_BYTES {
        return Err(diagnostic::input_invalid("document-bytes"));
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| diagnostic::input_invalid("invalid-encoding"))?;
    let json = super::canonical::parse_unique(text)?;
    from_value(&json)
}

/// The one entry from a decoded, duplicate-free value to the typed
/// attachment.
pub(crate) fn from_value(json: &Json) -> Result<Composition, DiagnosticSet> {
    let object = json
        .as_object()
        .ok_or_else(|| diagnostic::input_invalid("top-level-shape"))?;
    check_members(object, TOP_LEVEL_KEYS)?;
    if object.get("schemaVersion").and_then(Json::as_str) != Some(version::SCHEMA_VERSION) {
        return Err(diagnostic::input_invalid("schema-version"));
    }
    if object.get("identity").and_then(Json::as_str) != Some(version::IDENTITY) {
        return Err(diagnostic::input_invalid("contract-identity"));
    }
    if object.get("closed") != Some(&Json::Bool(true)) {
        return Err(diagnostic::input_invalid("closed-required"));
    }
    let project_id = require_str(object, "projectId")?;
    let model_ref = object
        .get("modelRef")
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("model-ref-shape"))?;
    check_members(model_ref, &["modelVersion", "digest"])?;
    if model_ref.get("modelVersion").and_then(Json::as_str) != Some(version::MODEL_VERSION) {
        return Err(diagnostic::input_invalid("model-version"));
    }
    let model_digest = digest_pin(model_ref, "model-digest")?;
    let ir_ref = object
        .get("irRef")
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("ir-ref-shape"))?;
    check_members(ir_ref, &["identity", "digest"])?;
    if ir_ref.get("identity").and_then(Json::as_str) != Some(version::IR_IDENTITY) {
        return Err(diagnostic::input_invalid("ir-identity"));
    }
    let ir_digest = digest_pin(ir_ref, "ir-digest")?;

    let consumed = consume_all(object.get("consumed"))?;
    let operations = parse_operations(object.get("operations"))?;
    let independent_effects = parse_independence(object.get("independentEffects"))?;
    let correlations = parse_correlations(object.get("correlations"))?;
    let approvals = parse_approvals(object.get("approvals"))?;
    let snapshots = parse_snapshots(object.get("snapshots"))?;
    let public_allowlist = parse_allowlist(object.get("publicAllowlist"))?;
    let replay_scopes = parse_replay(object.get("replayScopes"))?;
    let capability_requirements = parse_capabilities(object.get("capabilityRequirements"))?;

    Composition::assemble(
        project_id,
        model_digest,
        ir_digest,
        consumed,
        operations,
        independent_effects,
        correlations,
        approvals,
        snapshots,
        public_allowlist,
        replay_scopes,
        capability_requirements,
    )
}

/// Reject every member outside the closed set.
pub(crate) fn check_members(
    object: &serde_json::Map<String, Json>,
    keys: &[&str],
) -> Result<(), DiagnosticSet> {
    for key in object.keys() {
        if !keys.contains(&key.as_str()) {
            return Err(diagnostic::input_invalid("unknown-field"));
        }
    }
    Ok(())
}

fn require_str<'a>(
    object: &'a serde_json::Map<String, Json>,
    key: &str,
) -> Result<&'a str, DiagnosticSet> {
    object
        .get(key)
        .and_then(Json::as_str)
        .ok_or_else(|| diagnostic::input_invalid("missing-member"))
}

/// One exact lowercase `sha256:<64 hex>` digest of some bytes.
pub(crate) fn digest_pin(
    object: &serde_json::Map<String, Json>,
    detail: &'static str,
) -> Result<String, DiagnosticSet> {
    let raw = require_str(object, "digest")?;
    let rest = raw
        .strip_prefix("sha256:")
        .filter(|rest| rest.len() == 64 && rest.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .filter(|rest| rest.bytes().all(|byte| !byte.is_ascii_uppercase()))
        .ok_or_else(|| diagnostic::input_invalid(detail))?;
    Ok(format!("sha256:{rest}"))
}

fn bounded_text(raw: &str, max: usize) -> Result<String, DiagnosticSet> {
    if raw.is_empty() || raw.len() > max || raw.chars().any(|c| c.is_control()) {
        return Err(diagnostic::input_invalid("bounded-text"));
    }
    Ok(raw.to_owned())
}

fn closed_member(
    raw: &str,
    vocabulary: &[&str],
    detail: &'static str,
) -> Result<String, DiagnosticSet> {
    if vocabulary.contains(&raw) {
        Ok(raw.to_owned())
    } else {
        Err(diagnostic::input_invalid(detail))
    }
}

fn consume_all(json: Option<&Json>) -> Result<Vec<ConsumedRef>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("consumed-shape"))?;
    if items.is_empty() || items.len() > version::MAX_CONSUMED {
        return Err(diagnostic::input_invalid("consumed-bound"));
    }
    let mut consumed = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("consumed-shape"))?;
        check_members(object, CONSUMED_KEYS)?;
        let family = closed_member(
            require_str(object, "family")?,
            version::CONSUMED_FAMILIES,
            "consumed-family",
        )?;
        let reference = object
            .get("ref")
            .and_then(Json::as_object)
            .ok_or_else(|| diagnostic::input_invalid("consumed-ref-shape"))?;
        check_members(reference, CONTRACT_REF_KEYS)?;
        let identity = require_str(reference, "identity")?;
        if !identity.starts_with("dev.lekalo.") || identity.split('@').count() != 2 {
            return Err(diagnostic::input_invalid("consumed-identity"));
        }
        let identity = bounded_text(identity, 128)?;
        let contract_version = closed_member(
            require_str(reference, "version")?,
            &["0.2.16"],
            "consumed-version",
        )?;
        let digest = digest_pin(reference, "consumed-digest")?;
        consumed.push(ConsumedRef {
            family,
            identity,
            version: contract_version,
            digest: parse_digest(&digest)?,
        });
    }
    consumed.sort_by(|left, right| left.family.cmp(&right.family));
    consumed.dedup_by(|left, right| left.family == right.family);
    Ok(consumed)
}

fn parse_digest(raw: &str) -> Result<crate::lockfile::types::Sha256Digest, DiagnosticSet> {
    crate::lockfile::types::Sha256Digest::parse(raw)
        .map_err(|_| diagnostic::input_invalid("digest-shape"))
}

fn parse_operations(json: Option<&Json>) -> Result<Vec<OperationRole>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("operations-shape"))?;
    if items.len() < version::OPERATION_ROLES.len() || items.len() > version::MAX_OPERATIONS {
        return Err(diagnostic::input_invalid("operations-bound"));
    }
    let mut operations = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("operations-shape"))?;
        check_members(object, OPERATION_KEYS)?;
        let operation = bounded_text(require_str(object, "operation")?, 128)?;
        if !version::OPERATION_ROLES.contains(&operation.as_str()) {
            return Err(diagnostic::input_invalid("unknown-role"));
        }
        let effects_json = object
            .get("effects")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("effects-shape"))?;
        if effects_json.len() > 16 {
            return Err(diagnostic::input_invalid("effects-bound"));
        }
        let mut effects = Vec::with_capacity(effects_json.len());
        for effect in effects_json {
            let effect_object = effect
                .as_object()
                .ok_or_else(|| diagnostic::input_invalid("effects-shape"))?;
            check_members(effect_object, EFFECT_KEYS)?;
            let kind = closed_member(
                require_str(effect_object, "kind")?,
                version::EFFECT_KINDS,
                "effect-kind",
            )?;
            let effect_id = CompositionId::parse(require_str(effect_object, "effectId")?)
                .ok_or_else(|| diagnostic::input_invalid("effect-id"))?;
            effects.push(EffectDeclaration { kind, effect_id });
        }
        effects.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then(left.effect_id.cmp(&right.effect_id))
        });
        let commit_boundary = closed_member(
            require_str(object, "commitBoundary")?,
            version::COMMIT_BOUNDARIES,
            "commit-boundary",
        )?;
        let default_selection = match object.get("defaultSelection") {
            Some(Json::Bool(false)) | None => false,
            _ => return Err(diagnostic::input_invalid("default-selection")),
        };
        operations.push(OperationRole {
            operation,
            effects,
            commit_boundary,
            default_selection,
        });
    }
    operations.sort_by(|left, right| left.operation.cmp(&right.operation));
    operations.dedup_by(|left, right| left.operation == right.operation);
    Ok(operations)
}

fn parse_independence(json: Option<&Json>) -> Result<Vec<IndependenceRule>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("independence-shape"))?;
    if items.is_empty() || items.len() > version::MAX_INDEPENDENCE_RULES {
        return Err(diagnostic::input_invalid("independence-bound"));
    }
    let mut rules = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("independence-shape"))?;
        check_members(object, INDEPENDENCE_KEYS)?;
        let operation = bounded_text(require_str(object, "operation")?, 128)?;
        if !version::OPERATION_ROLES.contains(&operation.as_str()) {
            return Err(diagnostic::input_invalid("unknown-role"));
        }
        let targets = object
            .get("mustNotEnable")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("independence-shape"))?;
        if targets.is_empty() || targets.len() > 16 {
            return Err(diagnostic::input_invalid("independence-bound"));
        }
        let mut must_not_enable = Vec::with_capacity(targets.len());
        for target in targets {
            let target = target
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("independence-shape"))?;
            if !version::OPERATION_ROLES.contains(&target) {
                return Err(diagnostic::input_invalid("unknown-role"));
            }
            must_not_enable.push(target.to_owned());
        }
        must_not_enable.sort_unstable();
        must_not_enable.dedup();
        rules.push(IndependenceRule {
            operation,
            must_not_enable,
        });
    }
    rules.sort_by(|left, right| left.operation.cmp(&right.operation));
    rules.dedup_by(|left, right| left.operation == right.operation);
    Ok(rules)
}

fn parse_correlations(json: Option<&Json>) -> Result<Vec<Correlation>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("correlations-shape"))?;
    if items.len() > version::MAX_CORRELATIONS {
        return Err(diagnostic::input_invalid("correlations-bound"));
    }
    let mut correlations = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("correlations-shape"))?;
        check_members(object, CORRELATION_KEYS)?;
        let correlation_id = CompositionId::parse(require_str(object, "correlationId")?)
            .ok_or_else(|| diagnostic::input_invalid("correlation-id"))?;
        let key_ref = bounded_text(require_str(object, "keyRef")?, 64)?;
        let members = object
            .get("memberOperations")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("correlations-shape"))?;
        if members.is_empty() || members.len() > 16 {
            return Err(diagnostic::input_invalid("correlations-bound"));
        }
        let mut member_operations = Vec::with_capacity(members.len());
        for member in members {
            let member = member
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("correlations-shape"))?;
            if !version::OPERATION_ROLES.contains(&member) {
                return Err(diagnostic::input_invalid("unknown-role"));
            }
            member_operations.push(member.to_owned());
        }
        member_operations.sort_unstable();
        member_operations.dedup();
        correlations.push(Correlation {
            correlation_id,
            key_ref,
            member_operations,
        });
    }
    correlations.sort_by(|left, right| left.correlation_id.cmp(&right.correlation_id));
    correlations.dedup_by(|left, right| left.correlation_id == right.correlation_id);
    Ok(correlations)
}

fn parse_approvals(json: Option<&Json>) -> Result<Vec<ApprovalBinding>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("approvals-shape"))?;
    if items.len() > version::MAX_APPROVALS {
        return Err(diagnostic::input_invalid("approvals-bound"));
    }
    let mut approvals = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("approvals-shape"))?;
        check_members(object, APPROVAL_KEYS)?;
        let approval_id = CompositionId::parse(require_str(object, "approvalId")?)
            .ok_or_else(|| diagnostic::input_invalid("approval-id"))?;
        let operation = bounded_text(require_str(object, "operation")?, 128)?;
        if operation != "changelog.publish_entry" {
            return Err(diagnostic::input_invalid("approval-operation"));
        }
        let field = |key: &str| bounded_text(require_str(object, key)?, 64);
        approvals.push(ApprovalBinding {
            approval_id,
            operation,
            preview_revision_field: field("previewRevisionField")?,
            preview_digest_field: field("previewDigestField")?,
            tenant_field: field("tenantField")?,
            destination_field: field("destinationField")?,
            authority_field: field("authorityField")?,
            policy_revision_field: field("policyRevisionField")?,
        });
    }
    approvals.sort_by(|left, right| left.approval_id.cmp(&right.approval_id));
    approvals.dedup_by(|left, right| left.approval_id == right.approval_id);
    Ok(approvals)
}

fn parse_snapshots(json: Option<&Json>) -> Result<Vec<SnapshotDecl>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("snapshots-shape"))?;
    if items.len() > version::MAX_SNAPSHOTS {
        return Err(diagnostic::input_invalid("snapshots-bound"));
    }
    let mut snapshots = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("snapshots-shape"))?;
        check_members(object, SNAPSHOT_KEYS)?;
        let snapshot_id = CompositionId::parse(require_str(object, "snapshotId")?)
            .ok_or_else(|| diagnostic::input_invalid("snapshot-id"))?;
        let entity = CompositionId::parse(require_str(object, "entity")?)
            .ok_or_else(|| diagnostic::input_invalid("snapshot-entity"))?;
        let immutability = closed_member(
            require_str(object, "immutability")?,
            &["append_only"],
            "snapshot-immutability",
        )?;
        let field = |key: &str| bounded_text(require_str(object, key)?, 64);
        snapshots.push(SnapshotDecl {
            snapshot_id,
            entity,
            revision_field: field("revisionField")?,
            digest_field: field("digestField")?,
            schema_version_field: field("schemaVersionField")?,
            immutability,
        });
    }
    snapshots.sort_by(|left, right| left.snapshot_id.cmp(&right.snapshot_id));
    snapshots.dedup_by(|left, right| left.snapshot_id == right.snapshot_id);
    Ok(snapshots)
}

fn parse_allowlist(json: Option<&Json>) -> Result<PublicAllowlist, DiagnosticSet> {
    let object = json
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("allowlist-shape"))?;
    check_members(object, ALLOWLIST_KEYS)?;
    let dto = CompositionId::parse(require_str(object, "dto")?)
        .ok_or_else(|| diagnostic::input_invalid("allowlist-dto"))?;
    let fields_json = object
        .get("fields")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("allowlist-fields-shape"))?;
    if fields_json.is_empty() || fields_json.len() > version::MAX_ALLOWLIST_FIELDS {
        return Err(diagnostic::input_invalid("allowlist-fields-bound"));
    }
    let mut fields = Vec::with_capacity(fields_json.len());
    for field_json in fields_json {
        let field_object = field_json
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("allowlist-fields-shape"))?;
        check_members(field_object, PUBLIC_FIELD_KEYS)?;
        let name = bounded_text(require_str(field_object, "name")?, 64)?;
        let kind = bounded_text(require_str(field_object, "type")?, 128)?;
        let source = closed_member(
            require_str(field_object, "source")?,
            &["snapshot", "reviewed_projection"],
            "allowlist-source",
        )?;
        fields.push(PublicField { name, kind, source });
    }
    fields.sort_by(|left, right| left.name.cmp(&right.name));
    fields.dedup_by(|left, right| left.name == right.name);
    let excluded_json = object
        .get("excluded")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("allowlist-excluded-shape"))?;
    if excluded_json.is_empty() || excluded_json.len() > version::MAX_EXCLUDED_FIELDS {
        return Err(diagnostic::input_invalid("allowlist-excluded-bound"));
    }
    let mut excluded = Vec::with_capacity(excluded_json.len());
    for field in excluded_json {
        let field = field
            .as_str()
            .ok_or_else(|| diagnostic::input_invalid("allowlist-excluded-shape"))?;
        excluded.push(bounded_text(field, 64)?);
    }
    excluded.sort_unstable();
    excluded.dedup();
    let rendering_json = object
        .get("rendering")
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("rendering-shape"))?;
    check_members(rendering_json, RENDERING_KEYS)?;
    let default_mode = closed_member(
        require_str(rendering_json, "defaultMode")?,
        &["plain_text"],
        "rendering-mode",
    )?;
    let url_policy = rendering_json
        .get("urlPolicy")
        .and_then(Json::as_object)
        .ok_or_else(|| diagnostic::input_invalid("url-policy-shape"))?;
    check_members(url_policy, URL_POLICY_KEYS)?;
    let schemes_json = url_policy
        .get("schemes")
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("url-schemes-shape"))?;
    if schemes_json.is_empty() || schemes_json.len() > 8 {
        return Err(diagnostic::input_invalid("url-schemes-bound"));
    }
    let mut url_schemes = Vec::with_capacity(schemes_json.len());
    for scheme in schemes_json {
        let scheme = closed_member(
            scheme
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("url-schemes-shape"))?,
            &["https", "http"],
            "url-scheme",
        )?;
        url_schemes.push(scheme);
    }
    url_schemes.sort_unstable();
    url_schemes.dedup();
    if url_policy.get("rejectEncoded") != Some(&Json::Bool(true))
        || url_policy.get("rejectControlChars") != Some(&Json::Bool(true))
    {
        return Err(diagnostic::input_invalid("url-policy-closed"));
    }
    Ok(PublicAllowlist {
        dto,
        fields,
        excluded,
        rendering: RenderingPolicy {
            default_mode,
            url_schemes,
            reject_encoded: true,
            reject_control_chars: true,
        },
    })
}

fn parse_replay(json: Option<&Json>) -> Result<Vec<ReplayScope>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("replay-shape"))?;
    if items.is_empty() || items.len() > version::MAX_REPLAY_SCOPES {
        return Err(diagnostic::input_invalid("replay-bound"));
    }
    let mut scopes = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("replay-shape"))?;
        check_members(object, REPLAY_KEYS)?;
        let operation = bounded_text(require_str(object, "operation")?, 128)?;
        if !version::OPERATION_ROLES.contains(&operation.as_str()) {
            return Err(diagnostic::input_invalid("unknown-role"));
        }
        let scope = CompositionId::parse(require_str(object, "scope")?)
            .ok_or_else(|| diagnostic::input_invalid("replay-scope-id"))?;
        let key_ref = bounded_text(require_str(object, "keyRef")?, 64)?;
        let duplicate = closed_member(
            require_str(object, "duplicate")?,
            &["replay_result", "conflict"],
            "replay-duplicate",
        )?;
        let same_key = closed_member(
            require_str(object, "sameKeyDifferentRequest")?,
            &["conflict"],
            "replay-conflict",
        )?;
        let payload_binding = closed_member(
            require_str(object, "payloadBinding")?,
            &["canonical_digest", "none"],
            "replay-payload-binding",
        )?;
        scopes.push(ReplayScope {
            operation,
            scope,
            key_ref,
            duplicate,
            same_key_different_request: same_key,
            payload_binding,
        });
    }
    scopes.sort_by(|left, right| left.operation.cmp(&right.operation));
    scopes.dedup_by(|left, right| left.operation == right.operation);
    Ok(scopes)
}

fn parse_capabilities(json: Option<&Json>) -> Result<Vec<CapabilityRequirement>, DiagnosticSet> {
    let items = json
        .and_then(Json::as_array)
        .ok_or_else(|| diagnostic::input_invalid("capabilities-shape"))?;
    if items.len() > version::MAX_CAPABILITY_REQUIREMENTS {
        return Err(diagnostic::input_invalid("capabilities-bound"));
    }
    let mut requirements = Vec::with_capacity(items.len());
    for item in items {
        let object = item
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("capabilities-shape"))?;
        check_members(object, CAPABILITY_KEYS)?;
        let requirement_id = CompositionId::parse(require_str(object, "requirementId")?)
            .ok_or_else(|| diagnostic::input_invalid("requirement-id"))?;
        let capability = bounded_text(require_str(object, "capability")?, 96)?;
        let minimum = closed_member(
            require_str(object, "minimum")?,
            &["full", "partial"],
            "capability-minimum",
        )?;
        let reason = bounded_text(require_str(object, "reason")?, 2000)?;
        let operation_refs_json = object
            .get("operationRefs")
            .and_then(Json::as_array)
            .ok_or_else(|| diagnostic::input_invalid("capabilities-shape"))?;
        if operation_refs_json.is_empty() || operation_refs_json.len() > 16 {
            return Err(diagnostic::input_invalid("capabilities-bound"));
        }
        let mut operation_refs = Vec::with_capacity(operation_refs_json.len());
        for operation in operation_refs_json {
            let operation = operation
                .as_str()
                .ok_or_else(|| diagnostic::input_invalid("capabilities-shape"))?;
            if !version::OPERATION_ROLES.contains(&operation) {
                return Err(diagnostic::input_invalid("unknown-role"));
            }
            operation_refs.push(operation.to_owned());
        }
        operation_refs.sort_unstable();
        operation_refs.dedup();
        requirements.push(CapabilityRequirement {
            requirement_id,
            capability,
            minimum,
            reason,
            operation_refs,
        });
    }
    requirements.sort_by(|left, right| left.requirement_id.cmp(&right.requirement_id));
    requirements.dedup_by(|left, right| left.requirement_id == right.requirement_id);
    Ok(requirements)
}
