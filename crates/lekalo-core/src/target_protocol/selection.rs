//! Deterministic adapter/profile selection with explainable verdicts
//! (issue #28).
//!
//! [`select`] consumes discovered adapters plus an explicit policy and
//! produces one closed, serializable [`SelectionReport`]: the selected
//! adapter and profile, per-capability provenance and warnings, and every
//! excluded adapter with its sorted stable reason tokens. The report is a
//! plain value — the caller projects it into any envelope — and two runs
//! over the same inputs always produce byte-identical reports.
//!
//! The fixed filter order per candidate is: IR compatibility, then
//! required-capability support under the policy. `unknown` never counts
//! as an available capability: in the strict default policy it excludes,
//! and only the explicit non-strict policy may proceed past it, with a
//! recorded warning. `partial` proceeds only with the explicit
//! `allow_partial` policy, likewise recorded. Survivors are ordered by
//! adapter id ascending, version descending, selected profile, then every
//! remaining projected member, so distinct identities resolve
//! deterministically and only identical projections may tie — and the
//! first survivor is selected — every other survivor is reported as
//! `ordering`, never silently dropped.

use serde::Serialize;

use super::discovery::{DiscoveredAdapter, Provenance};
use super::wire;

/// The explicit selection policy. Both fields default to the strict,
/// fail-closed reading of the issue: partial support blocks without the
/// accepted policy, and unknown is never an optimistic yes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct SelectionPolicy {
    /// Proceed past `partial` support, recording a warning.
    pub allow_partial: bool,
    /// Proceed past `unknown` support, recording a warning. The strict
    /// workflow (default) leaves this off.
    pub tolerate_unknown: bool,
}

/// One selection request over discovered candidates.
#[derive(Clone, Copy, Debug)]
pub struct SelectionRequest<'a> {
    /// The capability ids the result must support, sorted by the caller
    /// or deduplicated here.
    pub required: &'a [String],
    /// The preferred profile token, when any.
    pub preferred_profile: Option<&'a str>,
    /// The explicit policy.
    pub policy: SelectionPolicy,
}

/// The sorted stable exclusion reasons.
pub mod reasons {
    /// The adapter did not declare the required IR contract version.
    pub const IR_UNDECLARED: &str = "ir-undeclared";
    /// A required capability is declared `unsupported` (or absent).
    pub const CAPABILITY_UNSUPPORTED: &str = "capability-unsupported";
    /// A required capability is declared `unknown` under strict policy.
    pub const CAPABILITY_UNKNOWN: &str = "capability-unknown";
    /// A required capability is only `partial` without the accepted
    /// policy.
    pub const PARTIAL_POLICY: &str = "partial-policy";
    /// A compatible survivor that deterministic ordering did not select.
    pub const ORDERING: &str = "ordering";
}

/// The warnings recorded on a selection that proceeded past a policy
/// gate.
pub mod warnings {
    /// Partial support was accepted under the explicit policy.
    pub const PARTIAL_ACCEPTED: &str = "partial-accepted";
    /// Unknown support was tolerated under the explicit non-strict
    /// policy.
    pub const UNKNOWN_TOLERATED: &str = "unknown-tolerated";
}

/// One excluded (or not selected) adapter with its reasons.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExcludedAdapter {
    /// The declared adapter id.
    pub adapter: String,
    /// The declared adapter version.
    pub version: String,
    /// The sorted stable reason tokens.
    pub reasons: Vec<&'static str>,
}

/// One selected capability as the report projects it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SelectedCapability {
    pub id: String,
    pub state: wire::SupportState,
    /// How the support state was established.
    pub provenance: Provenance,
    /// The definition version of the id's semantics.
    pub definition_version: &'static str,
}

/// The selected adapter, profile, and capability view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SelectionChoice {
    /// The declared adapter identity.
    pub adapter: wire::AdapterIdentity,
    /// The negotiated protocol version of the live session.
    pub negotiated_version: &'static str,
    /// The digest over the canonical declared capability bytes.
    pub capability_digest: String,
    /// The verified executable digest, when one was computed.
    pub executable_digest: Option<String>,
    /// The deterministically selected profile, when the adapter declared
    /// one.
    pub profile: Option<String>,
    /// The IR contract versions the adapter declared.
    pub ir_versions: Vec<String>,
    /// Every discovered capability, sorted by id.
    pub capabilities: Vec<SelectedCapability>,
    /// The sorted warnings recorded under the explicit policy.
    pub warnings: Vec<&'static str>,
}

/// The closed selection report: explainable and machine-readable.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SelectionReport {
    /// The policy the selection ran under.
    pub policy: SelectionPolicy,
    /// The required capability ids, in canonical order.
    pub required: Vec<String>,
    /// The selection, or `None` when every candidate was excluded.
    pub selected: Option<SelectionChoice>,
    /// Every excluded candidate, sorted by (adapter, version, reasons).
    pub excluded: Vec<ExcludedAdapter>,
}

impl SelectionReport {
    /// Whether an adapter was selected.
    pub fn is_selected(&self) -> bool {
        self.selected.is_some()
    }
}

/// Run the deterministic selection over the discovered candidates.
pub fn select(
    candidates: &[DiscoveredAdapter],
    request: SelectionRequest<'_>,
    core_ir_version: &str,
) -> SelectionReport {
    let mut required: Vec<String> = request.required.to_vec();
    required.sort();
    required.dedup();

    let mut survivors: Vec<(&DiscoveredAdapter, Vec<&'static str>)> = Vec::new();
    let mut excluded: Vec<ExcludedAdapter> = Vec::new();
    for candidate in candidates {
        let mut reasons: Vec<&'static str> = Vec::new();
        let mut warnings: Vec<&'static str> = Vec::new();

        // Fixed order, step 1: declared IR compatibility. A candidate
        // that never declared the core IR version is refused before any
        // project IR could reach it.
        if !candidate.ir_compatible(core_ir_version) {
            reasons.push(reasons::IR_UNDECLARED);
        }

        // Step 2: required-capability support under the explicit policy.
        for id in &required {
            match candidate.capability(id).map(|entry| entry.state) {
                Some(wire::SupportState::Full) => {}
                Some(wire::SupportState::Partial) => {
                    if request.policy.allow_partial {
                        warnings.push(warnings::PARTIAL_ACCEPTED);
                    } else {
                        reasons.push(reasons::PARTIAL_POLICY);
                    }
                }
                Some(wire::SupportState::Unknown) => {
                    if request.policy.tolerate_unknown {
                        warnings.push(warnings::UNKNOWN_TOLERATED);
                    } else {
                        reasons.push(reasons::CAPABILITY_UNKNOWN);
                    }
                }
                Some(wire::SupportState::Unsupported) | None => {
                    reasons.push(reasons::CAPABILITY_UNSUPPORTED);
                }
            }
        }

        if reasons.is_empty() {
            survivors.push((candidate, warnings));
        } else {
            reasons.sort_unstable();
            reasons.dedup();
            excluded.push(ExcludedAdapter {
                adapter: candidate.adapter.id.clone(),
                version: candidate.adapter.version.clone(),
                reasons,
            });
        }
    }

    // Deterministic survivor order: adapter id ascending, version
    // descending (newest first), selected profile, then every remaining
    // member the choice projects. Distinct identities must not leave the
    // chosen output dependent on caller input order; only byte-identical
    // projections compare Equal and may tie, because their outputs are
    // indistinguishable.
    survivors.sort_by(|left, right| projected_order(left.0, right.0, request.preferred_profile));

    let Some((chosen, warnings)) = survivors.first() else {
        excluded.sort_by(exclusion_order);
        return SelectionReport {
            policy: request.policy,
            required,
            selected: None,
            excluded,
        };
    };

    // Every other survivor is reported as `ordering`, never dropped.
    for survivor in &survivors[1..] {
        excluded.push(ExcludedAdapter {
            adapter: survivor.0.adapter.id.clone(),
            version: survivor.0.adapter.version.clone(),
            reasons: vec![reasons::ORDERING],
        });
    }
    excluded.sort_by(exclusion_order);

    let mut warnings = warnings.clone();
    warnings.sort_unstable();
    warnings.dedup();

    SelectionReport {
        policy: request.policy,
        required,
        selected: Some(SelectionChoice {
            adapter: chosen.adapter.clone(),
            negotiated_version: chosen.negotiated_version,
            capability_digest: chosen.capability_digest.clone(),
            executable_digest: chosen.executable_digest.clone(),
            profile: chosen.selected_profile(request.preferred_profile),
            ir_versions: chosen.ir_versions.clone(),
            capabilities: chosen
                .capabilities
                .iter()
                .map(|entry| SelectedCapability {
                    id: entry.id.clone(),
                    state: entry.state,
                    provenance: entry.provenance,
                    definition_version: entry.definition_version,
                })
                .collect(),
            warnings,
        }),
        excluded,
    }
}

/// The total projected order over candidates: the identity precedence
/// (adapter id ascending, version descending, selected profile) and then
/// every remaining member the selection choice projects — declared
/// digest, negotiated version, capability digest, executable digest, IR
/// declarations, capability records. Candidates that differ in any
/// projected member resolve deterministically, so the chosen output
/// never depends on caller input order; only byte-identical projections
/// compare Equal and may tie.
fn projected_order(
    left: &DiscoveredAdapter,
    right: &DiscoveredAdapter,
    preferred_profile: Option<&str>,
) -> std::cmp::Ordering {
    left.adapter
        .id
        .cmp(&right.adapter.id)
        .then_with(|| version_desc(&left.adapter.version, &right.adapter.version))
        .then_with(|| {
            left.selected_profile(preferred_profile)
                .cmp(&right.selected_profile(preferred_profile))
        })
        .then_with(|| left.adapter.digest.cmp(&right.adapter.digest))
        .then_with(|| left.negotiated_version.cmp(right.negotiated_version))
        .then_with(|| left.capability_digest.cmp(&right.capability_digest))
        .then_with(|| left.executable_digest.cmp(&right.executable_digest))
        .then_with(|| left.ir_versions.cmp(&right.ir_versions))
        .then_with(|| capability_records(left).cmp(capability_records(right)))
}

/// The projected capability-record order of one candidate. The records
/// are already sorted by id, so the tuple comparison is lexicographic by
/// (id, state, definition version, provenance).
fn capability_records(
    candidate: &DiscoveredAdapter,
) -> impl Iterator<Item = (&str, wire::SupportState, &'static str, Provenance)> + '_ {
    candidate.capabilities.iter().map(|entry| {
        (
            entry.id.as_str(),
            entry.state,
            entry.definition_version,
            entry.provenance,
        )
    })
}

/// The canonical exclusion order: adapter, version, then the sorted
/// reason tokens. Two rejected candidates that share an adapter and
/// version still serialize identically when their input order flips,
/// in every branch of the selection.
fn exclusion_order(left: &ExcludedAdapter, right: &ExcludedAdapter) -> std::cmp::Ordering {
    left.adapter
        .cmp(&right.adapter)
        .then_with(|| left.version.cmp(&right.version))
        .then_with(|| left.reasons.cmp(&right.reasons))
}

/// Descending exact-version comparator over the canonical
/// MAJOR.MINOR.PATCH spellings adapters declare.
fn version_desc(left: &str, right: &str) -> std::cmp::Ordering {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (left, right) = (parse(left), parse(right));
    right.cmp(&left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target_protocol::discovery::{DiscoveredCapability, Provenance};

    fn identity(id: &str, version: &str) -> wire::AdapterIdentity {
        wire::AdapterIdentity {
            id: id.to_owned(),
            version: version.to_owned(),
            digest: format!("sha256:{}", "ab".repeat(32)),
        }
    }

    fn adapter(
        id: &str,
        version: &str,
        ir: &[&str],
        capabilities: &[(&str, wire::SupportState)],
    ) -> DiscoveredAdapter {
        DiscoveredAdapter {
            adapter: identity(id, version),
            negotiated_version: crate::target_protocol::version::VERSION,
            declared_protocols: vec!["1.0.0".to_owned(), "1.1.0".to_owned()],
            ir_versions: ir.iter().map(|value| value.to_string()).collect(),
            constraints: None,
            targets: vec![id.to_owned()],
            profiles: vec!["default".to_owned()],
            capability_digest: format!("sha256:{}", "cd".repeat(32)),
            executable_digest: None,
            capabilities: capabilities
                .iter()
                .map(|(id, state)| DiscoveredCapability {
                    id: id.to_string(),
                    state: *state,
                    definition_version: "1.0.0",
                    provenance: Provenance::Declared,
                })
                .collect(),
        }
    }

    fn required(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    fn report(candidates: &[DiscoveredAdapter], req: &[String]) -> SelectionReport {
        select(
            candidates,
            SelectionRequest {
                required: req,
                preferred_profile: None,
                policy: SelectionPolicy::default(),
            },
            "0.1.0",
        )
    }

    #[test]
    fn incompatible_adapter_is_excluded_before_capabilities() {
        let candidates = [adapter(
            "node-typescript",
            "0.1.0",
            &["0.2.0"],
            &[("scan.symbols", wire::SupportState::Full)],
        )];
        let result = report(&candidates, &required(&["scan.symbols"]));
        assert!(!result.is_selected());
        assert_eq!(
            result.excluded[0].reasons,
            vec![reasons::IR_UNDECLARED],
            "the IR refusal comes first and alone"
        );
    }

    #[test]
    fn unknown_is_never_an_optimistic_yes() {
        let candidates = [adapter(
            "node-typescript",
            "0.1.0",
            &["0.1.0"],
            &[("scan.symbols", wire::SupportState::Unknown)],
        )];
        let strict = report(&candidates, &required(&["scan.symbols"]));
        assert!(!strict.is_selected());
        assert_eq!(
            strict.excluded[0].reasons,
            vec![reasons::CAPABILITY_UNKNOWN]
        );

        let lenient = select(
            &candidates,
            SelectionRequest {
                required: &required(&["scan.symbols"]),
                preferred_profile: None,
                policy: SelectionPolicy {
                    tolerate_unknown: true,
                    allow_partial: false,
                },
            },
            "0.1.0",
        );
        assert!(lenient.is_selected());
        assert_eq!(
            lenient.selected.as_ref().unwrap().warnings,
            vec![warnings::UNKNOWN_TOLERATED]
        );
    }

    #[test]
    fn partial_requires_the_explicit_policy() {
        let candidates = [adapter(
            "node-typescript",
            "0.1.0",
            &["0.1.0"],
            &[("scan.symbols", wire::SupportState::Partial)],
        )];
        let strict = report(&candidates, &required(&["scan.symbols"]));
        assert!(!strict.is_selected());
        assert_eq!(strict.excluded[0].reasons, vec![reasons::PARTIAL_POLICY]);

        let with_policy = select(
            &candidates,
            SelectionRequest {
                required: &required(&["scan.symbols"]),
                preferred_profile: None,
                policy: SelectionPolicy {
                    tolerate_unknown: false,
                    allow_partial: true,
                },
            },
            "0.1.0",
        );
        assert!(with_policy.is_selected());
        assert_eq!(
            with_policy.selected.as_ref().unwrap().warnings,
            vec![warnings::PARTIAL_ACCEPTED]
        );
    }

    #[test]
    fn unsupported_and_absent_capabilities_are_unavailable() {
        let candidates = [
            adapter(
                "adapter-a",
                "0.1.0",
                &["0.1.0"],
                &[("scan.symbols", wire::SupportState::Unsupported)],
            ),
            adapter("adapter-b", "0.1.0", &["0.1.0"], &[]),
        ];
        let result = report(&candidates, &required(&["scan.symbols"]));
        assert!(!result.is_selected());
        assert_eq!(result.excluded.len(), 2);
        assert_eq!(
            result.excluded[0].reasons,
            vec![reasons::CAPABILITY_UNSUPPORTED]
        );
        assert_eq!(
            result.excluded[1].reasons,
            vec![reasons::CAPABILITY_UNSUPPORTED]
        );
    }

    #[test]
    fn selection_order_is_deterministic_and_explainable() {
        let candidates = [
            adapter(
                "zeta-adapter",
                "1.0.0",
                &["0.1.0"],
                &[("scan.symbols", wire::SupportState::Full)],
            ),
            adapter(
                "alpha-adapter",
                "0.2.0",
                &["0.1.0"],
                &[("scan.symbols", wire::SupportState::Full)],
            ),
            adapter(
                "alpha-adapter",
                "0.9.0",
                &["0.1.0"],
                &[("scan.symbols", wire::SupportState::Full)],
            ),
        ];
        let result = report(&candidates, &required(&["scan.symbols"]));
        let selected = result.selected.as_ref().unwrap();
        assert_eq!(selected.adapter.id, "alpha-adapter");
        assert_eq!(
            selected.adapter.version, "0.9.0",
            "the newest declared version wins inside one identity"
        );
        let alpha_old = result
            .excluded
            .iter()
            .find(|entry| entry.adapter == "alpha-adapter")
            .unwrap();
        assert_eq!(alpha_old.version, "0.2.0");
        assert_eq!(alpha_old.reasons, vec![reasons::ORDERING]);
        let zeta = result
            .excluded
            .iter()
            .find(|entry| entry.adapter == "zeta-adapter")
            .unwrap();
        assert_eq!(zeta.reasons, vec![reasons::ORDERING]);
        assert_eq!(selected.capabilities[0].provenance, Provenance::Declared);
    }

    #[test]
    fn no_survivor_exclusions_canonically_order_shared_ids() {
        // Two rejected candidates share the adapter id with distinct
        // valid versions; another pair shares id and version but carries
        // distinct reasons. Flipping the caller's input order must not
        // flip the serialized exclusion list.
        let newest_ir = adapter(
            "adapter-a",
            "0.2.0",
            &[],
            &[("scan.symbols", wire::SupportState::Full)],
        );
        let older_ir = adapter(
            "adapter-a",
            "0.1.0",
            &[],
            &[("scan.symbols", wire::SupportState::Full)],
        );
        let unknown = adapter(
            "adapter-a",
            "0.1.0",
            &["0.1.0"],
            &[("scan.symbols", wire::SupportState::Unknown)],
        );
        let forward = report(
            &[newest_ir.clone(), older_ir.clone(), unknown.clone()],
            &required(&["scan.symbols"]),
        );
        let reversed = report(
            &[unknown, older_ir, newest_ir],
            &required(&["scan.symbols"]),
        );
        assert_eq!(
            forward.excluded, reversed.excluded,
            "the serialized exclusion list is canonical, not input-ordered"
        );
        assert_eq!(
            forward.excluded,
            vec![
                ExcludedAdapter {
                    adapter: "adapter-a".to_owned(),
                    version: "0.1.0".to_owned(),
                    reasons: vec![reasons::CAPABILITY_UNKNOWN],
                },
                ExcludedAdapter {
                    adapter: "adapter-a".to_owned(),
                    version: "0.1.0".to_owned(),
                    reasons: vec![reasons::IR_UNDECLARED],
                },
                ExcludedAdapter {
                    adapter: "adapter-a".to_owned(),
                    version: "0.2.0".to_owned(),
                    reasons: vec![reasons::IR_UNDECLARED],
                },
            ],
            "the canonical exclusion order is id, version, reasons"
        );
    }

    #[test]
    fn survivor_ties_resolve_without_caller_input_order() {
        // Same id, version, and profile; each pair differs in exactly one
        // other projected member. The chosen projection must not depend
        // on the caller's input order.
        let variant = |digest: &str, executable: Option<&str>, capability: &str, ir: &[&str]| {
            let mut candidate = adapter(
                "adapter-a",
                "0.1.0",
                ir,
                &[("scan.symbols", wire::SupportState::Full)],
            );
            candidate.adapter.digest = digest.to_owned();
            candidate.capability_digest = capability.to_owned();
            candidate.executable_digest = executable.map(str::to_owned);
            candidate
        };
        let mut with_partial = adapter(
            "adapter-a",
            "0.1.0",
            &["0.1.0"],
            &[("scan.symbols", wire::SupportState::Partial)],
        );
        with_partial.adapter.digest = "sha256:aa".to_owned();
        with_partial.capability_digest = "sha256:cc".to_owned();
        // Full and Partial records differ while both proceed under the
        // explicit partial policy, projecting distinct capability
        // records and warnings.
        let pairs = [
            (
                variant("sha256:aa", None, "sha256:cc", &["0.1.0"]),
                variant("sha256:bb", None, "sha256:cc", &["0.1.0"]),
                SelectionPolicy::default(),
                "declared digest",
            ),
            (
                variant("sha256:aa", Some("sha256:11"), "sha256:cc", &["0.1.0"]),
                variant("sha256:aa", Some("sha256:22"), "sha256:cc", &["0.1.0"]),
                SelectionPolicy::default(),
                "executable digest",
            ),
            (
                variant("sha256:aa", None, "sha256:cc", &["0.1.0"]),
                variant("sha256:aa", None, "sha256:dd", &["0.1.0"]),
                SelectionPolicy::default(),
                "capability digest",
            ),
            (
                variant("sha256:aa", None, "sha256:cc", &["0.1.0", "0.2.0"]),
                variant("sha256:aa", None, "sha256:cc", &["0.2.0", "0.1.0"]),
                SelectionPolicy::default(),
                "ir declaration order",
            ),
            (
                variant("sha256:aa", None, "sha256:cc", &["0.1.0"]),
                with_partial.clone(),
                SelectionPolicy {
                    allow_partial: true,
                    tolerate_unknown: false,
                },
                "capability record",
            ),
        ];
        for (first, second, policy, label) in pairs {
            let run = |candidates: &[DiscoveredAdapter]| {
                select(
                    candidates,
                    SelectionRequest {
                        required: &required(&["scan.symbols"]),
                        preferred_profile: None,
                        policy,
                    },
                    "0.1.0",
                )
            };
            let forward = run(&[first.clone(), second.clone()]);
            let reversed = run(&[second.clone(), first.clone()]);
            assert_eq!(
                forward.selected, reversed.selected,
                "the chosen projection is input-order-independent ({label})"
            );
            assert_eq!(
                forward.excluded, reversed.excluded,
                "the ordering exclusions are input-order-independent ({label})"
            );
        }
    }

    #[test]
    fn empty_candidates_select_nothing() {
        let result = report(&[], &required(&["scan.symbols"]));
        assert!(!result.is_selected());
        assert!(result.excluded.is_empty());
        assert_eq!(result.required, vec!["scan.symbols".to_owned()]);
    }

    #[test]
    fn the_report_serializes_with_closed_sorted_members() {
        let candidates = [adapter(
            "node-typescript",
            "0.1.0",
            &["0.1.0"],
            &[("scan.symbols", wire::SupportState::Full)],
        )];
        let result = report(&candidates, &required(&["scan.symbols"]));
        let bytes = serde_json::to_vec(&result).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec!["excluded", "policy", "required", "selected"],
            "the report serializes every closed member"
        );
        let selected = value["selected"].as_object().unwrap();
        assert_eq!(selected["profile"], "default");
        assert_eq!(selected["capabilities"][0]["id"], "scan.symbols");
        assert_eq!(selected["capabilities"][0]["definition_version"], "1.0.0");
        assert_eq!(selected["capabilities"][0]["provenance"], "declared");
    }
}
