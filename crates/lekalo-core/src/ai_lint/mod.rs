//! Optional AI readability analysis over accepted Model/graph and admitted
//! adapter evidence. This module never parses a target language or executes it.
pub mod collect;
pub(crate) mod compare;
mod depth;
pub mod diagnostic;
pub mod input;
pub mod wire;
use crate::{
    ir::{Compilation, Definition},
    result::{DomainResult, Status},
};
use input::{hash, header};
use std::collections::{BTreeMap, BTreeSet};
pub use wire::*;
pub const RULES: [&str; 11] = [
    "ambiguity.implicit-target-defaults",
    "ambiguity.multiple-resolutions",
    "ambiguity.scattered-state-writes",
    "hidden.convention-only-path",
    "hidden.dispatch-without-binding",
    "hidden.observer-write",
    "hidden.path-without-trace-owner",
    "hidden.reflective-call",
    "hidden.string-reference",
    "hidden.undeclared-effect",
    "indirection.depth-exceeded",
];
pub struct Request<'a> {
    pub compilation: &'a Compilation,
    pub model_ref: &'a str,
    /// Current load-envelope pin used by the existing artifact producer.
    pub artifact_model_ref: &'a str,
    pub scope: Vec<String>,
    pub config: &'a Config,
    pub profile: &'a str,
    pub evidence: &'a [Evidence],
    pub waivers: Option<&'a Waivers>,
    pub as_of: Option<&'a str>,
    pub baseline: Option<&'a Report>,
    pub check: bool,
    pub transitions: Option<&'a crate::invariant_transition::InvariantTransitionAttachment>,
    pub trace: Option<&'a crate::trace::TraceManifest>,
    pub artifacts: Option<&'a crate::artifacts::ArtifactManifest>,
    pub observed: Option<&'a crate::observed::ObservedIndex>,
    pub fs: &'a crate::project_fs::Fs,
}
pub fn default_config() -> Config {
    let h = header("config");
    Config {
        schema_version: h.0,
        identity: h.1,
        registry_ref: input::registry_ref(),
        related_writer_groups: Vec::new(),
        permitted_defaults: Vec::new(),
        profiles: ["advisory", "ci", "off"]
            .into_iter()
            .map(|id| Profile {
                id: id.into(),
                recipe: "ai-readability/1".into(),
                rules: RULES
                    .iter()
                    .map(|r| Rule {
                        id: (*r).into(),
                        enabled: id != "off",
                        severity: State::Unknown,
                    })
                    .collect(),
                thresholds: Thresholds {
                    semantic_dependency_depth: State::Unknown,
                    native_call_depth: State::Unknown,
                    uncovered_writer_groups: State::Unknown,
                },
                gate: Gate {
                    minimum_confidence: Confidence::High,
                    fail_on_active_warnings: id == "ci",
                    required_coverage: Vec::new(),
                    require_comparable_baseline: false,
                    fail_on_regression: false,
                },
            })
            .collect(),
    }
}
fn enabled(profile: &Profile, rule: &str) -> bool {
    profile.rules.iter().any(|r| r.id == rule && r.enabled)
}
fn current(e: &Evidence, r: &Record, model: &str, ir: &str) -> bool {
    r.currency == Currency::Current
        && e.pins.model.known().is_some_and(|p| p == model)
        && e.pins.ir.known().is_some_and(|p| p == ir)
}
fn covered(e: &Evidence, rule: &str) -> bool {
    e.scope.iter().all(|s| {
        e.coverage
            .iter()
            .any(|c| c.rule == rule && &c.scope == s && c.state == CoverageState::Complete)
    })
}
fn confidence(r: &Record) -> Confidence {
    let original = r
        .activation
        .iter()
        .map(|s| s.confidence)
        .chain(
            r.candidates
                .iter()
                .filter(|c| c.currency == Currency::Current)
                .map(|c| c.confidence),
        )
        .fold(r.confidence, std::cmp::min);
    let cap = if r.origin == Origin::Inferred {
        Confidence::Medium
    } else if r.claim == Claim::Structural {
        Confidence::Exact
    } else {
        Confidence::High
    };
    std::cmp::min(original, cap)
}
#[allow(clippy::too_many_arguments)]
fn make(
    rule: &str,
    subject: &str,
    symbol: State<String>,
    target: &str,
    scope: &[String],
    confidence: Confidence,
    claim: Claim,
    condition: &str,
    evidence: Vec<String>,
    locations: Vec<String>,
    witness: Vec<String>,
    guards: Vec<String>,
    profile: &Profile,
) -> Finding {
    let registry =
        crate::diagnostics::registry::DiagnosticRegistry::embedded().expect("embedded registry");
    let entry = registry.entry(rule).expect("AI lint rule");
    let alternative = match rule {
        "ambiguity.multiple-resolutions" => "Declare one context-qualified semantic binding and retain every competing candidate.",
        "ambiguity.scattered-state-writes" => "Declare field assignments through a state transition owner or a justified related writer group.",
        "ambiguity.implicit-target-defaults" => "Declare a shared explicit setting or a justified target-specific difference.",
        "hidden.observer-write" | "hidden.undeclared-effect" => "Declare the command effect and activation guards or replace the hook with an explicit call.",
        "hidden.path-without-trace-owner" => "Declare the semantic owner and a revision-bound trace link.",
        "hidden.convention-only-path" => "Declare the effective configuration and its source.",
        "indirection.depth-exceeded" => "Introduce a direct typed boundary for the witnessed dependency path.",
        _ => "Use typed injection or an explicit closed reference map with binding evidence.",
    };
    let severity = if confidence <= Confidence::Low
        || profile
            .rules
            .iter()
            .any(|r| r.id == rule && r.severity.known().is_some_and(|s| s == "info"))
    {
        "info"
    } else {
        "warning"
    };
    Finding {
        id: hash(&(rule, subject, target, &symbol)),
        rule_id: rule.into(),
        code: entry.code().into(),
        subject: subject.into(),
        semantic_symbol: symbol,
        target: target.into(),
        scope: scope.to_vec(),
        confidence,
        claim,
        condition_digest: condition.into(),
        evidence,
        locations,
        witness,
        guards,
        disposition: "active".into(),
        waiver: State::Unknown,
        alternative: alternative.into(),
        message: entry.default_message().into(),
        severity: severity.into(),
    }
}
fn from_record(
    rule: &str,
    e: &Evidence,
    r: &Record,
    config_ref: &str,
    scope: &[String],
    p: &Profile,
) -> Finding {
    let mut guards: Vec<String> = r
        .guards
        .iter()
        .chain(r.activation.iter().flat_map(|s| &s.guards))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    guards.truncate(32);
    // Semantic identity stays stable when byte positions or revisions change.
    // Unmapped native identities are opaque in the default projection.
    let subject = hash(&(
        &r.semantic_symbol,
        &r.native_id,
        &r.resource,
        &r.field,
        &r.key,
        &r.subject,
    ));
    make(
        rule,
        &subject,
        r.semantic_symbol.clone(),
        &e.target,
        scope,
        confidence(r),
        r.claim,
        &hash(&(config_ref, &e.sources, r)),
        vec![format!("{}#{}", hash(e), r.id)],
        r.locations.clone(),
        r.activation.iter().map(|s| hash(&s.identity)).collect(),
        guards,
        p,
    )
}
pub fn analyze(request: &Request<'_>) -> Result<Report, DomainResult> {
    let p = request
        .config
        .profiles
        .iter()
        .find(|p| p.id == request.profile)
        .ok_or_else(|| diagnostic::failure("ai-lint.input-invalid", "profile-missing"))?;
    if request.scope.is_empty()
        || request.scope.len() > input::MAX_ROWS
        || !input::sorted(&request.scope)
        || request.as_of.is_some_and(|s| !input::date(s))
    {
        return Err(diagnostic::failure(
            "ai-lint.input-invalid",
            "selection-date",
        ));
    }
    let rows = request
        .evidence
        .iter()
        .map(|e| e.records.len() + e.coverage.len() + e.locations.len() + e.sources.len())
        .sum::<usize>();
    let source_bytes = request
        .evidence
        .iter()
        .flat_map(|e| &e.sources)
        .try_fold(0usize, |n, s| n.checked_add(s.bytes as usize));
    let join_rows = request.compilation.project.definitions.len()
        + request.config.related_writer_groups.len()
        + request.trace.map_or(0, |t| {
            t.manifest().nodes.len() + t.manifest().relations.len()
        })
        + request.transitions.map_or(0, |t| t.transitions().len());
    if request.evidence.len() > 64
        || rows > input::MAX_ROWS
        || source_bytes.map_or(true, |n| n > input::MAX_BYTES)
        || rows
            .saturating_mul(join_rows.saturating_add(rows))
            .saturating_mul(4)
            > input::MAX_WORK
    {
        return Err(diagnostic::failure(
            "ai-lint.input-invalid",
            "analysis-work-limit",
        ));
    }
    let graph = crate::graph::build(&request.compilation.project).map_err(DomainResult::invalid)?;
    for s in &request.scope {
        if graph.resolve(s).is_none() {
            return Err(diagnostic::failure(
                "ai-lint.input-invalid",
                "selection-symbol",
            ));
        }
    }
    let ir_ref = input::digest(request.compilation.project.to_canonical_json().as_bytes());
    let config_ref = hash(request.config);
    let profile_ref = hash(p);
    let h = header("report");
    let mut report = Report {
        schema_version: h.0,
        identity: h.1,
        registry_ref: input::registry_ref(),
        model_ref: request.model_ref.into(),
        ir_ref: ir_ref.clone(),
        config_ref: config_ref.clone(),
        profile_ref,
        waiver_ref: request
            .waivers
            .map_or(State::Unknown, |w| State::Known(hash(w))),
        evidence_refs: request.evidence.iter().map(hash).collect(),
        attachment_refs: Vec::new(),
        scope: request.scope.clone(),
        profile: p.id.clone(),
        mode: if request.check { "check" } else { "advisory" }.into(),
        as_of: request
            .as_of
            .map_or(State::Unknown, |s| State::Known(s.into())),
        recipes: vec![
            "ai-readability/1".into(),
            "semantic-dependency/1".into(),
            "native-call/1".into(),
        ],
        coverage: Vec::new(),
        findings: Vec::new(),
        summary: Summary {
            raw: 0,
            active: 0,
            waived: 0,
            possible_effects: 0,
            verified_effects: 0,
            ambiguity_sets: 0,
            uncovered_writer_groups: State::Unknown,
            diagnostic_projection_truncated: false,
        },
        metrics: Vec::new(),
        depths: Vec::new(),
        waiver_audit: Vec::new(),
        comparison: State::Unknown,
    };
    report.evidence_refs.sort();
    report.evidence_refs.dedup();
    if let Some(t) = request.transitions {
        if t.model_ref().digest().as_str() != request.model_ref || t.ir_digest().as_str() != ir_ref
        {
            return Err(diagnostic::failure(
                "ai-lint.input-invalid",
                "transition-pin",
            ));
        }
        report.attachment_refs.push(input::digest(
            t.canonical_bytes()
                .map_err(DomainResult::invalid)?
                .as_bytes(),
        ));
    }
    if let Some(t) = request.trace {
        report
            .attachment_refs
            .push(t.digest().map_err(DomainResult::invalid)?);
    }
    if let Some(o) = request.observed {
        report.attachment_refs.push(hash(o));
    }
    if let Some(a) = request.artifacts {
        let lock = request
            .fs
            .read_file_opt("", "lekalo.lock", input::MAX_BYTES)
            .ok()
            .flatten()
            .and_then(|b| crate::lockfile::Lockfile::parse_canonical(&b).ok());
        if a.model().digest().as_str() != request.artifact_model_ref
            || a.ir().digest().as_str() != ir_ref
            || lock.as_ref().map_or(true, |l| {
                l.digest().as_str() != a.lock_ref().digest().as_str()
            })
        {
            return Err(diagnostic::failure("ai-lint.input-invalid", "artifact-pin"));
        }
        report
            .attachment_refs
            .push(a.manifest_digest().as_str().into());
    }
    report.attachment_refs.sort();
    let mut targets = BTreeSet::new();
    for e in request.evidence {
        if !targets.insert(&e.target) {
            return Err(diagnostic::failure(
                "ai-lint.input-invalid",
                "duplicate-target",
            ));
        }
        input::admit_evidence(e, request.fs, request.model_ref, &ir_ref, &request.scope)?;
        if e.pins
            .observed
            .known()
            .is_some_and(|pin| request.observed.map_or(true, |o| &hash(o) != pin))
        {
            return Err(diagnostic::failure("ai-lint.input-invalid", "observed-pin"));
        }
        for r in &e.records {
            if let Some(s) = r.semantic_symbol.known() {
                if graph.resolve(s).is_none() {
                    return Err(diagnostic::failure(
                        "ai-lint.input-invalid",
                        "record-symbol",
                    ));
                }
            }
            if let Some(s) = r.operation.known() {
                if !request
                    .scope
                    .iter()
                    .any(|root| root == s || s.starts_with(&format!("{root}.")))
                {
                    return Err(diagnostic::failure(
                        "ai-lint.input-invalid",
                        "record-outside-selection",
                    ));
                }
                if !request
                    .compilation
                    .project
                    .definitions
                    .iter()
                    .any(|d| matches!(d, Definition::Command(c) if c.id.as_str()==s))
                {
                    return Err(diagnostic::failure(
                        "ai-lint.input-invalid",
                        "record-command",
                    ));
                }
            }
            if let Some(resource) = r.resource.known() {
                if !request.compilation.project.definitions.iter().any(|d| matches!(d, Definition::Entity(entity) if entity.id.as_str()==resource && r.field.known().map_or(true, |field| entity.fields.iter().any(|f| f.name.as_str()==field)))) { return Err(diagnostic::failure("ai-lint.input-invalid", "record-resource-field")); }
            }
        }
    }
    // Core semantic graph depth is distinct from the adapter's native calls.
    let edges = graph
        .edges()
        .iter()
        .filter(|e| {
            [
                "references",
                "accepts",
                "returns",
                "reads",
                "emits",
                "exposes",
            ]
            .contains(&e.key().relation().key())
        })
        .map(|e| {
            (
                e.key().from().semantic_id().into(),
                e.key().to().semantic_id().into(),
            )
        })
        .collect::<Vec<_>>();
    if enabled(p, "indirection.depth-exceeded") {
        let roots = request
            .compilation
            .project
            .definitions
            .iter()
            .map(|d| d.id().as_str())
            .filter(|id| {
                request
                    .scope
                    .iter()
                    .any(|root| *id == root || id.starts_with(&format!("{root}.")))
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let d = depth::measure("semantic-dependency", "model", &edges, &roots);
        if let (Some(value), Some(limit)) = (
            d.maximum.known(),
            p.thresholds.semantic_dependency_depth.known(),
        ) {
            if value > limit {
                report.findings.push(make(
                    "indirection.depth-exceeded",
                    d.witness.first().map_or("model", String::as_str),
                    State::Known(
                        d.witness
                            .first()
                            .cloned()
                            .unwrap_or_else(|| request.scope[0].clone()),
                    ),
                    "model",
                    &request.scope,
                    Confidence::Exact,
                    Claim::Structural,
                    &hash(&(&config_ref, &ir_ref, &d)),
                    vec![ir_ref.clone()],
                    Vec::new(),
                    d.witness.clone(),
                    Vec::new(),
                    p,
                ));
            }
        }
        report.depths.push(d);
    }
    for e in request.evidence {
        for rule in RULES {
            for scope in &request.scope {
                let mut c = e
                    .coverage
                    .iter()
                    .find(|c| c.rule == rule && &c.scope == scope)
                    .cloned()
                    .unwrap_or_else(|| Coverage {
                        rule: rule.into(),
                        target: e.target.clone(),
                        scope: scope.clone(),
                        state: CoverageState::Unknown,
                        eligible: State::Unknown,
                        examined: State::Unknown,
                        limitations: vec!["evidence-absent".into()],
                    });
                if !enabled(p, rule) {
                    c.state = CoverageState::Disabled;
                    c.eligible = State::Unknown;
                    c.examined = State::Unknown;
                    c.limitations = vec!["rule-disabled".into()];
                } else if e.pins.model.known().is_none()
                    || e.pins.ir.known().is_none()
                    || e.records.iter().any(|r| {
                        r.currency != Currency::Current
                            || confidence(r) == Confidence::Unknown
                            || r.candidates.iter().any(|c| c.currency != Currency::Current)
                    })
                {
                    c.state = CoverageState::Partial;
                    c.limitations = vec!["custody-or-currency-unknown".into()];
                }
                let incomplete_trace = rule == "hidden.path-without-trace-owner"
                    && request.trace.map_or(true, |t| {
                        let m = t.manifest();
                        m.completeness != crate::trace::Completeness::Full
                            || (m.model_ref.digest != request.model_ref
                                && m.model_ref.digest != request.artifact_model_ref)
                            || m.ir_ref.as_ref().map_or(true, |v| v.digest != ir_ref)
                            || e.pins.revision.known() != Some(&m.source_revision)
                            || request.artifacts.map_or(true, |a| {
                                m.artifact_manifest_ref
                                    .as_ref()
                                    .map_or(true, |pin| pin.digest != a.manifest_digest().as_str())
                            })
                    });
                if enabled(p, rule)
                    && (incomplete_trace
                        || rule == "ambiguity.scattered-state-writes"
                            && request.transitions.is_none())
                {
                    c.state = CoverageState::Unknown;
                    c.limitations = vec!["required-attachment-absent-or-incomplete".into()];
                }
                let incomplete_effect = e.records.iter().any(|r| {
                    r.kind == Kind::Effect
                        && rule
                            == if r.mechanism == Mechanism::Observer {
                                "hidden.observer-write"
                            } else {
                                "hidden.undeclared-effect"
                            }
                        && (r.operation.known().is_none()
                            || r.resource.known().is_none()
                            || r.key.known().is_none())
                });
                if enabled(p, rule) && incomplete_effect {
                    c.state = CoverageState::Partial;
                    c.limitations
                        .push("effect-comparison-input-unavailable".into());
                    c.limitations.sort();
                    c.limitations.dedup();
                }
                report.coverage.push(c);
            }
        }
        for r in &e.records {
            if !current(e, r, request.model_ref, &ir_ref) || confidence(r) == Confidence::Unknown {
                continue;
            }
            let rule = match r.kind {
                Kind::BindingCandidates
                    if r.candidates
                        .iter()
                        .filter(|c| c.currency == Currency::Current)
                        .map(|c| &c.identity)
                        .collect::<BTreeSet<_>>()
                        .len()
                        > 1 =>
                {
                    Some("ambiguity.multiple-resolutions")
                }
                Kind::Dispatch
                    if r.binding == State::Known(false)
                        && covered(e, "hidden.dispatch-without-binding") =>
                {
                    Some("hidden.dispatch-without-binding")
                }
                Kind::Convention
                    if r.configuration == State::Known(false)
                        && covered(e, "hidden.convention-only-path") =>
                {
                    Some("hidden.convention-only-path")
                }
                Kind::Reflection if r.binding == State::Known(false) => {
                    Some("hidden.reflective-call")
                }
                Kind::StringReference if r.binding == State::Known(false) => {
                    Some("hidden.string-reference")
                }
                Kind::Effect => {
                    let declared = r
                        .operation
                        .known()
                        .zip(r.resource.known())
                        .zip(r.key.known())
                        .map(|((op, resource), action)| {
                            declared_effect(request.compilation, op, resource, action)
                        });
                    if declared == Some(false) {
                        if r.mechanism == Mechanism::Observer {
                            Some("hidden.observer-write")
                        } else {
                            Some("hidden.undeclared-effect")
                        }
                    } else {
                        None
                    }
                }
                Kind::Path if covered(e, "hidden.path-without-trace-owner") => {
                    request.trace.and_then(|t| {
                        let m = t.manifest();
                        if m.completeness != crate::trace::Completeness::Full
                            || m.model_ref.digest != request.model_ref
                            || m.ir_ref.as_ref().map_or(true, |ir| ir.digest != ir_ref)
                            || (e.pins.revision.known() != Some(&m.source_revision))
                            || request.artifacts.map_or(true, |a| {
                                m.artifact_manifest_ref
                                    .as_ref()
                                    .map_or(true, |pin| pin.digest != a.manifest_digest().as_str())
                            })
                        {
                            return None;
                        }
                        let paths: BTreeSet<&str> = r
                            .locations
                            .iter()
                            .filter_map(|id| e.locations.iter().find(|l| &l.id == id))
                            .filter_map(|l| e.sources.iter().find(|s| s.id == l.source))
                            .map(|s| s.path.as_str())
                            .collect();
                        let owned = m.nodes.iter().any(|n| {
                            n.path.as_deref().is_some_and(|p| paths.contains(p))
                                && n.ownership.is_some()
                                && request.artifacts.is_some_and(|a| {
                                    a.artifacts().iter().any(|entry| {
                                        Some(entry.key().path().as_str()) == n.path.as_deref()
                                            && Some(entry.content().as_str())
                                                == n.content_digest.as_deref()
                                            && n.ownership.as_ref().is_some_and(|o| {
                                                o.as_str() == entry.lifecycle().as_str()
                                            })
                                            && r.semantic_symbol.known().is_some_and(|s| {
                                                entry.key().semantic_owner().as_str() == s
                                            })
                                    })
                                })
                                && n.content_digest.as_ref().is_some_and(|d| {
                                    e.sources.iter().any(|s| {
                                        s.path == n.path.clone().unwrap_or_default()
                                            && &s.fingerprint == d
                                    })
                                })
                        });
                        let traced = r.semantic_symbol.known().is_some_and(|s| {
                            m.relations.iter().any(|relation| {
                                relation.relation_kind == crate::trace::RelationKind::Binds
                                    && m.nodes.iter().any(|n| {
                                        n.node_id == relation.from_node
                                            && n.semantic_id.as_ref() == Some(s)
                                    })
                                    && m.nodes.iter().any(|n| {
                                        n.node_id == relation.to_node
                                            && n.path.as_deref().is_some_and(|p| paths.contains(p))
                                            && n.content_digest.as_ref().is_some_and(|d| {
                                                e.sources
                                                    .iter()
                                                    .any(|source| source.fingerprint == *d)
                                            })
                                    })
                            })
                        });
                        (!owned || !traced).then_some("hidden.path-without-trace-owner")
                    })
                }
                _ => None,
            };
            if let Some(rule) = rule.filter(|r| enabled(p, r)) {
                report
                    .findings
                    .push(from_record(rule, e, r, &config_ref, &request.scope, p));
            }
        }
        if enabled(p, "indirection.depth-exceeded") {
            let native_records = e
                .records
                .iter()
                .filter(|r| r.kind == Kind::NativeEdge)
                .collect::<Vec<_>>();
            let native_confidence = native_records
                .iter()
                .map(|r| confidence(r))
                .fold(Confidence::High, std::cmp::min);
            let incomplete = !covered(e, "indirection.depth-exceeded")
                || native_records.iter().any(|r| {
                    !current(e, r, request.model_ref, &ir_ref)
                        || confidence(r) == Confidence::Unknown
                        || r.value.known().is_none()
                });
            let native = native_records
                .iter()
                .filter(|r| current(e, r, request.model_ref, &ir_ref))
                .filter_map(|r| r.value.known().map(|v| (hash(&r.native_id), hash(v))))
                .collect::<Vec<_>>();
            let roots = native.iter().map(|(a, _)| a.clone()).collect::<Vec<_>>();
            let mut d = depth::measure("native-call", &e.target, &native, &roots);
            if incomplete {
                d.maximum = State::Unknown;
            }
            if d.maximum.known().is_none() {
                for c in report
                    .coverage
                    .iter_mut()
                    .filter(|c| c.target == e.target && c.rule == "indirection.depth-exceeded")
                {
                    c.state = CoverageState::Partial;
                    c.limitations
                        .push("native-depth-incomplete-or-bounded".into());
                    c.limitations.sort();
                    c.limitations.dedup();
                }
            }
            if let (Some(value), Some(limit)) =
                (d.maximum.known(), p.thresholds.native_call_depth.known())
            {
                if value > limit {
                    report.findings.push(make(
                        "indirection.depth-exceeded",
                        d.witness.first().map_or("native", String::as_str),
                        State::Unknown,
                        &e.target,
                        &request.scope,
                        native_confidence,
                        Claim::Structural,
                        &hash(&(&config_ref, &e.sources, &d)),
                        vec![hash(e)],
                        Vec::new(),
                        d.witness.clone(),
                        Vec::new(),
                        p,
                    ));
                }
            }
            report.depths.push(d);
        }
    }
    observed_ambiguities(request, &mut report, p, &config_ref)?;
    state_writes(request, &mut report, p, &config_ref, &ir_ref);
    defaults(request, &mut report, p, &config_ref, &ir_ref);
    for rule in RULES {
        if rule != "indirection.depth-exceeded" && !request.evidence.is_empty() {
            continue;
        }
        for scope in &request.scope {
            let core = rule == "indirection.depth-exceeded"
                && p.thresholds.semantic_dependency_depth.known().is_some();
            report.coverage.push(Coverage {
                rule: rule.into(),
                target: "model".into(),
                scope: scope.clone(),
                state: if !enabled(p, rule) {
                    CoverageState::Disabled
                } else if core
                    && report.depths.iter().any(|d| {
                        d.dimension == "semantic-dependency" && d.maximum.known().is_some()
                    })
                {
                    CoverageState::Complete
                } else {
                    CoverageState::Unknown
                },
                eligible: if core {
                    State::Known(request.scope.len() as u64)
                } else {
                    State::Unknown
                },
                examined: if core {
                    State::Known(request.scope.len() as u64)
                } else {
                    State::Unknown
                },
                limitations: if core {
                    Vec::new()
                } else {
                    vec![if !enabled(p, rule) {
                        "rule-disabled"
                    } else {
                        "target-or-threshold-evidence-required"
                    }
                    .into()]
                },
            });
        }
    }
    // Core never truncates an apparently clean finding set.
    if report.findings.len() > input::MAX_ROWS {
        return Err(diagnostic::failure(
            "ai-lint.input-invalid",
            "finding-limit",
        ));
    }
    report.findings.sort_by(|a, b| a.id.cmp(&b.id));
    // Coalesce evidence for the same semantic condition without losing routes.
    let mut merged: Vec<Finding> = Vec::new();
    for f in std::mem::take(&mut report.findings) {
        if let Some(previous) = merged.last_mut().filter(|p| p.id == f.id) {
            previous.confidence = previous.confidence.min(f.confidence);
            previous.condition_digest = hash(&(&previous.condition_digest, &f.condition_digest));
            for (left, right) in [
                (&mut previous.evidence, f.evidence),
                (&mut previous.locations, f.locations),
                (&mut previous.witness, f.witness),
                (&mut previous.guards, f.guards),
            ] {
                left.extend(right);
                left.sort();
                left.dedup();
                if left.len() > 32 {
                    return Err(diagnostic::failure(
                        "ai-lint.input-invalid",
                        "finding-witness-limit",
                    ));
                }
            }
            if previous.confidence <= Confidence::Low {
                previous.severity = "info".into();
            }
        } else {
            merged.push(f);
        }
    }
    report.findings = merged;
    apply_waivers(request, &mut report);
    report
        .coverage
        .sort_by(|a, b| (&a.rule, &a.target, &a.scope).cmp(&(&b.rule, &b.target, &b.scope)));
    report
        .depths
        .sort_by(|a, b| (&a.dimension, &a.target).cmp(&(&b.dimension, &b.target)));
    report.summary.raw = report.findings.len() as u64;
    report.summary.waived = report
        .findings
        .iter()
        .filter(|f| f.disposition == "waived")
        .count() as u64;
    report.summary.active = report.summary.raw - report.summary.waived;
    report.summary.ambiguity_sets = report
        .findings
        .iter()
        .filter(|f| f.rule_id == "ambiguity.multiple-resolutions")
        .count() as u64;
    report.summary.possible_effects = report
        .findings
        .iter()
        .filter(|f| {
            ["hidden.observer-write", "hidden.undeclared-effect"].contains(&f.rule_id.as_str())
                && f.claim == Claim::PossibleBehavior
        })
        .count() as u64;
    report.summary.verified_effects = 0;
    for rule in RULES {
        for confidence in [
            Confidence::Unknown,
            Confidence::Low,
            Confidence::Medium,
            Confidence::High,
            Confidence::Exact,
        ] {
            let fs: Vec<_> = report
                .findings
                .iter()
                .filter(|f| f.rule_id == rule && f.confidence == confidence)
                .collect();
            let waived = fs.iter().filter(|f| f.disposition == "waived").count() as u64;
            report.metrics.push(Metric {
                rule: rule.into(),
                confidence,
                raw: fs.len() as u64,
                active: fs.len() as u64 - waived,
                waived,
            });
        }
    }
    report.summary.diagnostic_projection_truncated = report
        .findings
        .iter()
        .filter(|f| f.disposition == "active")
        .count()
        > 252;
    if let Some(base) = request.baseline {
        report.comparison = State::Known(compare::compare(base, &report));
    }
    compare::validate_report(&report)?;
    if input::canonical(&report).len() > input::MAX_BYTES {
        return Err(diagnostic::failure("ai-lint.input-invalid", "report-limit"));
    }
    Ok(report)
}
fn declared_effect(compilation: &Compilation, op: &str, resource: &str, operation: &str) -> bool {
    compilation.project.definitions.iter().any(|d| if let Definition::Command(c) = d { c.id.as_str()==op && c.effects.iter().any(|id| compilation.project.definitions.iter().any(|e| matches!(e, Definition::Effect(e) if &e.id==id && e.entity.as_str()==resource && e.operation.as_str()==operation))) } else {false})
}
fn observed_ambiguities(
    request: &Request<'_>,
    report: &mut Report,
    p: &Profile,
    config: &str,
) -> Result<(), DomainResult> {
    let Some(o) = request
        .observed
        .filter(|_| enabled(p, "ambiguity.multiple-resolutions"))
    else {
        return Ok(());
    };
    for r in &o.symbols {
        if !request
            .scope
            .iter()
            .any(|s| s == &r.id || r.id.starts_with(&format!("{s}.")))
        {
            continue;
        }
        let mut candidates = Vec::new();
        for c in &r.candidates {
            if let Some(fp) = &c.fingerprint {
                if input::read_source(request.fs, &c.path)
                    .ok()
                    .is_some_and(|bytes| input::digest(&bytes) == *fp)
                {
                    candidates.push(hash(&c.native));
                }
            }
        }
        candidates.sort();
        candidates.dedup();
        if candidates.len() > 1 {
            report.findings.push(make(
                "ambiguity.multiple-resolutions",
                &r.id,
                State::Known(r.id.clone()),
                o.target.as_deref().unwrap_or("observed"),
                &request.scope,
                Confidence::Exact,
                Claim::Structural,
                &hash(&(config, r)),
                vec![hash(o)],
                Vec::new(),
                candidates.into_iter().take(32).collect(),
                Vec::new(),
                p,
            ));
        }
    }
    Ok(())
}
fn state_writes(request: &Request<'_>, report: &mut Report, p: &Profile, config: &str, ir: &str) {
    if !enabled(p, "ambiguity.scattered-state-writes") {
        return;
    }
    if request.transitions.is_none() {
        return;
    }
    let Some(limit) = p.thresholds.uncovered_writer_groups.known() else {
        return;
    };
    type WriterRows<'a> =
        BTreeMap<(String, String, String), BTreeMap<String, (&'a Evidence, &'a Record)>>;
    let mut writers: WriterRows<'_> = BTreeMap::new();
    for e in request.evidence {
        if !covered(e, "ambiguity.scattered-state-writes") {
            continue;
        }
        for r in &e.records {
            if r.kind != Kind::FieldWrite || !current(e, r, request.model_ref, ir) {
                continue;
            }
            if let (Some(resource), Some(field), Some(op)) =
                (r.resource.known(), r.field.known(), r.operation.known())
            {
                let transition = request.transitions.is_some_and(|a| {
                    a.transitions().iter().any(|t| {
                        t.command().as_str() == op
                            && a.state_spaces().iter().any(|s| {
                                s.state_space_id() == t.state_space_id()
                                    && s.entity().as_str() == resource
                            })
                            && t.assignments().iter().any(|s| s.field().as_str() == field)
                    })
                });
                if !transition {
                    writers
                        .entry((e.target.clone(), resource.clone(), field.clone()))
                        .or_default()
                        .insert(op.clone(), (e, r));
                }
            }
        }
    }
    let mut total = 0;
    for ((target, resource, field), rows) in writers {
        let mut groups: BTreeSet<String> = rows.keys().cloned().collect();
        for g in &request.config.related_writer_groups {
            if g.resource == resource
                && g.field == field
                && g.operations.iter().all(|op| rows.contains_key(op))
            {
                for op in &g.operations {
                    groups.remove(op);
                }
                groups.insert(format!("group:{}", g.owner));
            }
        }
        total += groups.len() as u64;
        if groups.len() as u64 > *limit {
            let refs = rows
                .values()
                .map(|(e, r)| format!("{}#{}", hash(e), r.id))
                .take(32)
                .collect();
            let locations = rows
                .values()
                .flat_map(|(_, r)| r.locations.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .take(32)
                .collect();
            let subject = format!("{resource}/{field}");
            report.findings.push(make(
                "ambiguity.scattered-state-writes",
                &subject,
                State::Known(resource.clone()),
                &target,
                &request.scope,
                rows.values()
                    .map(|(_, r)| confidence(r))
                    .min()
                    .unwrap_or(Confidence::Unknown),
                Claim::Structural,
                &hash(&(
                    config,
                    &rows
                        .values()
                        .map(|(e, r)| (&e.sources, *r))
                        .collect::<Vec<_>>(),
                    &report.attachment_refs,
                    &resource,
                    &field,
                )),
                refs,
                locations,
                groups.into_iter().take(32).collect(),
                Vec::new(),
                p,
            ));
        }
    }
    report.summary.uncovered_writer_groups = if request
        .evidence
        .iter()
        .all(|e| covered(e, "ambiguity.scattered-state-writes"))
        && !request.evidence.is_empty()
    {
        State::Known(total)
    } else {
        State::Unknown
    };
}
fn defaults(request: &Request<'_>, report: &mut Report, p: &Profile, config: &str, ir: &str) {
    if !enabled(p, "ambiguity.implicit-target-defaults") {
        return;
    }
    let mut defaults: BTreeMap<(&str, &str), Vec<(&Evidence, &Record)>> = BTreeMap::new();
    for e in request.evidence {
        for r in &e.records {
            if r.kind == Kind::Default
                && current(e, r, request.model_ref, ir)
                && r.configuration == State::Known(false)
                && r.value.known().is_some()
            {
                if let Some(key) = r.key.known() {
                    defaults.entry((&r.subject, key)).or_default().push((e, r));
                }
            }
        }
    }
    for ((subject, key), rows) in defaults {
        let targets: Vec<_> = rows
            .iter()
            .map(|(e, _)| e.target.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if targets.len() < 2
            || rows
                .iter()
                .map(|(_, r)| r.value.known())
                .collect::<BTreeSet<_>>()
                .len()
                < 2
            || request
                .config
                .permitted_defaults
                .iter()
                .any(|d| d.subject == subject && d.key == key && d.targets == targets)
        {
            continue;
        }
        let (e, r) = rows[0];
        let mut f = from_record(
            "ambiguity.implicit-target-defaults",
            e,
            r,
            config,
            &request.scope,
            p,
        );
        f.confidence = rows
            .iter()
            .map(|(_, r)| confidence(r))
            .min()
            .unwrap_or(Confidence::Unknown);
        if f.confidence <= Confidence::Low {
            f.severity = "info".into();
        }
        f.target = "cross-target".into();
        f.subject = format!("{subject}/{key}");
        f.id = hash(&(&f.rule_id, &f.subject, &f.target, &f.semantic_symbol));
        f.condition_digest = hash(&(
            config,
            &rows
                .iter()
                .map(|(e, r)| (&e.target, &e.sources, &r.value))
                .collect::<Vec<_>>(),
        ));
        f.evidence = rows
            .iter()
            .map(|(e, r)| format!("{}#{}", hash(e), r.id))
            .take(32)
            .collect();
        report.findings.push(f);
    }
}
fn apply_waivers(request: &Request<'_>, report: &mut Report) {
    let Some(w) = request.waivers else {
        return;
    };
    for entry in &w.entries {
        let finding = report.findings.iter_mut().find(|f| {
            f.rule_id == entry.rule_id && f.subject == entry.subject && f.target == entry.target
        });
        let disposition = if entry
            .expires_on
            .known()
            .zip(request.as_of)
            .is_some_and(|(expires, now)| now > expires.as_str())
        {
            "expired"
        } else if let Some(f) = finding.as_ref() {
            if f.condition_digest != entry.condition_digest {
                "condition-changed"
            } else if entry.source_digest.known().is_some_and(|digest| {
                !request
                    .evidence
                    .iter()
                    .flat_map(|e| &e.sources)
                    .any(|s| &s.fingerprint == digest)
            }) {
                "source-changed"
            } else {
                "applied"
            }
        } else {
            "orphan"
        };
        let pin = finding
            .as_ref()
            .map_or(State::Unknown, |f| State::Known(f.id.clone()));
        if disposition == "applied" {
            if let Some(f) = finding {
                f.disposition = "waived".into();
                f.waiver = State::Known(entry.id.clone());
            }
        }
        report.waiver_audit.push(WaiverAudit {
            id: entry.id.clone(),
            disposition: disposition.into(),
            finding: pin,
        });
    }
    report.waiver_audit.sort_by(|a, b| a.id.cmp(&b.id));
}
pub fn render(report: &Report, config: &Config, check: bool) -> DomainResult {
    let p = config
        .profiles
        .iter()
        .find(|p| p.id == report.profile)
        .expect("resolved profile");
    let denied = check
        && (p.gate.fail_on_active_warnings
            && report.findings.iter().any(|f| {
                f.disposition == "active"
                    && f.severity == "warning"
                    && f.confidence >= p.gate.minimum_confidence
            })
            || p.gate.required_coverage.iter().any(|rule| {
                !report
                    .coverage
                    .iter()
                    .filter(|c| &c.rule == rule)
                    .any(|c| c.state == CoverageState::Complete)
                    || report
                        .coverage
                        .iter()
                        .filter(|c| &c.rule == rule && c.target != "model")
                        .any(|c| c.state != CoverageState::Complete)
            })
            || p.gate.require_comparable_baseline
                && report.comparison.known().map_or(true, |c| !c.comparable)
            || p.gate.fail_on_regression
                && report
                    .comparison
                    .known()
                    .map_or(true, |c| c.regression != State::Known(false)));
    let status = if denied {
        Status::Denied
    } else {
        Status::Valid
    };
    let mut diagnostics: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.disposition == "active")
        .take(252)
        .map(diagnostic::finding)
        .collect();
    if report
        .coverage
        .iter()
        .any(|c| !matches!(c.state, CoverageState::Complete | CoverageState::Disabled))
    {
        diagnostics.push(diagnostic::summary(
            "ai-lint.coverage-incomplete",
            "required-evidence-gaps",
        ));
    }
    if report.comparison.known().is_some_and(|c| !c.comparable)
        || check && p.gate.require_comparable_baseline && report.comparison.known().is_none()
    {
        diagnostics.push(diagnostic::summary(
            "ai-lint.baseline-incomparable",
            "baseline-recipe-or-coverage",
        ));
    }
    if denied {
        diagnostics.push(diagnostic::summary(
            "ai-lint.policy-denied",
            "selected-profile-gate",
        ));
    }
    let set = crate::diagnostics::DiagnosticSet::try_from_unsorted(diagnostics, status)
        .expect("registry bounded projection");
    let json = serde_json::json!({"status":status.as_str(),"report":report}).to_string();
    let human = format!(
        "AI lint {}: {} raw finding(s), {} active, {} waived\n{}",
        status.as_str(),
        report.summary.raw,
        report.summary.active,
        report.summary.waived,
        report
            .findings
            .iter()
            .map(|f| format!(
                "{} [{} {:?} {:?}] {} ({}) -> {}",
                f.code, f.disposition, f.confidence, f.claim, f.message, f.subject, f.alternative
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
    if denied {
        DomainResult::DeniedWithEvidence {
            diagnostics: set,
            json,
            human,
        }
    } else {
        DomainResult::graph(json, human, set.as_slice().to_vec())
    }
}
pub fn parse_report(bytes: &[u8]) -> Result<Report, DomainResult> {
    let r: Report = input::parse(bytes)?;
    compare::validate_report(&r)?;
    Ok(r)
}

pub fn resolve_symbol(compilation: &Compilation, symbol: &str) -> Result<String, DomainResult> {
    let selected = crate::inspect::selector::resolve(&compilation.project, symbol)
        .map_err(DomainResult::invalid)?;
    Ok(compilation.project.definitions[selected.definition]
        .id()
        .as_str()
        .to_owned())
}
