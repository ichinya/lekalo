//! Canonical serialization of the semantic diff result (issue #18).
//!
//! Compact UTF-8 JSON, no insignificant whitespace, object keys in
//! unsigned UTF-8 byte order, records in canonical order. The bytes are
//! path-independent: no physical root, raw source, timestamp, host,
//! locale, runtime value, or adapter transcript ever enters them. The CLI
use super::change::{ChangeRecord, Summary};
use super::compatibility::{BlockedOn, MigrationHint, ProfileDecision, ProfileOutcome};
use super::version::MAX_EXPORT_BYTES;
use super::DiffResult;
use crate::diagnostics::DiagnosticSet;

/// Quote one string into canonical JSON bytes.
pub(crate) fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

/// Serialize the whole diff result to canonical bytes, or refuse beyond
/// the export bound.
pub fn result_bytes(result: &DiffResult) -> Result<String, DiagnosticSet> {
    let bytes = render(result);
    if bytes.len() > MAX_EXPORT_BYTES {
        return Err(super::diagnostic::export_limit_set(bytes.len()));
    }
    Ok(bytes)
}

/// The full canonical rendering with byte-sorted keys.
fn render(result: &DiffResult) -> String {
    let mut json = String::from("{\"adapters\":");
    json.push_str(&adapters_array(result));
    json.push_str(",\"affectedSeeds\":");
    json.push_str(&seeds_array(result));
    json.push_str(",\"base\":");
    json.push_str(&project_ref_bytes(&result.base_ref));
    json.push_str(",\"candidate\":");
    json.push_str(&project_ref_bytes(&result.candidate_ref));
    json.push_str(",\"changes\":");
    json.push_str(&changes_array(result));
    json.push_str(",\"classification\":");
    json.push_str(&class_array(&result.classification));
    json.push_str(",\"comparisonId\":");
    json.push_str(&quote(&result.comparison_id));
    json.push_str(",\"complete\":");
    json.push_str(if result.complete { "true" } else { "false" });
    json.push_str(",\"completeReason\":");
    json.push_str(&match result.complete_reason {
        Some(reason) => quote(reason),
        None => "null".to_owned(),
    });
    json.push_str(",\"equal\":");
    json.push_str(if result.equal { "true" } else { "false" });
    json.push_str(",\"identity\":");
    json.push_str(&quote(super::version::IDENTITY));
    json.push_str(",\"metadata\":{\"adapterCount\":");
    json.push_str(&result.adapters.len().to_string());
    json.push_str(",\"changeCount\":");
    json.push_str(&result.changes.len().to_string());
    json.push_str(",\"hintCount\":");
    json.push_str(&result.migration_hints.len().to_string());
    json.push_str(",\"profileCount\":");
    json.push_str(&result.profiles.len().to_string());
    json.push_str(",\"reasonCount\":");
    json.push_str(&result.reasons.len().to_string());
    json.push_str(",\"seedCount\":");
    json.push_str(&result.seeds.len().to_string());
    json.push_str("},\"migrationHints\":");
    json.push_str(&hints_array(result));
    json.push_str(",\"profiles\":");
    json.push_str(&profiles_array(result));
    json.push_str(",\"reasons\":");
    json.push_str(&reason_array(result));
    json.push_str(",\"schemaVersion\":");
    json.push_str(&quote(super::version::SCHEMA_VERSION));
    json.push('}');
    json
}

/// The canonical bytes of one project reference.
fn project_ref_bytes(reference: &super::ProjectRef) -> String {
    format!(
        "{{\"irDigest\":{},\"irIdentity\":{},\"modelVersion\":{},\"semanticDigest\":{}}}",
        quote(reference.ir_digest()),
        quote(reference.ir_identity()),
        quote(reference.model_version()),
        quote(reference.semantic_digest())
    )
}

/// The canonical bytes of one change record.
fn change_bytes(record: &ChangeRecord) -> String {
    format!(
        "{{\"after\":{},\"before\":{},\"changeId\":{},\"kind\":{},\"reasons\":{},\"side\":{},\"subject\":{}}}",
        summary_bytes(record.after()),
        summary_bytes(record.before()),
        quote(record.change_id()),
        quote(record.kind().key()),
        string_array(record.reasons()),
        quote(record.side().key()),
        subject_bytes(record.subject())
    )
}

/// The canonical bytes of one typed summary.
fn summary_bytes(summary: &Summary) -> String {
    match summary {
        Summary::Text(value) => quote(value),
        Summary::Flag(value) => value.to_string(),
        Summary::Count(value) => value.to_string(),
        Summary::Ids(values) => string_array(values),
        Summary::Renamed { renamed_from } => {
            format!("{{\"renamedFrom\":{}}}", string_array(renamed_from))
        }
        Summary::Replaced { replaced_by } => {
            format!("{{\"replacedBy\":{}}}", quote(replaced_by))
        }
        Summary::Tombstoned { since } => format!("{{\"since\":{since}}}"),
        Summary::None => "null".to_owned(),
    }
}

/// The canonical bytes of one subject.
fn subject_bytes(subject: &super::Subject) -> String {
    format!(
        "{{\"family\":{},\"id\":{},\"member\":{}}}",
        quote(subject.family().key()),
        quote(subject.id()),
        match subject.member() {
            Some(member) => quote(member),
            None => "null".to_owned(),
        }
    )
}

/// The canonical bytes of one seed.
fn seed_bytes(seed: &super::AffectedSeed) -> String {
    format!(
        "{{\"changeIds\":{},\"origin\":{},\"side\":{},\"subject\":{}}}",
        string_array(seed.change_ids()),
        quote(seed.origin()),
        quote(seed.side().key()),
        subject_bytes(seed.subject())
    )
}

/// The canonical bytes of one migration hint.
fn hint_bytes(hint: &MigrationHint) -> String {
    format!(
        "{{\"changeId\":{},\"detail\":{},\"hint\":{}}}",
        quote(hint.change_id()),
        quote(hint.detail()),
        quote(hint.hint())
    )
}

/// The canonical bytes of one class array.
fn class_array(classes: &[super::CompatibilityClass]) -> String {
    let rendered: Vec<String> = classes.iter().map(|class| quote(class.key())).collect();
    format!("[{}]", rendered.join(","))
}

/// The canonical bytes of a string array.
fn string_array(values: &[String]) -> String {
    let rendered: Vec<String> = values.iter().map(|value| quote(value)).collect();
    format!("[{}]", rendered.join(","))
}

/// The canonical bytes of the changes array.
fn changes_array(result: &DiffResult) -> String {
    let rendered: Vec<String> = result.changes.iter().map(change_bytes).collect();
    format!("[{}]", rendered.join(","))
}

/// The canonical bytes of the seeds array.
fn seeds_array(result: &DiffResult) -> String {
    let rendered: Vec<String> = result.seeds.iter().map(seed_bytes).collect();
    format!("[{}]", rendered.join(","))
}

/// The canonical bytes of the reasons array.
fn reason_array(result: &DiffResult) -> String {
    string_array(&result.reasons)
}

/// The canonical bytes of the hints array.
fn hints_array(result: &DiffResult) -> String {
    let rendered: Vec<String> = result.migration_hints.iter().map(hint_bytes).collect();
    format!("[{}]", rendered.join(","))
}

/// The canonical bytes of the profiles array.
fn profiles_array(result: &DiffResult) -> String {
    let rendered: Vec<String> = result.profiles.iter().map(profile_bytes).collect();
    format!("[{}]", rendered.join(","))
}

/// The canonical bytes of one profile decision.
fn profile_bytes(profile: &ProfileDecision) -> String {
    let outcomes: Vec<String> = profile.outcomes().iter().map(outcome_bytes).collect();
    let blocked = match profile.blocked_on() {
        Some(blocked) => blocked_bytes(blocked),
        None => "null".to_owned(),
    };
    format!(
        "{{\"blockedOn\":{},\"classes\":{},\"outcomes\":[{}],\"policyDigest\":{},\"policyRevision\":{},\"profileId\":{},\"profileVersion\":{},\"verdict\":{}}}",
        blocked,
        class_array(profile.classes()),
        outcomes.join(","),
        quote(profile.policy_digest()),
        quote(profile.policy_revision()),
        quote(profile.profile_id().key()),
        quote(profile.profile_version()),
        quote(profile.verdict().key())
    )
}

/// The canonical bytes of one profile outcome.
fn outcome_bytes(outcome: &ProfileOutcome) -> String {
    format!(
        "{{\"changeId\":{},\"classes\":{},\"reasons\":{}}}",
        quote(outcome.change_id()),
        class_array(outcome.classes()),
        string_array(outcome.reasons())
    )
}

/// The canonical bytes of one blocked-on record.
fn blocked_bytes(blocked: &BlockedOn) -> String {
    format!(
        "{{\"changeIds\":{},\"reason\":{}}}",
        string_array(blocked.change_ids()),
        quote(blocked.reason())
    )
}

/// The canonical bytes of the adapters array.
fn adapters_array(result: &DiffResult) -> String {
    let rendered: Vec<String> = result.adapters.iter().map(adapter_bytes).collect();
    format!("[{}]", rendered.join(","))
}

/// The canonical bytes of one recorded adapter contribution.
fn adapter_bytes(adapter: &super::adapter::RecordedAdapter) -> String {
    let effects: Vec<String> = adapter
        .effects
        .iter()
        .map(|effect| {
            format!(
                "{{\"changeId\":{},\"classes\":{},\"reasons\":[]}}",
                quote(&effect.change_id),
                class_array(&effect.classes)
            )
        })
        .collect();
    let seeds: Vec<String> = adapter
        .seeds
        .iter()
        .map(|seed| {
            format!(
                "{{\"changeIds\":[],\"origin\":{},\"side\":{},\"subject\":{}}}",
                quote("direct-diff"),
                quote(seed.side.key()),
                subject_bytes(&seed.subject)
            )
        })
        .collect();
    format!(
        "{{\"adapterId\":{},\"adapterVersion\":{},\"contributionDigest\":{},\"effects\":[{}],\"evidenceDigest\":{},\"evidenceRevision\":{},\"inputIrDigest\":{},\"namespace\":{},\"profileRef\":{},\"protocolRef\":{},\"seeds\":[{}],\"states\":{},\"targetId\":{},\"trust\":{}}}",
        quote(&adapter.adapter_id),
        quote(&adapter.adapter_version),
        quote(&adapter.contribution_digest),
        effects.join(","),
        quote(&adapter.evidence_digest),
        quote(&adapter.evidence_revision),
        quote(&adapter.input_ir_digest),
        quote(&adapter.namespace),
        quote(adapter.profile_ref.key()),
        quote(&adapter.protocol_ref),
        seeds.join(","),
        string_array(&adapter.states),
        quote(&adapter.target_id),
        quote(adapter.trust.key())
    )
}

/// The canonical policy descriptor bytes a policy digest commits to.
pub(crate) fn policy_bytes(profile: super::ProfileId) -> String {
    format!(
        "{{\"policyRevision\":{},\"profileId\":{},\"profileVersion\":{}}}",
        quote(super::version::POLICY_REVISION),
        quote(profile.key()),
        quote(super::version::PROFILE_VERSION)
    )
}
