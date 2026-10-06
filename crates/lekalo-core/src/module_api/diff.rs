//! Public API diff: exact changes, affected consumers, and policy verdicts
//! (issue #113).
//!
//! `compare_public` compares two bundles of the same semantic module:
//! canonical public semantics first (layout, descriptions, and private
//! provenance never enter), per-change compatibility classes with exact
//! reasons, then the union of the before/after consumer inventories —
//! removed exports still affect old consumers, newly added consumers are
//! explicit, and declared subscriptions with zero current edges remain
//! range-policy consumers. Without both inventories the public break
//! classification stays possible but exact consumer acceptance is
//! explicitly unknown.

use super::consumers::parse_snapshot;
use super::diagnostic::Evidence;
use super::public::parse_bundle;
use super::version::{ApiVersion, RequiredBump};
use super::wire::{
    self, ConsumerImpact, ConsumerSnapshot, DiffChange, DiffCoverage, DiffReport, DiffSide,
    VersionDecision,
};
use crate::diagnostics::DiagnosticSet;

/// Compare two bundles with optional consumer snapshots. Both snapshots are
/// a pair: absence means public changes are known but consumer coverage is
/// unknown. Same-identity evidence (alias/rename history inside the
/// snapshots) resolves ID changes; private helper renames never become
/// public breaks.
pub fn compare_public(
    before_bytes: &[u8],
    after_bytes: &[u8],
    consumers_before: Option<&[u8]>,
    consumers_after: Option<&[u8]>,
) -> Result<(DiffReport, Vec<Evidence>), DiagnosticSet> {
    let before = parse_bundle(before_bytes)?;
    let after = parse_bundle(after_bytes)?;
    if before.producer.module != after.producer.module
        || before.producer.project != after.producer.project
    {
        return Err(super::diagnostic::invalid("bundle-identity"));
    }
    let before_snapshot = consumers_before.map(parse_snapshot).transpose()?;
    let after_snapshot = consumers_after.map(parse_snapshot).transpose()?;
    if consumers_before.is_some() != consumers_after.is_some() {
        return Err(super::diagnostic::invalid("consumer-pair"));
    }
    // Bind each side to its own snapshot, never to identical digests.
    if let Some(before_snapshot) = &before_snapshot {
        if before_snapshot.producer.module != before.producer.module {
            return Err(super::diagnostic::invalid("snapshot-side"));
        }
    }
    if let Some(after_snapshot) = &after_snapshot {
        if after_snapshot.producer.module != after.producer.module {
            return Err(super::diagnostic::invalid("snapshot-side"));
        }
    }
    let module = before.producer.module.clone();
    let before_map: std::collections::BTreeMap<&str, &super::wire::PublicExport> = before
        .exports
        .iter()
        .map(|export| (export.export.as_str(), export))
        .collect();
    let after_map: std::collections::BTreeMap<&str, &super::wire::PublicExport> = after
        .exports
        .iter()
        .map(|export| (export.export.as_str(), export))
        .collect();
    let mut changes: Vec<DiffChange> = Vec::new();
    let mut breaking = false;
    let mut additive = false;
    // Removed exports: a break for every old consumer.
    for (export, removed) in &before_map {
        if !after_map.contains_key(export) {
            changes.push(DiffChange {
                export: (*export).to_owned(),
                member: None,
                class: "source-breaking".to_owned(),
                reason: "export-removed".to_owned(),
                before: Some(export_kind(removed)),
                after: None,
            });
            breaking = true;
        }
    }
    // Added exports: additive surface.
    for (export, added) in &after_map {
        if !before_map.contains_key(export) {
            changes.push(DiffChange {
                export: (*export).to_owned(),
                member: None,
                class: "additive".to_owned(),
                reason: "export-added".to_owned(),
                before: None,
                after: Some(export_kind(added)),
            });
            additive = true;
        }
    }
    // Changed exports: field/shape-level comparison.
    for (export, after_export) in &after_map {
        let Some(before_export) = before_map.get(export) else {
            continue;
        };
        let (export_changes, export_breaking) = compare_exports(before_export, after_export);
        breaking |= export_breaking;
        for (member, class, reason, before_text, after_text) in export_changes {
            changes.push(DiffChange {
                export: (*export).to_owned(),
                member,
                class,
                reason,
                before: before_text,
                after: after_text,
            });
        }
    }
    // Public dependency pins: a changed transitive digest is a break.
    {
        let before_deps: std::collections::BTreeMap<&str, &str> = before
            .public_dependencies
            .iter()
            .map(|dependency| (dependency.module_id.as_str(), dependency.digest.as_str()))
            .collect();
        for dependency in &after.public_dependencies {
            if before_deps
                .get(dependency.module_id.as_str())
                .is_some_and(|digest| *digest != dependency.digest)
            {
                changes.push(DiffChange {
                    export: dependency.module_id.clone(),
                    member: None,
                    class: "wire-breaking".to_owned(),
                    reason: "public-dependency-changed".to_owned(),
                    before: before_deps.get(dependency.module_id.as_str()).copied().map(str::to_owned),
                    after: Some(dependency.digest.clone()),
                });
                breaking = true;
            }
        }
    }
    // Version policy evaluation.
    let before_version = before.api_version.as_deref().and_then(ApiVersion::parse);
    let after_version = after.api_version.as_deref().and_then(ApiVersion::parse);
    let required = RequiredBump::evaluate(
        before_version.as_ref(),
        after_version.as_ref(),
        breaking,
        additive,
    );
    let version_policy = if before.api_version.is_none() && after.api_version.is_none() {
        VersionDecision {
            decision: "digest-only".to_owned(),
            expected: None,
            actual: None,
            detail: if breaking { "digest-changed" } else { "digest-stable" }.to_owned(),
        }
    } else {
        match required {
            RequiredBump::None => VersionDecision {
                decision: "compliant".to_owned(),
                expected: None,
                actual: after.api_version.clone(),
                detail: "bump-satisfied".to_owned(),
            },
            needed => VersionDecision {
                decision: "violation".to_owned(),
                expected: Some(needed.as_str().to_owned()),
                actual: after.api_version.clone(),
                detail: "bump-missing".to_owned(),
            },
        }
    };
    if version_policy.decision == "violation" {
        breaking = true;
    }
    // Consumer impact over the union inventory.
    let (consumers, coverage) =
        consumer_impact(&before_map, &after_map, &changes, &before_snapshot, &after_snapshot);
    let verdict = if consumers
        .iter()
        .any(|consumer| consumer.status == "affected" && consumer.policy.is_some())
        && strict_denial(&consumers)
    {
        "policy-denied"
    } else if breaking {
        "breaking"
    } else {
        "compatible"
    };
    let report = DiffReport {
        schema_version: format!("{}v{}", wire::DIFF_SCHEMA, wire::VERSION),
        identity: format!("{}@{}", wire::DIFF_IDENTITY, wire::VERSION),
        before: DiffSide {
            module: module.clone(),
            project: before.producer.project.clone(),
            digest: before.digest.clone(),
            api_version: before.api_version.clone(),
        },
        after: DiffSide {
            module,
            project: after.producer.project.clone(),
            digest: after.digest.clone(),
            api_version: after.api_version.clone(),
        },
        changes,
        consumers,
        coverage,
        version_policy,
        verdict: verdict.to_owned(),
    };
    let mut evidence = Vec::new();
    if report.verdict == "breaking" {
        let mut finding = Evidence::new("module-api.version-policy-violation", &report.after.module, "bump-missing");
        finding.expected = report.version_policy.expected.clone();
        finding.actual = report.version_policy.actual.clone();
        evidence.push(finding);
    }
    for consumer in &report.consumers {
        if consumer.status == "affected" {
            let mut finding = Evidence::new(
                "module-api.consumer-incompatible",
                &report.after.module,
                "consumer-affected",
            );
            finding.consumer = Some(consumer.consumer.clone());
            finding.producer = Some(report.after.module.clone());
            finding.path = consumer
                .changes
                .first()
                .map(|change| vec![change.clone()])
                .unwrap_or_default();
            evidence.push(finding);
        }
    }
    Ok((report, evidence))
}

/// Whether strict policy denies: any affected consumer carrying an
/// incompatible declared policy.
fn strict_denial(consumers: &[ConsumerImpact]) -> bool {
    consumers
        .iter()
        .any(|consumer| consumer.status == "affected" && consumer.reason == Some("policy-incompatible".to_owned()))
}

/// The wire kind spelling of one export for evidence text.
fn export_kind(export: &super::wire::PublicExport) -> String {
    export.kind.clone()
}

/// The field list of one export by kind.
fn fields_of(export: &super::wire::PublicExport) -> &Vec<super::wire::PublicField> {
    match export.kind.as_str() {
        "command" => &export.input,
        "event" => &export.payload,
        _ => &export.fields,
    }
}

/// Field-level comparison of one export pair: exact classes and reasons.
#[allow(clippy::type_complexity)]
fn compare_exports(
    before: &super::wire::PublicExport,
    after: &super::wire::PublicExport,
) -> (Vec<(Option<String>, String, String, Option<String>, Option<String>)>, bool) {
    let mut changes = Vec::new();
    let mut breaking = false;
    if before.kind != after.kind {
        changes.push((None, "source-breaking".to_owned(), "kind-changed".to_owned(), Some(before.kind.clone()), Some(after.kind.clone())));
        return (changes, true);
    }
    let before_fields = fields_of(before);
    let after_fields = fields_of(after);
    let before_map: std::collections::BTreeMap<&str, &super::wire::PublicField> =
        before_fields.iter().map(|field| (field.name.as_str(), field)).collect();
    let after_map: std::collections::BTreeMap<&str, &super::wire::PublicField> =
        after_fields.iter().map(|field| (field.name.as_str(), field)).collect();
    for (name, removed) in &before_map {
        if !after_map.contains_key(name) {
            changes.push((Some((*name).to_owned()), "source-breaking".to_owned(), "field-removed".to_owned(), Some(field_text(removed)), None));
            breaking = true;
        }
    }
    for (name, added) in &after_map {
        if !before_map.contains_key(name) {
            // A required input addition breaks source consumers; an
            // optional output field addition is policy-dependent and can
            // never be universally assumed safe.
            if added.required && before.kind == "command" {
                changes.push((Some((*name).to_owned()), "source-breaking".to_owned(), "required-input-added".to_owned(), None, Some(field_text(added))));
                breaking = true;
            } else {
                changes.push((Some((*name).to_owned()), "additive".to_owned(), "field-added".to_owned(), None, Some(field_text(added))));
            }
        }
    }
    for (name, after_field) in &after_map {
        let Some(before_field) = before_map.get(name) else {
            continue;
        };
        // Nullable-to-required is a break; required-to-nullable is a
        // behavior change that widens output.
        if after_field.required && !before_field.required {
            changes.push((Some((*name).to_owned()), "source-breaking".to_owned(), "nullable-became-required".to_owned(), Some(field_text(before_field)), Some(field_text(after_field))));
            breaking = true;
        } else if before_field.required && !after_field.required {
            changes.push((Some((*name).to_owned()), "behavioral".to_owned(), "required-became-optional".to_owned(), Some(field_text(before_field)), Some(field_text(after_field))));
        }
        // Type identity change.
        if before_field.r#type != after_field.r#type {
            changes.push((Some((*name).to_owned()), "wire-breaking".to_owned(), "type-changed".to_owned(), Some(field_text(before_field)), Some(field_text(after_field))));
            breaking = true;
        }
    }
    // Enum value additions: additive but wire-visible.
    if !before.values.is_empty() || !after.values.is_empty() {
        for value in &after.values {
            if !before.values.contains(value) {
                changes.push((None, "additive".to_owned(), "enum-value-added".to_owned(), None, Some(value.clone())));
            }
        }
        for value in &before.values {
            if !after.values.contains(value) {
                changes.push((None, "wire-breaking".to_owned(), "enum-value-removed".to_owned(), Some(value.clone()), None));
                breaking = true;
            }
        }
    }
    // Query result type change.
    if before.returns != after.returns {
        changes.push((None, "wire-breaking".to_owned(), "result-type-changed".to_owned(), before.returns.as_ref().map(public_type_text), after.returns.as_ref().map(public_type_text)));
        breaking = true;
    }
    // Error export payload/code/retry obligations.
    if before.code != after.code {
        changes.push((None, "wire-breaking".to_owned(), "error-code-changed".to_owned(), before.code.clone(), after.code.clone()));
        breaking = true;
    }
    if before.retryable != after.retryable {
        changes.push((None, "behavioral".to_owned(), "retry-changed".to_owned(), before.retryable.map(|value| value.to_string()), after.retryable.map(|value| value.to_string())));
    }
    // Effect obligations.
    if before.effects != after.effects {
        changes.push((None, "behavioral".to_owned(), "effects-changed".to_owned(), Some(format!("{}", before.effects.len())), Some(format!("{}", after.effects.len()))));
    }
    (changes, breaking)
}

fn field_text(field: &super::wire::PublicField) -> String {
    format!(
        "{}{}:{}",
        field.name,
        if field.required { "!" } else { "?" },
        public_type_text(&field.r#type)
    )
}

fn public_type_text(expression: &super::wire::TypeExpr) -> String {
    match expression {
        super::wire::TypeExpr::Ref { r#ref } => r#ref.clone(),
        super::wire::TypeExpr::List { list } => format!("list<{}>", public_type_text(list)),
        super::wire::TypeExpr::Optional { optional } => {
            format!("optional<{}>", public_type_text(optional))
        }
    }
}

/// The union before/after consumer inventory partition.
#[allow(clippy::type_complexity)]
fn consumer_impact<'a>(
    before_map: &std::collections::BTreeMap<&'a str, &'a super::wire::PublicExport>,
    after_map: &std::collections::BTreeMap<&'a str, &'a super::wire::PublicExport>,
    changes: &[DiffChange],
    before_snapshot: &Option<ConsumerSnapshot>,
    after_snapshot: &Option<ConsumerSnapshot>,
) -> (Vec<ConsumerImpact>, DiffCoverage) {
    let impacted: std::collections::BTreeSet<&str> = changes
        .iter()
        .map(|change| change.export.as_str())
        .collect();
    let mut rows: std::collections::BTreeMap<String, (std::collections::BTreeSet<&str>, String, Option<&super::wire::VersionPolicyWire>)> =
        std::collections::BTreeMap::new();
    // presentIn tracking: before/after/both.
    let mut presence: std::collections::BTreeMap<String, std::collections::BTreeSet<&str>> =
        std::collections::BTreeMap::new();
    if let Some(snapshot) = before_snapshot {
        for consumer in &snapshot.consumers {
            presence
                .entry(consumer.consumer.clone())
                .or_default()
                .insert("before");
            let entry = rows
                .entry(consumer.consumer.clone())
                .or_insert_with(|| (std::collections::BTreeSet::new(), String::new(), consumer.policy.as_ref()));
            for use_row in &consumer.uses {
                if before_map.contains_key(use_row.export.as_str()) || after_map.contains_key(use_row.export.as_str()) {
                    entry.0.insert(use_row.export.as_str());
                }
            }
        }
    }
    if let Some(snapshot) = after_snapshot {
        for consumer in &snapshot.consumers {
            presence
                .entry(consumer.consumer.clone())
                .or_default()
                .insert("after");
            let entry = rows
                .entry(consumer.consumer.clone())
                .or_insert_with(|| (std::collections::BTreeSet::new(), String::new(), consumer.policy.as_ref()));
            if consumer.policy.is_some() {
                entry.2 = consumer.policy.as_ref();
            }
            for use_row in &consumer.uses {
                entry.0.insert(use_row.export.as_str());
            }
        }
    }
    let mut consumers = Vec::new();
    let mut known = 0u64;
    for (consumer, (uses, _mode, policy)) in &rows {
        let hits: Vec<&str> = uses.iter().copied().filter(|use_id| impacted.contains(use_id)).collect();
        let present_in = match (
            presence.get(consumer).is_some_and(|set| set.contains("before")),
            presence.get(consumer).is_some_and(|set| set.contains("after")),
        ) {
            (true, true) => "both",
            (true, false) => "before",
            (false, true) => "after",
            (false, false) => "both",
        };
        let status = if hits.is_empty() {
            "unaffected"
        } else if changes
            .iter()
            .any(|change| change.class != "additive" && hits.contains(&change.export.as_str()))
        {
            "affected"
        } else {
            "compatible"
        };
        if status != "unaffected" {
            known += 1;
        }
        let policy_kind = policy.map(|policy| policy_kind_text(policy));
        let reason = match status {
            "affected" => {
                if policy_kind.is_some() {
                    Some("policy-incompatible".to_owned())
                } else {
                    Some("unversioned-consumer".to_owned())
                }
            }
            "compatible" => Some("additive-only".to_owned()),
            _ => None,
        };
        consumers.push(ConsumerImpact {
            consumer: consumer.clone(),
            status: status.to_owned(),
            present_in: present_in.to_owned(),
            policy: policy_kind,
            changes: hits.iter().map(|hit| (*hit).to_owned()).collect(),
            witness: hits.iter().map(|hit| format!("{consumer}::{hit}")).collect(),
            reason,
        });
    }
    let mut unknown_reasons = Vec::new();
    if before_snapshot.is_none() || after_snapshot.is_none() {
        unknown_reasons.push("consumer-snapshots-missing".to_owned());
    }
    for snapshot in [before_snapshot, after_snapshot].into_iter().flatten() {
        if !snapshot.complete {
            unknown_reasons.push("external-callers-unknown".to_owned());
        }
    }
    unknown_reasons.sort();
    unknown_reasons.dedup();
    (
        consumers,
        DiffCoverage {
            known,
            possible: rows.keys().cloned().collect(),
            unknown_reasons,
        },
    )
}

fn policy_kind_text(policy: &super::wire::VersionPolicyWire) -> String {
    match policy {
        super::wire::VersionPolicyWire::ExactDigest { .. } => "exact-digest".to_owned(),
        super::wire::VersionPolicyWire::ExactVersion { .. } => "exact-version".to_owned(),
        super::wire::VersionPolicyWire::VersionRange { .. } => "version-range".to_owned(),
    }
}
