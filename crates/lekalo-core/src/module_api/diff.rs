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
    // Every pin is verified: a snapshot minted for another project or a
    // stale/different producer revision is invalid input, never a silent
    // participant in exact-consumer reporting (finding 12).
    for (snapshot, bundle) in [
        (before_snapshot.as_ref(), &before),
        (after_snapshot.as_ref(), &after),
    ]
    .into_iter()
    .filter_map(|(snapshot, bundle)| snapshot.map(|snapshot| (snapshot, bundle)))
    {
        let bound = snapshot.producer.module == bundle.producer.module
            && snapshot.producer.project == bundle.producer.project
            && snapshot.pins.project == bundle.producer.project
            && snapshot.pins.model_version == bundle.pins.model_version
            && snapshot.pins.ir_digest == bundle.pins.ir_digest
            && snapshot.pins.api_digest == bundle.digest;
        if !bound {
            return Err(super::diagnostic::invalid("snapshot-binding"));
        }
        // Selected-export membership (r4 review finding 3/codex): every
        // declared use of a bound snapshot names an exact export of the
        // bundle it is pinned to — a use of a symbol the producer never
        // exported is invalid inventory, never a phantom impact row.
        let exports: std::collections::BTreeSet<&str> = bundle
            .exports
            .iter()
            .map(|export| export.export.as_str())
            .collect();
        for consumer in &snapshot.consumers {
            for use_row in &consumer.uses {
                if !exports.contains(use_row.export.as_str()) {
                    return Err(super::diagnostic::invalid("snapshot-use-not-exported"));
                }
            }
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
    // r6 review finding (devin F2): the exports consumers construct in
    // input position, computed over BOTH sides so a shape that moved
    // between anchors is judged by the union of the two surfaces. A
    // required member added to a constructed shape is a source break;
    // the same addition on an output-only shape is wire-breaking — never
    // bare additive.
    let input_constructed = input_constructed_exports(&before_map, &after_map);
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
    // Changed exports: field/shape-level comparison. Member-level
    // additive surface and member-level breaks both propagate to the
    // version policy (r3 review findings 4/9).
    for (export, after_export) in &after_map {
        let Some(before_export) = before_map.get(export) else {
            continue;
        };
        let (export_changes, export_breaking, export_additive) = compare_exports(
            before_export,
            after_export,
            input_constructed.contains(*export),
        );
        breaking |= export_breaking;
        additive |= export_additive;
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
                    before: before_deps
                        .get(dependency.module_id.as_str())
                        .copied()
                        .map(str::to_owned),
                    after: Some(dependency.digest.clone()),
                });
                breaking = true;
            }
        }
    }
    // Bundle-level public policy facts: ports, capabilities, and
    // deprecation declarations are public semantics the diff must never
    // ignore (finding 6).
    //
    // Port obligations connect to their operations (r4 review finding
    // 7/codex): an operation that leaves the callable surface — present
    // on at least one before-port, absent from every after-port — is a
    // wire break of that operation itself, so consumers using it are
    // exactly affected, never silently unaffected.
    let mut required_capability_gap = false;
    {
        let before_callable = callable_surface(&before.ports);
        let after_callable = callable_surface(&after.ports);
        for operation in before_callable.difference(&after_callable) {
            changes.push(DiffChange {
                export: (*operation).to_owned(),
                member: None,
                class: "wire-breaking".to_owned(),
                reason: "port-operation-removed".to_owned(),
                before: Some("callable".to_owned()),
                after: None,
            });
            breaking = true;
        }
        let before_ports: std::collections::BTreeMap<&str, &super::wire::PortPolicy> = before
            .ports
            .iter()
            .map(|port| (port.port.as_str(), port))
            .collect();
        for port in &after.ports {
            match before_ports.get(port.port.as_str()) {
                None => {
                    changes.push(DiffChange {
                        export: port.port.clone(),
                        member: None,
                        class: "additive".to_owned(),
                        reason: "port-added".to_owned(),
                        before: None,
                        after: Some(port.operations.join(",")),
                    });
                    additive = true;
                }
                Some(before_port) => {
                    if before_port.operations != port.operations {
                        changes.push(DiffChange {
                            export: port.port.clone(),
                            member: None,
                            class: "behavioral".to_owned(),
                            reason: "port-operations-changed".to_owned(),
                            before: Some(before_port.operations.join(",")),
                            after: Some(port.operations.join(",")),
                        });
                    }
                }
            }
        }
        for port in &before.ports {
            if !after.ports.iter().any(|added| added.port == port.port) {
                changes.push(DiffChange {
                    export: port.port.clone(),
                    member: None,
                    class: "wire-breaking".to_owned(),
                    reason: "port-removed".to_owned(),
                    before: Some(port.operations.join(",")),
                    after: None,
                });
                breaking = true;
            }
        }
        let before_capabilities: std::collections::BTreeMap<&str, &super::wire::CapabilityPolicy> =
            before
                .capabilities
                .iter()
                .map(|capability| (capability.capability.as_str(), capability))
                .collect();
        for capability in &after.capabilities {
            match before_capabilities.get(capability.capability.as_str()) {
                None => {
                    // A newly required capability is a demand on every
                    // consumer of the module, and the mapping from a
                    // capability to the affected operations is not part
                    // of the public bundle (r4 review finding 7/codex):
                    // the change is behavioral — never purely additive —
                    // and the exact consumer impact stays an explicit
                    // evidence gap instead of an exact unaffected claim.
                    let required = capability.obligation == "required";
                    changes.push(DiffChange {
                        export: capability.capability.clone(),
                        member: None,
                        class: if required {
                            "behavioral".to_owned()
                        } else {
                            "additive".to_owned()
                        },
                        reason: if required {
                            "required-capability-added".to_owned()
                        } else {
                            "capability-added".to_owned()
                        },
                        before: None,
                        after: Some(capability.obligation.clone()),
                    });
                    if required {
                        required_capability_gap = true;
                    } else {
                        additive = true;
                    }
                }
                Some(before_capability) => {
                    if before_capability.obligation != capability.obligation {
                        changes.push(DiffChange {
                            export: capability.capability.clone(),
                            member: None,
                            class: "behavioral".to_owned(),
                            reason: "capability-obligation-changed".to_owned(),
                            before: Some(before_capability.obligation.clone()),
                            after: Some(capability.obligation.clone()),
                        });
                        if capability.obligation == "required" {
                            required_capability_gap = true;
                        }
                    }
                    // The capability version pin is public semantics (r3
                    // review finding 8/codex): a changed version is a
                    // consumer-visible compatibility fact, never digest-only.
                    if before_capability.version != capability.version {
                        changes.push(DiffChange {
                            export: capability.capability.clone(),
                            member: None,
                            class: "behavioral".to_owned(),
                            reason: "capability-version-changed".to_owned(),
                            before: before_capability.version.clone(),
                            after: capability.version.clone(),
                        });
                    }
                    if before_capability.owner != capability.owner {
                        changes.push(DiffChange {
                            export: capability.capability.clone(),
                            member: None,
                            class: "behavioral".to_owned(),
                            reason: "capability-owner-changed".to_owned(),
                            before: before_capability.owner.clone(),
                            after: capability.owner.clone(),
                        });
                    }
                }
            }
        }
        for capability in &before.capabilities {
            if !after
                .capabilities
                .iter()
                .any(|added| added.capability == capability.capability)
            {
                changes.push(DiffChange {
                    export: capability.capability.clone(),
                    member: None,
                    class: "wire-breaking".to_owned(),
                    reason: "capability-removed".to_owned(),
                    before: Some(capability.obligation.clone()),
                    after: None,
                });
                breaking = true;
            }
        }
        let before_deprecations: std::collections::BTreeMap<&str, &super::wire::DeprecationPolicy> =
            before
                .deprecations
                .iter()
                .map(|entry| (entry.export.as_str(), entry))
                .collect();
        for entry in &after.deprecations {
            match before_deprecations.get(entry.export.as_str()) {
                None => {
                    // A deprecation declaration that appears only on the
                    // after side is new public semantics (r3 review finding
                    // 5): the diff must never ignore it.
                    changes.push(DiffChange {
                        export: entry.export.clone(),
                        member: None,
                        class: "additive".to_owned(),
                        reason: "deprecation-added".to_owned(),
                        before: None,
                        after: entry.replacement.clone(),
                    });
                    additive = true;
                }
                Some(before_entry) => {
                    if before_entry.replacement != entry.replacement {
                        changes.push(DiffChange {
                            export: entry.export.clone(),
                            member: None,
                            class: "behavioral".to_owned(),
                            reason: "deprecation-replacement-changed".to_owned(),
                            before: before_entry.replacement.clone(),
                            after: entry.replacement.clone(),
                        });
                    }
                    if before_entry.introduced_in != entry.introduced_in
                        || before_entry.removed_in != entry.removed_in
                    {
                        changes.push(DiffChange {
                            export: entry.export.clone(),
                            member: None,
                            class: "behavioral".to_owned(),
                            reason: "deprecation-window-changed".to_owned(),
                            before: Some(format!(
                                "{}..{}",
                                before_entry.introduced_in,
                                before_entry.removed_in.as_deref().unwrap_or("open")
                            )),
                            after: Some(format!(
                                "{}..{}",
                                entry.introduced_in,
                                entry.removed_in.as_deref().unwrap_or("open")
                            )),
                        });
                    }
                    if before_entry.hint != entry.hint {
                        changes.push(DiffChange {
                            export: entry.export.clone(),
                            member: None,
                            class: "behavioral".to_owned(),
                            reason: "deprecation-hint-changed".to_owned(),
                            before: Some(before_entry.hint.clone()),
                            after: Some(entry.hint.clone()),
                        });
                    }
                }
            }
        }
        // A deprecation retracted between the two sides is a behavioral
        // change of the public migration policy (r3 review finding 5).
        for entry in &before.deprecations {
            if !after
                .deprecations
                .iter()
                .any(|kept| kept.export == entry.export)
            {
                changes.push(DiffChange {
                    export: entry.export.clone(),
                    member: None,
                    class: "behavioral".to_owned(),
                    reason: "deprecation-removed".to_owned(),
                    before: entry.replacement.clone(),
                    after: None,
                });
            }
        }
    }
    // Version policy evaluation: identical bundles demand no bump.
    let before_version = before.api_version.as_deref().and_then(ApiVersion::parse);
    let after_version = after.api_version.as_deref().and_then(ApiVersion::parse);
    // Deprecation windows: a removal before the declared window, a
    // removal due that never happened, and a broken replacement obligation
    // are policy violations with typed evidence (finding 18: the parsed
    // deprecation surface is enforced at the diff seam).
    let deprecation_evidence = deprecation_findings(&before, &after, after_version.as_ref());
    let deprecation_denial = !deprecation_evidence.is_empty();
    let required = RequiredBump::evaluate(
        before_version.as_ref(),
        after_version.as_ref(),
        !changes.is_empty(),
        breaking,
        additive,
    );
    let version_policy = if before.api_version.is_none() && after.api_version.is_none() {
        VersionDecision {
            decision: "digest-only".to_owned(),
            expected: None,
            actual: None,
            // The digest-only detail reports the actual digest relation
            // (r3 review finding 4): additive and behavioral changes move
            // the digest too, so `digest-stable` never mislabels them.
            detail: if before.digest != after.digest {
                "digest-changed"
            } else {
                "digest-stable"
            }
            .to_owned(),
        }
    } else {
        match required {
            RequiredBump::None => VersionDecision {
                decision: "compliant".to_owned(),
                expected: None,
                actual: after.api_version.clone(),
                detail: "bump-satisfied".to_owned(),
            },
            // A version that moved backwards is a violation independent
            // of the change classification (r4 review finding 8/codex).
            RequiredBump::Backwards => VersionDecision {
                decision: "violation".to_owned(),
                expected: before_version
                    .as_ref()
                    .map(|version| format!("> {version}")),
                actual: after.api_version.clone(),
                detail: "version-moved-backwards".to_owned(),
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
    // Consumer impact over the union inventory; dependency changes
    // propagate through the used-export transitive closure.
    let (consumers, mut coverage) = consumer_impact(
        &before_map,
        &after_map,
        &changes,
        &before_snapshot,
        &after_snapshot,
        &after,
    );
    // A new or newly-required capability has no operation mapping in the
    // public bundle, so its exact consumer impact is explicitly unknown —
    // never an exact unaffected claim (r4 review finding 7/codex).
    if required_capability_gap {
        coverage
            .unknown_reasons
            .push("required-capability-impact-unknown".to_owned());
        coverage.unknown_reasons.sort();
        coverage.unknown_reasons.dedup();
    }
    let policy_denied = strict_denial(&consumers) || deprecation_denial;
    let verdict = if policy_denied {
        "policy-denied"
    } else if breaking {
        "breaking"
    } else {
        "compatible"
    };
    let report = DiffReport {
        schema_version: format!("{}/v{}", wire::DIFF_SCHEMA, wire::VERSION),
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
    let mut evidence = deprecation_evidence;
    if report.version_policy.decision == "violation" {
        let detail = report.version_policy.detail.clone();
        let mut finding = Evidence::new(
            "module-api.version-policy-violation",
            &report.after.module,
            detail.clone(),
        );
        finding.expected = report.version_policy.expected.clone();
        finding.actual = report.version_policy.actual.clone();
        finding.path = vec![detail];
        evidence.push(finding);
    }
    for consumer in &report.consumers {
        if consumer.status == "affected" && consumer.reason.as_deref() != Some("policy-accepted") {
            let mut finding = Evidence::new(
                "module-api.consumer-incompatible",
                &report.after.module,
                "consumer-affected",
            );
            finding.consumer = Some(consumer.consumer.clone());
            finding.producer = Some(report.after.module.clone());
            finding.path = vec![consumer
                .policy
                .clone()
                .unwrap_or_else(|| "unversioned".to_owned())];
            evidence.push(finding);
        }
    }
    // Unknown consumer coverage is explicit evidence (r3 review finding
    // 7/codex): a strict gate can never silently approve a diff whose
    // consumer inventory is missing. The permanently unknown external
    // caller surface stays a coverage note; the missing-snapshot gap is
    // the required-evidence denial seam. A required-capability change
    // without an operation mapping is the same class of explicit
    // unknown (r4 review finding 7/codex).
    for reason in [
        "consumer-snapshots-missing",
        "required-capability-impact-unknown",
    ] {
        if report
            .coverage
            .unknown_reasons
            .iter()
            .any(|kept| kept == reason)
        {
            let mut gap = Evidence::new(
                "module-api.evidence-incomplete",
                &report.after.module,
                "consumer-coverage-unknown",
            );
            gap.gaps = vec![reason.to_owned()];
            evidence.push(gap);
        }
    }
    Ok((report, evidence))
}

/// Deprecation-window evidence across the two bundles: a removed export
/// that left its declared window early, a still-present export whose
/// window has closed at the candidate version, and a replacement
/// obligation pointing at a non-public target.
fn deprecation_findings(
    before: &super::wire::PublicBundle,
    after: &super::wire::PublicBundle,
    after_version: Option<&ApiVersion>,
) -> Vec<Evidence> {
    let mut findings = Vec::new();
    let version_of = |text: &str| ApiVersion::parse(text);
    // The after side: still-present exports whose removal window closed.
    for entry in &after.deprecations {
        let Some(removed_in) = entry.removed_in.as_deref().and_then(version_of) else {
            continue;
        };
        let Some(current) = after_version else {
            continue;
        };
        let still_present = after
            .exports
            .iter()
            .any(|export| export.export == entry.export);
        if still_present && *current >= removed_in {
            let mut finding = Evidence::new(
                "module-api.deprecation-window-violation",
                &after.producer.module,
                "removal-window-closed",
            );
            finding.export = Some(entry.export.clone());
            finding.expected = Some(format!("removed-by-{removed_in}"));
            finding.actual = after.api_version.clone();
            findings.push(finding);
        }
    }
    // Removals before the window and broken replacement obligations.
    for entry in &before.deprecations {
        if after
            .exports
            .iter()
            .any(|export| export.export == entry.export)
        {
            continue;
        }
        if let (Some(removed_in), Some(current)) = (
            entry.removed_in.as_deref().and_then(version_of),
            after_version,
        ) {
            if *current < removed_in {
                let mut finding = Evidence::new(
                    "module-api.deprecation-window-violation",
                    &after.producer.module,
                    "removal-before-window",
                );
                finding.export = Some(entry.export.clone());
                finding.expected = Some(format!("not-before-{removed_in}"));
                finding.actual = after.api_version.clone();
                findings.push(finding);
            }
        }
        if let Some(replacement) = &entry.replacement {
            if !after
                .exports
                .iter()
                .any(|export| &export.export == replacement)
            {
                let mut finding = Evidence::new(
                    "module-api.deprecation-window-violation",
                    &after.producer.module,
                    "replacement-obligation",
                );
                finding.export = Some(entry.export.clone());
                finding.expected = Some(replacement.clone());
                finding.actual = Some("absent".to_owned());
                findings.push(finding);
            }
        }
    }
    findings
}

/// Whether any consumer row denies under strict policy: an affected
/// consumer whose declared policy rejects the candidate resolution.
fn strict_denial(consumers: &[ConsumerImpact]) -> bool {
    consumers.iter().any(|consumer| {
        consumer.status == "affected" && consumer.reason == Some("policy-incompatible".to_owned())
    })
}
/// The wire kind spelling of one export for evidence text.
fn export_kind(export: &super::wire::PublicExport) -> String {
    export.kind.clone()
}

/// The callable surface of one side: every operation present on at
/// least one port (r4 review finding 7/codex).
fn callable_surface(ports: &[super::wire::PortPolicy]) -> std::collections::BTreeSet<String> {
    ports
        .iter()
        .flat_map(|port| port.operations.iter().cloned())
        .collect()
}

/// The field list of one export by kind (error payloads are error-kind
/// fields: r3 review finding 8/codex — the public payload type of an
/// error is part of the compared surface).
fn fields_of(export: &super::wire::PublicExport) -> &Vec<super::wire::PublicField> {
    match export.kind.as_str() {
        "command" => &export.input,
        "event" | "error" => &export.payload,
        _ => &export.fields,
    }
}

/// Field-level comparison of one export pair: exact classes and reasons.
/// The return carries the member-level break and additive bits so the
/// version policy sees the complete final change set (r3 review findings
/// 4 and 9/codex). `consumer_constructed` says whether the compared
/// shape is constructed by consumers in input position (r6 review
/// finding devin F2).
#[allow(clippy::type_complexity)]
fn compare_exports(
    before: &super::wire::PublicExport,
    after: &super::wire::PublicExport,
    consumer_constructed: bool,
) -> (
    Vec<(
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
    )>,
    bool,
    bool,
) {
    let mut changes = Vec::new();
    let mut breaking = false;
    let mut additive = false;
    if before.kind != after.kind {
        changes.push((
            None,
            "source-breaking".to_owned(),
            "kind-changed".to_owned(),
            Some(before.kind.clone()),
            Some(after.kind.clone()),
        ));
        return (changes, true, false);
    }
    let before_fields = fields_of(before);
    let after_fields = fields_of(after);
    let before_map: std::collections::BTreeMap<&str, &super::wire::PublicField> = before_fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect();
    let after_map: std::collections::BTreeMap<&str, &super::wire::PublicField> = after_fields
        .iter()
        .map(|field| (field.name.as_str(), field))
        .collect();
    for (name, removed) in &before_map {
        if !after_map.contains_key(name) {
            changes.push((
                Some((*name).to_owned()),
                "source-breaking".to_owned(),
                "field-removed".to_owned(),
                Some(field_text(removed)),
                None,
            ));
            breaking = true;
        }
    }
    for (name, added) in &after_map {
        if !before_map.contains_key(name) {
            // A required input addition breaks source consumers; an
            // optional output field addition is policy-dependent and can
            // never be universally assumed safe. r6 review finding (devin
            // F2): the same holds for non-command kinds — a required
            // member on a shape consumers construct is a source break
            // (they must supply it), and on an output-only shape it is
            // wire-visible and policy-dependent, never bare additive.
            if added.required && (before.kind == "command" || consumer_constructed) {
                changes.push((
                    Some((*name).to_owned()),
                    "source-breaking".to_owned(),
                    "required-input-added".to_owned(),
                    None,
                    Some(field_text(added)),
                ));
                breaking = true;
            } else if added.required {
                changes.push((
                    Some((*name).to_owned()),
                    "wire-breaking".to_owned(),
                    "required-field-added".to_owned(),
                    None,
                    Some(field_text(added)),
                ));
                breaking = true;
            } else {
                changes.push((
                    Some((*name).to_owned()),
                    "additive".to_owned(),
                    "field-added".to_owned(),
                    None,
                    Some(field_text(added)),
                ));
                additive = true;
            }
        }
    }
    for (name, after_field) in &after_map {
        let Some(before_field) = before_map.get(name) else {
            continue;
        };
        // Nullable-to-required is a break; required-to-nullable carries
        // input/output variance (r3 review finding 9/codex): a command
        // input that becomes optional widens the accepted surface, but a
        // consumer that relied on the presence of an output member is no
        // longer safe, so output optionality breaks.
        if after_field.required && !before_field.required {
            changes.push((
                Some((*name).to_owned()),
                "source-breaking".to_owned(),
                "nullable-became-required".to_owned(),
                Some(field_text(before_field)),
                Some(field_text(after_field)),
            ));
            breaking = true;
        } else if before_field.required && !after_field.required {
            if before.kind == "command" {
                changes.push((
                    Some((*name).to_owned()),
                    "behavioral".to_owned(),
                    "required-became-optional".to_owned(),
                    Some(field_text(before_field)),
                    Some(field_text(after_field)),
                ));
            } else {
                changes.push((
                    Some((*name).to_owned()),
                    "wire-breaking".to_owned(),
                    "required-output-became-optional".to_owned(),
                    Some(field_text(before_field)),
                    Some(field_text(after_field)),
                ));
                breaking = true;
            }
        }
        // Type identity change.
        if before_field.r#type != after_field.r#type {
            changes.push((
                Some((*name).to_owned()),
                "wire-breaking".to_owned(),
                "type-changed".to_owned(),
                Some(field_text(before_field)),
                Some(field_text(after_field)),
            ));
            breaking = true;
        }
    }
    // Enum value additions: additive but wire-visible.
    if !before.values.is_empty() || !after.values.is_empty() {
        for value in &after.values {
            if !before.values.contains(value) {
                changes.push((
                    None,
                    "additive".to_owned(),
                    "enum-value-added".to_owned(),
                    None,
                    Some(value.clone()),
                ));
                additive = true;
            }
        }
        for value in &before.values {
            if !after.values.contains(value) {
                changes.push((
                    None,
                    "wire-breaking".to_owned(),
                    "enum-value-removed".to_owned(),
                    Some(value.clone()),
                    None,
                ));
                breaking = true;
            }
        }
    }
    // Query result type change.
    if before.returns != after.returns {
        changes.push((
            None,
            "wire-breaking".to_owned(),
            "result-type-changed".to_owned(),
            before.returns.as_ref().map(public_type_text),
            after.returns.as_ref().map(public_type_text),
        ));
        breaking = true;
    }
    // Error export payload/code/retry obligations.
    if before.code != after.code {
        changes.push((
            None,
            "wire-breaking".to_owned(),
            "error-code-changed".to_owned(),
            before.code.clone(),
            after.code.clone(),
        ));
        breaking = true;
    }
    if before.retryable != after.retryable {
        changes.push((
            None,
            "behavioral".to_owned(),
            "retry-changed".to_owned(),
            before.retryable.map(|value| value.to_string()),
            after.retryable.map(|value| value.to_string()),
        ));
    }
    // Scalar base: the public primitive domain of the type.
    if before.base != after.base {
        changes.push((
            None,
            "wire-breaking".to_owned(),
            "base-changed".to_owned(),
            before.base.clone(),
            after.base.clone(),
        ));
        breaking = true;
    }
    // Entity identity members are public semantics.
    if before.identity != after.identity {
        changes.push((
            None,
            "wire-breaking".to_owned(),
            "identity-changed".to_owned(),
            Some(before.identity.join(",")),
            Some(after.identity.join(",")),
        ));
        breaking = true;
    }
    // Query read sets are pinned public obligations.
    if before.reads != after.reads {
        changes.push((
            None,
            "behavioral".to_owned(),
            "reads-changed".to_owned(),
            Some(before.reads.join(",")),
            Some(after.reads.join(",")),
        ));
    }
    // Endpoint wire binding: method and path are the route contract.
    if before.method != after.method || before.path != after.path {
        changes.push((
            None,
            "wire-breaking".to_owned(),
            "route-changed".to_owned(),
            Some(format!(
                "{} {}",
                before.method.as_deref().unwrap_or_default(),
                before.path.as_deref().unwrap_or_default()
            )),
            Some(format!(
                "{} {}",
                after.method.as_deref().unwrap_or_default(),
                after.path.as_deref().unwrap_or_default()
            )),
        ));
        breaking = true;
    }
    if before.invokes != after.invokes {
        changes.push((
            None,
            "wire-breaking".to_owned(),
            "operation-changed".to_owned(),
            before.invokes.clone(),
            after.invokes.clone(),
        ));
        breaking = true;
    }
    // Error unions of exported operations (r3 review finding 14/codex):
    // an added member breaks exhaustive consumers; a removed member is a
    // wire-visible union change.
    if before.errors != after.errors {
        let added: Vec<&String> = after
            .errors
            .iter()
            .filter(|error| !before.errors.contains(error))
            .collect();
        let removed: Vec<&String> = before
            .errors
            .iter()
            .filter(|error| !after.errors.contains(error))
            .collect();
        for error in added {
            changes.push((
                None,
                "wire-breaking".to_owned(),
                "error-union-added".to_owned(),
                None,
                Some(error.clone()),
            ));
            breaking = true;
        }
        for error in removed {
            changes.push((
                None,
                "wire-breaking".to_owned(),
                "error-union-removed".to_owned(),
                Some(error.clone()),
                None,
            ));
            breaking = true;
        }
    }
    // Public query owner facts: cardinality, consistency, and pagination
    // are consumer-visible query semantics.
    for (member, before_fact, after_fact) in [
        (
            "cardinality",
            before.cardinality.clone(),
            after.cardinality.clone(),
        ),
        (
            "consistency",
            before.consistency.clone(),
            after.consistency.clone(),
        ),
        (
            "pagination",
            before.pagination.clone(),
            after.pagination.clone(),
        ),
    ] {
        if before_fact != after_fact {
            changes.push((
                None,
                "behavioral".to_owned(),
                format!("{member}-changed"),
                before_fact,
                after_fact,
            ));
        }
    }
    // Accepted alias evidence is preserved public semantics (r3 review
    // finding 10/codex): a changed rename binding is visible to the diff.
    if before.alias_of != after.alias_of {
        changes.push((
            None,
            "behavioral".to_owned(),
            "alias-changed".to_owned(),
            before.alias_of.clone(),
            after.alias_of.clone(),
        ));
    }
    if before.since != after.since {
        changes.push((
            None,
            "behavioral".to_owned(),
            "since-changed".to_owned(),
            before.since.clone(),
            after.since.clone(),
        ));
    }
    // Policy decision and governed operations.
    if before.decision != after.decision {
        changes.push((
            None,
            "behavioral".to_owned(),
            "decision-changed".to_owned(),
            before.decision.clone(),
            after.decision.clone(),
        ));
    }
    if before.operations != after.operations {
        changes.push((
            None,
            "behavioral".to_owned(),
            "operations-changed".to_owned(),
            Some(before.operations.join(",")),
            Some(after.operations.join(",")),
        ));
    }
    // Effect obligations, keyed by (entity, operation): a removed
    // obligation is a wire break; additions and emit changes are
    // behavioral facts consumers must observe.
    {
        let key = |obligation: &super::wire::EffectObligation| {
            (obligation.entity.clone(), obligation.operation.clone())
        };
        let before_effects: std::collections::BTreeMap<_, _> =
            before.effects.iter().map(|o| (key(o), o)).collect();
        let after_effects: std::collections::BTreeMap<_, _> =
            after.effects.iter().map(|o| (key(o), o)).collect();
        for (key, obligation) in &before_effects {
            if !after_effects.contains_key(key) {
                changes.push((
                    None,
                    "wire-breaking".to_owned(),
                    "effect-removed".to_owned(),
                    Some(format!("{}|{}", key.0, key.1)),
                    None,
                ));
                breaking = true;
            } else if after_effects[key].emits != obligation.emits {
                changes.push((
                    None,
                    "behavioral".to_owned(),
                    "effect-emits-changed".to_owned(),
                    Some(obligation.emits.join(",")),
                    Some(after_effects[key].emits.join(",")),
                ));
            }
        }
        for key in after_effects.keys() {
            if !before_effects.contains_key(key) {
                changes.push((
                    None,
                    "behavioral".to_owned(),
                    "effect-added".to_owned(),
                    None,
                    Some(format!("{}|{}", key.0, key.1)),
                ));
            }
        }
    }
    (changes, breaking, additive)
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

/// The union before/after consumer inventory partition. A public
/// dependency change marks every consumer whose used exports transitively
/// reach the changed dependency (finding 19); each declared policy is
/// evaluated against the candidate resolution (finding 5).
#[allow(clippy::type_complexity)]
fn consumer_impact<'a>(
    before_map: &std::collections::BTreeMap<&'a str, &'a super::wire::PublicExport>,
    after_map: &std::collections::BTreeMap<&'a str, &'a super::wire::PublicExport>,
    changes: &[DiffChange],
    before_snapshot: &Option<ConsumerSnapshot>,
    after_snapshot: &Option<ConsumerSnapshot>,
    after: &super::wire::PublicBundle,
) -> (Vec<ConsumerImpact>, DiffCoverage) {
    // The hard-impact export set (r3 review findings 5 and 6/codex): every
    // export with a non-additive change, every export whose public
    // signature closure — computed over BOTH versions, so before-only
    // removal paths survive — transitively reaches a changed or removed
    // export, and every export reaching a symbol pinned by a changed
    // foreign public dependency.
    let mut hard: std::collections::BTreeSet<&str> = changes
        .iter()
        .filter(|change| change.class != "additive")
        .map(|change| change.export.as_str())
        .collect();
    // The propagation seeds: changed/removed exports plus the symbols of
    // changed foreign dependencies.
    let mut seeds: std::collections::BTreeSet<&str> = hard.clone();
    let changed_deps: std::collections::BTreeSet<&str> = changes
        .iter()
        .filter(|change| change.reason == "public-dependency-changed")
        .map(|change| change.export.as_str())
        .collect();
    for dependency in &after.public_dependencies {
        if changed_deps.contains(dependency.module_id.as_str()) {
            seeds.extend(dependency.exports.iter().map(String::as_str));
        }
    }
    if !seeds.is_empty() {
        let before_closure = reference_closures(before_map);
        let after_closure = reference_closures(after_map);
        for export_id in before_map.keys().copied().chain(after_map.keys().copied()) {
            let touches = after_closure.get(export_id).is_some_and(|closure| {
                closure.iter().any(|symbol| seeds.contains(symbol.as_str()))
            }) || before_closure.get(export_id).is_some_and(|closure| {
                closure.iter().any(|symbol| seeds.contains(symbol.as_str()))
            });
            if touches {
                hard.insert(export_id);
            }
        }
    }
    let impacted: std::collections::BTreeSet<&str> = changes
        .iter()
        .map(|change| change.export.as_str())
        .chain(hard.iter().copied())
        .collect();
    let mut rows: std::collections::BTreeMap<
        String,
        (
            std::collections::BTreeSet<&str>,
            String,
            Option<&super::wire::VersionPolicyWire>,
        ),
    > = std::collections::BTreeMap::new();
    // presentIn tracking: before/after/both.
    let mut presence: std::collections::BTreeMap<String, std::collections::BTreeSet<&str>> =
        std::collections::BTreeMap::new();
    if let Some(snapshot) = before_snapshot {
        for consumer in &snapshot.consumers {
            presence
                .entry(consumer.consumer.clone())
                .or_default()
                .insert("before");
            let entry = rows.entry(consumer.consumer.clone()).or_insert_with(|| {
                (
                    std::collections::BTreeSet::new(),
                    String::new(),
                    consumer.policy.as_ref(),
                )
            });
            for use_row in &consumer.uses {
                if before_map.contains_key(use_row.export.as_str())
                    || after_map.contains_key(use_row.export.as_str())
                {
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
            let entry = rows.entry(consumer.consumer.clone()).or_insert_with(|| {
                (
                    std::collections::BTreeSet::new(),
                    String::new(),
                    consumer.policy.as_ref(),
                )
            });
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
        let hits: Vec<&str> = uses
            .iter()
            .copied()
            .filter(|use_id| impacted.contains(use_id))
            .collect();
        let present_in = match (
            presence
                .get(consumer)
                .is_some_and(|set| set.contains("before")),
            presence
                .get(consumer)
                .is_some_and(|set| set.contains("after")),
        ) {
            (true, true) => "both",
            (true, false) => "before",
            (false, true) => "after",
            (false, false) => "both",
        };
        let mut status = if hits.is_empty() {
            "unaffected"
        } else if hits.iter().any(|hit| hard.contains(hit)) {
            "affected"
        } else {
            "compatible"
        };
        let policy_kind = policy.map(policy_kind_text);
        // The declared policy is evaluated against the candidate
        // resolution (findings 5, r3 findings 6/codex): policy evaluation
        // is an independent obligation — a rejected resolution marks the
        // consumer affected and denies under strict even when no used
        // shape changed at all (the pinned digest can never resolve
        // again). Hits only refine the reason, never the verdict.
        let policy_verdict = policy
            .and_then(super::version::ConsumerPolicy::parse)
            .map(|parsed| {
                if parsed.accepts(&after.digest, after.api_version.as_deref()) {
                    "policy-accepted"
                } else {
                    "policy-incompatible"
                }
            });
        if policy_verdict == Some("policy-incompatible") {
            status = "affected";
        }
        if status != "unaffected" {
            known += 1;
        }
        let reason = match status {
            "affected" => match policy_verdict {
                Some("policy-incompatible") => Some("policy-incompatible".to_owned()),
                Some(_) => Some("policy-accepted".to_owned()),
                None => Some("unversioned-consumer".to_owned()),
            },
            "compatible" => match policy_verdict {
                Some("policy-incompatible") => Some("policy-incompatible".to_owned()),
                _ => Some("additive-only".to_owned()),
            },
            _ => None,
        };
        consumers.push(ConsumerImpact {
            consumer: consumer.clone(),
            status: status.to_owned(),
            present_in: present_in.to_owned(),
            policy: policy_kind,
            changes: hits.iter().map(|hit| (*hit).to_owned()).collect(),
            witness: hits
                .iter()
                .map(|hit| format!("{consumer}::{hit}"))
                .collect(),
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

/// The exports consumers construct in input position (r6 review finding
/// devin F2): command inputs are the positions a consumer builds and
/// sends (the closed grammar admits no query parameters), and every
/// shape transitively referenced by a constructed shape's compared
/// field set is constructed with it. The walk runs over BOTH sides of
/// the comparison so an export that changed anchors is judged by the
/// union surface. Commands terminate the walk: their input closure is
/// seeded as roots, and a required input addition on a command is
/// already classified by the kind check.
fn input_constructed_exports(
    before_map: &std::collections::BTreeMap<&str, &super::wire::PublicExport>,
    after_map: &std::collections::BTreeMap<&str, &super::wire::PublicExport>,
) -> std::collections::BTreeSet<String> {
    let mut constructed: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut stack: Vec<String> = Vec::new();
    for map in [before_map, after_map] {
        for definition in map.values() {
            if definition.kind == "command" {
                stack.extend(
                    definition
                        .input
                        .iter()
                        .map(|field| field.r#type.leaf().to_owned()),
                );
            }
        }
    }
    while let Some(node) = stack.pop() {
        if !constructed.insert(node.clone()) {
            continue;
        }
        for map in [before_map, after_map] {
            if let Some(definition) = map.get(node.as_str()) {
                if definition.kind != "command" {
                    stack.extend(
                        fields_of(definition)
                            .iter()
                            .map(|field| field.r#type.leaf().to_owned()),
                    );
                }
            }
        }
    }
    constructed
}

/// The memoized transitive local reference closure per export of one
/// side, including the foreign leaves that terminate it (r3 review
/// finding 5/codex: local shape changes propagate exactly like foreign
/// dependency changes).
fn reference_closures<'a>(
    side: &std::collections::BTreeMap<&'a str, &'a super::wire::PublicExport>,
) -> std::collections::BTreeMap<&'a str, std::collections::BTreeSet<String>> {
    let mut memo: std::collections::BTreeMap<&'a str, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for root in side.keys() {
        let mut visited: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut collected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut stack: Vec<String> = vec![(*root).to_owned()];
        while let Some(node) = stack.pop() {
            if !visited.insert(node.clone()) {
                continue;
            }
            let Some(node_export) = side.get(node.as_str()) else {
                // A foreign leaf: terminal, but still a hit candidate.
                collected.insert(node);
                continue;
            };
            for reference in super::context::export_references(node_export) {
                collected.insert(reference.clone());
                if side.contains_key(reference.as_str()) {
                    stack.push(reference);
                }
            }
        }
        memo.insert(root, collected);
    }
    memo
}
