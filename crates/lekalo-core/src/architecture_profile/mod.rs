//! Versioned, opt-in architecture policy; Model semantics and source custody stay unchanged.
pub mod wire;
use crate::diagnostics::types::{token_value, DataObject};
use crate::{
    diagnostics::DiagnosticSet,
    result::{DomainResult, Status},
};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::BTreeSet;
pub use wire::*;

pub const CATALOG_BYTES: &[u8] =
    include_bytes!("../../../../contracts/architecture-rule-catalog.v0.6.5.json");
pub const PROFILES_BYTES: &[u8] =
    include_bytes!("../../../../contracts/architecture-profile.v0.6.5.json");
pub fn hash<T: Serialize>(value: &T) -> String {
    digest(
        &serde_json::to_vec(&serde_json::to_value(value).expect("typed wire"))
            .expect("canonical JSON"),
    )
}
pub fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::digest::sha256_hex(bytes))
}
pub fn header(family: &str) -> (String, String) {
    (
        format!("lekalo/{family}/v{VERSION}"),
        format!("dev.lekalo.{family}@{VERSION}"),
    )
}
pub fn failure(id: &str, detail: &str) -> DomainResult {
    let status = if id == "architecture-profile.version-unsupported" {
        Status::UnsupportedVersion
    } else {
        Status::Invalid
    };
    let set = diagnostic(id, detail, status);
    if status == Status::UnsupportedVersion {
        DomainResult::unsupported_version(set)
    } else {
        DomainResult::invalid(set)
    }
}
pub fn diagnostic(id: &str, detail: &str, status: Status) -> DiagnosticSet {
    let data = DataObject::from([("detail".into(), token_value(detail))]);
    let d = crate::diagnostics::normalize::build(id, None, None, data)
        .expect("registered architecture rule");
    DiagnosticSet::try_from_unsorted(vec![d], status).expect("architecture status")
}
fn invalid(detail: &str) -> DomainResult {
    failure("architecture-profile.input-invalid", detail)
}
fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_.:/@-".contains(&c))
}
fn pin(s: &str) -> bool {
    s.len() == 71
        && s.starts_with("sha256:")
        && s[7..]
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn sorted<T: Ord>(rows: &[T]) -> bool {
    rows.windows(2).all(|v| v[0] < v[1])
}
fn prose(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 2000 && !s.chars().any(char::is_control)
}
pub fn decode<T: DeserializeOwned>(bytes: &[u8], family: &str) -> Result<T, DomainResult> {
    if bytes.len() > MAX_BYTES {
        return Err(invalid("input-bound"));
    }
    // The existing strict frontend rejects duplicate keys before serde can erase them.
    let text = std::str::from_utf8(bytes).map_err(|_| invalid("utf8"))?;
    let index = crate::loader::source::LineIndex::new(text);
    crate::loader::frontends::json::parse(text, &index).map_err(|_| invalid("strict-json"))?;
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| invalid("json"))?;
    let (schema, identity) = header(family);
    if value.get("schemaVersion").and_then(|v| v.as_str()) != Some(&schema)
        || value.get("identity").and_then(|v| v.as_str()) != Some(&identity)
    {
        return Err(failure(
            "architecture-profile.version-unsupported",
            "contract-version",
        ));
    }
    serde_json::from_value(value).map_err(|_| invalid("closed-wire"))
}
pub fn catalog() -> Catalog {
    decode(CATALOG_BYTES, "architecture-rule-catalog").expect("embedded catalog")
}
pub fn catalog_ref() -> Reference {
    Reference {
        id: "architecture-core".into(),
        version: VERSION.into(),
        digest: digest(CATALOG_BYTES),
    }
}
pub fn embedded() -> Document {
    parse(PROFILES_BYTES).expect("embedded architecture profiles")
}
pub fn parse(bytes: &[u8]) -> Result<Document, DomainResult> {
    let d: Document = decode(bytes, "architecture-profile")?;
    if d.catalog_ref != catalog_ref()
        || d.profiles.is_empty()
        || d.profiles.len() > 256
        || d.assignments.len() > 256
        || !sorted(&d.profiles.iter().map(|p| &p.id).collect::<Vec<_>>())
    {
        return Err(invalid("catalog-profile-order"));
    }
    for p in &d.profiles {
        if !token(&p.id)
            || p.version != "1"
            || p.rules.len() > 256
            || !sorted(&p.rules.iter().map(|r| &r.id).collect::<Vec<_>>())
        {
            return Err(invalid("profile-invariant"));
        }
        if !matches!(p.extends, State::Known { .. } | State::Unknown) {
            return Err(invalid("parent-state"));
        }
    }
    if !sorted(
        &d.assignments
            .iter()
            .map(|a| (&a.scope.kind, &a.scope.id))
            .collect::<Vec<_>>(),
    ) {
        return Err(invalid("assignment-order"));
    }
    let mut scopes = BTreeSet::new();
    for a in &d.assignments {
        if !["project", "module"].contains(&a.scope.kind.as_str())
            || !token(&a.scope.id)
            || !scopes.insert((&a.scope.kind, &a.scope.id))
        {
            return Err(invalid("scope-grammar"));
        }
        let r = resolve_admitted(&d, &a.profile_ref.id)?;
        if r.profile_ref != a.profile_ref {
            return Err(invalid("assignment-pin"));
        }
    }
    for p in &d.profiles {
        resolve_admitted(&d, &p.id)?;
    }
    Ok(d)
}
fn selection_valid(s: &Selection, rule: &Rule) -> bool {
    s.id == rule.id
        && (!rule.mandatory || (s.enabled && s.required && s.severity == rule.severity))
        && s.severity <= rule.severity
        && (!s.required
            || (s.enabled && matches!(rule.blocking_basis.as_str(), "semantic" | "measured")))
        && matches!(s.limit, State::Unknown | State::Known { .. })
        && s.limit.value().map_or(true, |l| {
            rule.blocking_basis == "measured"
                && s.enabled
                && l.maximum <= 10_000_000
                && token(&l.calibration_ref)
        })
}
fn weaker(base: &Selection, next: &Selection) -> bool {
    (base.enabled && !next.enabled)
        || (base.required && !next.required)
        || next.severity < base.severity
        || match (&base.limit, &next.limit) {
            (State::Known { value: a }, State::Known { value: b }) => b.maximum > a.maximum,
            (State::Known { .. }, _) => true,
            _ => false,
        }
}
fn admit(d: &Document) -> Result<Document, DomainResult> {
    parse(&serde_json::to_vec(d).map_err(|_| invalid("typed-document"))?)
}
pub fn resolve(d: &Document, id: &str) -> Result<Resolved, DomainResult> {
    resolve_admitted(&admit(d)?, id)
}
fn resolve_admitted(d: &Document, id: &str) -> Result<Resolved, DomainResult> {
    fn walk(
        d: &Document,
        id: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Result<(Vec<Reference>, Vec<Selection>), DomainResult> {
        if visiting.len() >= 8 || !visiting.insert(id.into()) {
            return Err(failure(
                "architecture-profile.inheritance-invalid",
                "cycle-or-depth",
            ));
        }
        let p =
            d.profiles.iter().find(|p| p.id == id).ok_or_else(|| {
                failure("architecture-profile.inheritance-invalid", "parent-missing")
            })?;
        let (mut chain, mut rules) = if let Some(parent) = p.extends.value() {
            let (chain, rules) = walk(d, &parent.id, visiting)?;
            if chain.last() != Some(parent) {
                return Err(invalid("parent-pin"));
            }
            (chain, rules)
        } else {
            (vec![], vec![])
        };
        let c = catalog();
        for s in &p.rules {
            let rule = c
                .rules
                .iter()
                .find(|r| r.id == s.id)
                .ok_or_else(|| invalid("unknown-rule"))?;
            if !selection_valid(s, rule) {
                return Err(invalid("rule-severity-evidence-limit"));
            }
            if let Some(old) = rules.iter_mut().find(|v| v.id == s.id) {
                if weaker(old, s) {
                    return Err(failure(
                        "architecture-profile.weakening-unacknowledged",
                        "inherited-rule",
                    ));
                }
                *old = s.clone();
            } else {
                rules.push(s.clone());
            }
        }
        rules.sort_by(|a, b| a.id.cmp(&b.id));
        if rules.len() != c.rules.len() {
            return Err(invalid("root-rule-completeness"));
        }
        let reference = Reference {
            id: p.id.clone(),
            version: p.version.clone(),
            digest: hash(&rules),
        };
        chain.push(reference);
        visiting.remove(id);
        Ok((chain, rules))
    }
    let (chain, rules) = walk(d, id, &mut BTreeSet::new())?;
    let (schema_version, identity) = header("architecture-profile-resolved");
    Ok(Resolved {
        schema_version,
        identity,
        profile_ref: chain.last().expect("nonempty chain").clone(),
        source_digest: hash(d),
        catalog_ref: catalog_ref(),
        chain,
        rules,
    })
}
pub fn snapshot(d: &Document) -> Result<Snapshot, DomainResult> {
    let admitted = admit(d)?;
    let d = &admitted;
    let profiles = d
        .profiles
        .iter()
        .map(|p| resolve_admitted(d, &p.id))
        .collect::<Result<_, _>>()?;
    let (schema_version, identity) = header("architecture-profile-lock");
    Ok(Snapshot {
        schema_version,
        identity,
        document_digest: hash(d),
        catalog_ref: catalog_ref(),
        profiles,
        assignments: d.assignments.clone(),
    })
}
pub fn verify_lock(bytes: &[u8], d: &Document) -> Result<(), DomainResult> {
    let stored: Snapshot = decode(bytes, "architecture-profile-lock")?;
    if stored != snapshot(d)? {
        return Err(invalid("lock-content-pin"));
    }
    Ok(())
}
pub fn policy_diff(base: &Document, candidate: &Document) -> Result<Diff, DomainResult> {
    let base_document = admit(base)?;
    let candidate_document = admit(candidate)?;
    let base = &base_document;
    let candidate = &candidate_document;
    let mut changes = vec![];
    let ids = base
        .profiles
        .iter()
        .chain(&candidate.profiles)
        .map(|p| p.id.clone())
        .collect::<BTreeSet<_>>();
    for id in ids {
        let a = base
            .profiles
            .iter()
            .find(|p| p.id == id)
            .map(|_| resolve_admitted(base, &id))
            .transpose()?;
        let b = candidate
            .profiles
            .iter()
            .find(|p| p.id == id)
            .map(|_| resolve_admitted(candidate, &id))
            .transpose()?;
        if let (Some(a), Some(b)) = (&a, &b) {
            for (old, new) in a.rules.iter().zip(&b.rules) {
                for (member, before, after, strength) in [
                    (
                        "enabled",
                        old.enabled.to_string(),
                        new.enabled.to_string(),
                        if old.enabled {
                            "weakened"
                        } else {
                            "strengthened"
                        },
                    ),
                    (
                        "severity",
                        format!("{:?}", old.severity),
                        format!("{:?}", new.severity),
                        if new.severity < old.severity {
                            "weakened"
                        } else {
                            "strengthened"
                        },
                    ),
                    (
                        "required",
                        old.required.to_string(),
                        new.required.to_string(),
                        if old.required {
                            "weakened"
                        } else {
                            "strengthened"
                        },
                    ),
                    (
                        "limit",
                        serde_json::to_string(&old.limit).unwrap(),
                        serde_json::to_string(&new.limit).unwrap(),
                        match (old.limit.value(), new.limit.value()) {
                            (Some(a), Some(b)) if b.maximum > a.maximum => "weakened",
                            (Some(a), Some(b)) if b.maximum < a.maximum => "strengthened",
                            (Some(_), None) => "weakened",
                            (None, Some(_)) => "strengthened",
                            _ => "incomparable",
                        },
                    ),
                ] {
                    if before != after {
                        changes.push(Change {
                            scope: id.clone(),
                            rule_id: old.id.clone(),
                            member: member.into(),
                            strength: strength.into(),
                            before,
                            after,
                        });
                    }
                }
            }
        } else {
            changes.push(Change {
                scope: id,
                rule_id: "profile".into(),
                member: "membership".into(),
                strength: "incomparable".into(),
                before: hash(&a),
                after: hash(&b),
            });
        }
    }
    if base.assignments != candidate.assignments {
        changes.push(Change {
            scope: "project".into(),
            rule_id: "assignment".into(),
            member: "assignments".into(),
            strength: "incomparable".into(),
            before: hash(&base.assignments),
            after: hash(&candidate.assignments),
        });
    }
    if hash(base) != hash(candidate) {
        changes.push(Change {
            scope: "project".into(),
            rule_id: "provenance".into(),
            member: "document".into(),
            strength: "incomparable".into(),
            before: hash(base),
            after: hash(candidate),
        });
    }
    let (schema_version, identity) = header("architecture-profile-diff");
    Ok(Diff {
        schema_version,
        identity,
        base_ref: hash(base),
        candidate_ref: hash(candidate),
        comparable: true,
        changes,
    })
}

pub struct Assessment<'a> {
    pub document: &'a Document,
    pub profile: &'a str,
    pub compilation: &'a crate::ir::Compilation,
    pub model_ref: String,
    pub module: Option<String>,
    pub check: bool,
    pub baseline: Option<&'a Report>,
    pub adoption: Option<&'a Adoption>,
    pub as_of: Option<&'a str>,
}
pub fn assess(a: Assessment<'_>) -> Result<Report, DomainResult> {
    let document = admit(a.document)?;
    if !pin(&a.model_ref) {
        return Err(invalid("model-pin"));
    }
    let semantic = crate::validator::ValidationProfile::embedded_default()
        .map_err(|_| invalid("semantic-profile"))?;
    crate::validator::validate(a.compilation, semantic).map_err(DomainResult::invalid)?;
    let project = &a.compilation.project;
    let project_id = project
        .project
        .as_ref()
        .map(|p| p.id.as_str())
        .unwrap_or("anonymous");
    if let Some(module) = &a.module {
        if !project.modules.iter().any(|m| m.id.as_str() == module) {
            return Err(invalid("module-unknown"));
        }
    }
    for assignment in &document.assignments {
        if (assignment.scope.kind == "project" && assignment.scope.id != project_id)
            || (assignment.scope.kind == "module"
                && !project
                    .modules
                    .iter()
                    .any(|m| m.id.as_str() == assignment.scope.id))
        {
            return Err(invalid("assignment-subject-unknown"));
        }
        if assignment.scope.kind == "project" && assignment.profile_ref.id != a.profile {
            return Err(invalid("selection-conflicts-with-assignment"));
        }
    }
    let base = resolve_admitted(&document, a.profile)?;
    let snap = snapshot(&document)?;
    let coupling = crate::coupling::analyze(
        &crate::coupling::Request {
            selection: crate::coupling::Selection::All,
            field: None,
            context_budget: None,
            source_revision: None,
            model_digest: Some(a.model_ref.clone()),
        },
        &crate::coupling::Profile::default(),
        a.compilation,
        None,
    )
    .map_err(DomainResult::invalid)?;
    let c = catalog();
    let mut rows = vec![];
    let mut profiles = vec![base.clone()];
    for module in &project.modules {
        if a.module.as_deref().is_some_and(|m| m != module.id.as_str()) {
            continue;
        }
        let selected = if let Some(x) = document
            .assignments
            .iter()
            .find(|x| x.scope.kind == "module" && x.scope.id == module.id.as_str())
        {
            resolve_admitted(&document, &x.profile_ref.id)?
        } else {
            base.clone()
        };
        if base
            .rules
            .iter()
            .zip(&selected.rules)
            .any(|(old, new)| weaker(old, new))
        {
            return Err(failure(
                "architecture-profile.weakening-unacknowledged",
                "module-assignment",
            ));
        }
        if !profiles
            .iter()
            .any(|p| p.profile_ref == selected.profile_ref)
        {
            profiles.push(selected.clone());
        }
        let definitions = project
            .definitions
            .iter()
            .filter(|d| d.id().as_str().split('.').next() == Some(module.id.as_str()))
            .collect::<Vec<_>>();
        let budget = crate::context_budget::plan(
            &crate::context_budget::BudgetRequest::new(
                None,
                Some(module.id.as_str().into()),
                false,
                false,
                false,
                false,
            )
            .map_err(DomainResult::invalid)?,
            &crate::context_budget::BudgetSelection::Generic(
                crate::context_budget::version::MAX_BUDGET_TOKENS,
            ),
            a.compilation,
        )
        .map_err(DomainResult::invalid)?;
        for (r, s) in c.rules.iter().zip(&selected.rules) {
            let mut coverage = if s.enabled {
                r.coverage.clone()
            } else {
                Coverage::Disabled
            };
            let mut value = State::Unsupported;
            let mut witnesses = vec![];
            if s.enabled {
                match r.measurement.as_str() {
                    "declaredSemanticIds" => {
                        value = State::known(definitions.len() as u64);
                        witnesses = definitions
                            .iter()
                            .map(|d| d.id().as_str().to_owned())
                            .collect();
                    }
                    "fanOutModules" => {
                        value = State::known(module.imports.len() as u64);
                        witnesses = module.imports.iter().map(|m| m.as_str().into()).collect();
                    }
                    "minimumRequiredSemanticTokens" => {
                        let values = budget
                            .subjects
                            .iter()
                            .map(|s| &s.metrics.minimum_required_semantic_tokens)
                            .collect::<Vec<_>>();
                        if budget.complete
                            && values
                                .iter()
                                .all(|v| matches!(v, crate::context_budget::StateValue::Known(_)))
                        {
                            value = State::known(
                                values
                                    .iter()
                                    .filter_map(|v| {
                                        if let crate::context_budget::StateValue::Known(n) = v {
                                            Some(*n)
                                        } else {
                                            None
                                        }
                                    })
                                    .max()
                                    .unwrap_or(0),
                            );
                        } else {
                            coverage = Coverage::Partial;
                            value = State::Unknown;
                        }
                        witnesses = budget.subjects.iter().map(|s| s.id.clone()).collect();
                    }
                    "publicOperationsWithoutScenario" => {
                        let covered = project
                            .definitions
                            .iter()
                            .filter_map(|d| {
                                if let crate::ir::Definition::Scenario(x) = d {
                                    Some(&x.covers)
                                } else {
                                    None
                                }
                            })
                            .flatten()
                            .map(|id| id.as_str())
                            .collect::<BTreeSet<_>>();
                        witnesses = definitions
                            .iter()
                            .filter(|d| {
                                matches!(
                                    d,
                                    crate::ir::Definition::Command(_)
                                        | crate::ir::Definition::Query(_)
                                ) && d.common().visibility != Some(crate::ir::Visibility::Module)
                                    && !covered.contains(d.id().as_str())
                            })
                            .map(|d| d.id().as_str().into())
                            .collect();
                        value = State::known(witnesses.len() as u64);
                    }
                    "sharedAbstractionRadius" => {
                        let max = coupling
                            .subjects
                            .iter()
                            .filter(|s| s.module == module.id.as_str())
                            .filter_map(|s| {
                                s.metrics
                                    .get("sharedAbstractionRadius")
                                    .and_then(|v| v.value())
                                    .copied()
                            })
                            .max();
                        value = max.map_or(State::Unknown, State::known);
                    }
                    _ => {}
                }
            }
            witnesses.sort();
            witnesses.dedup();
            if witnesses.len() > 10_000 {
                return Err(invalid("witness-bound"));
            }
            let exceeded = coverage == Coverage::Complete
                && value
                    .value()
                    .zip(s.limit.value())
                    .is_some_and(|(n, l)| *n > l.maximum);
            let disposition = if !s.enabled {
                "disabled"
            } else if s.required && coverage != Coverage::Complete {
                "evidence-gap"
            } else if exceeded {
                "violation"
            } else {
                "observed"
            };
            let condition_digest = hash(&(
                r.id.as_str(),
                module.id.as_str(),
                &coverage,
                &value,
                &witnesses,
                &s.limit,
            ));
            rows.push(Row {
                rule_id: r.id.clone(),
                module: module.id.as_str().into(),
                producer: r.producer.clone(),
                measurement: r.measurement.clone(),
                coverage,
                value,
                witnesses,
                severity: s.severity,
                required: s.required,
                limit: s.limit.clone(),
                disposition: disposition.into(),
                condition_digest,
                rationale: r.rationale.clone(),
                alternative: r.alternative.clone(),
            });
        }
    }
    rows.sort_by(|x, y| (&x.module, &x.rule_id).cmp(&(&y.module, &y.rule_id)));
    profiles.sort_by(|x, y| x.profile_ref.id.cmp(&y.profile_ref.id));
    let (schema_version, identity) = header("architecture-profile-report");
    let mut r = Report {
        schema_version,
        identity,
        recipe: RECIPE.into(),
        scope: Scope {
            kind: if a.module.is_some() {
                "module"
            } else {
                "project"
            }
            .into(),
            id: a.module.unwrap_or_else(|| project_id.into()),
        },
        document_digest: hash(&document),
        model_ref: a.model_ref,
        ir_ref: digest(project.to_canonical_json().as_bytes()),
        snapshot_ref: hash(&snap),
        profiles,
        rows,
        baseline_ref: a.baseline.map_or(State::Unknown, |b| State::known(hash(b))),
        assessment: "advisory".into(),
    };
    if let Some(baseline) = a.baseline {
        validate_report(baseline)?;
        if baseline.snapshot_ref != r.snapshot_ref
            || baseline.document_digest != r.document_digest
            || baseline.scope != r.scope
            || baseline.profiles != r.profiles
            || baseline
                .rows
                .iter()
                .map(|v| (&v.module, &v.rule_id))
                .collect::<Vec<_>>()
                != r.rows
                    .iter()
                    .map(|v| (&v.module, &v.rule_id))
                    .collect::<Vec<_>>()
        {
            return Err(failure(
                "architecture-profile.baseline-incomparable",
                "scope-policy-inventory",
            ));
        }
        // Row selections must agree with the admitted policy, never caller-forged limits.
        for row in &baseline.rows {
            let current = r
                .rows
                .iter()
                .find(|v| v.module == row.module && v.rule_id == row.rule_id)
                .expect("equal inventory");
            if row.severity != current.severity
                || row.required != current.required
                || row.limit != current.limit
                || (row.coverage == Coverage::Disabled) != (current.coverage == Coverage::Disabled)
            {
                return Err(failure(
                    "architecture-profile.baseline-incomparable",
                    "row-policy",
                ));
            }
        }
    }
    if let Some(ledger) = a.adoption {
        let (s, i) = header("architecture-adoption");
        if ledger.schema_version != s || ledger.identity != i {
            return Err(failure(
                "architecture-profile.version-unsupported",
                "adoption-version",
            ));
        }
        let baseline = a
            .baseline
            .ok_or_else(|| invalid("adoption-baseline-required"))?;
        validate_report(baseline)?;
        if ledger.snapshot_ref != r.snapshot_ref
            || ledger.baseline_ref != hash(baseline)
            || ledger.entries.len() > 10_000
            || baseline.snapshot_ref != r.snapshot_ref
            || baseline.scope != r.scope
        {
            return Err(failure(
                "architecture-profile.adoption-invalid",
                "ledger-pins",
            ));
        }
        let date = a
            .as_of
            .filter(|v| crate::ai_lint::input::date(v))
            .ok_or_else(|| failure("architecture-profile.adoption-invalid", "as-of-required"))?;
        let mut keys = BTreeSet::new();
        for e in &ledger.entries {
            if !token(&e.owner)
                || !prose(&e.reason)
                || !token(&e.review_ref)
                || !crate::ai_lint::input::date(&e.expires_on)
                || !keys.insert((&e.module, &e.rule_id))
            {
                return Err(failure(
                    "architecture-profile.adoption-invalid",
                    "debt-invariant",
                ));
            }
            let prior = baseline
                .rows
                .iter()
                .find(|v| {
                    v.rule_id == e.rule_id
                        && v.module == e.module
                        && v.disposition == "violation"
                        && v.coverage == Coverage::Complete
                        && v.condition_digest == e.condition_digest
                })
                .ok_or_else(|| {
                    failure(
                        "architecture-profile.adoption-invalid",
                        "debt-not-qualified",
                    )
                })?;
            if c.rules.iter().any(|v| v.id == e.rule_id && v.mandatory) {
                return Err(failure(
                    "architecture-profile.adoption-invalid",
                    "mandatory-debt",
                ));
            }
            if date <= e.expires_on.as_str() {
                if let Some(row) = r.rows.iter_mut().find(|v| {
                    v.module == prior.module
                        && v.rule_id == prior.rule_id
                        && v.disposition == "violation"
                        && v.condition_digest == e.condition_digest
                }) {
                    row.disposition = "adopted".into();
                }
            }
        }
    }
    let denied = r
        .rows
        .iter()
        .any(|row| matches!(row.disposition.as_str(), "evidence-gap" | "violation"));
    r.assessment = if a.check && denied {
        "denied"
    } else if a.check && r.rows.iter().any(|v| v.disposition == "adopted") {
        "adopting"
    } else if a.check {
        "enforced-core"
    } else {
        "advisory"
    }
    .into();
    validate_report(&r)?;
    Ok(r)
}
pub fn validate_report(r: &Report) -> Result<(), DomainResult> {
    let (schema, identity) = header("architecture-profile-report");
    if r.schema_version != schema
        || r.identity != identity
        || r.recipe != RECIPE
        || !pin(&r.model_ref)
        || !pin(&r.ir_ref)
        || !pin(&r.snapshot_ref)
        || !pin(&r.document_digest)
        || r.rows.len() > 10_000
        || !sorted(
            &r.rows
                .iter()
                .map(|x| (&x.module, &x.rule_id))
                .collect::<Vec<_>>(),
        )
        || !["project", "module"].contains(&r.scope.kind.as_str())
        || !token(&r.scope.id)
        || !matches!(
            r.assessment.as_str(),
            "advisory" | "denied" | "adopting" | "enforced-core"
        )
        || !matches!(&r.baseline_ref, State::Unknown | State::Known { .. })
        || r.baseline_ref.value().is_some_and(|v| !pin(v))
    {
        return Err(invalid("report-invariant"));
    }
    let c = catalog();
    let modules = r.rows.iter().map(|v| &v.module).collect::<BTreeSet<_>>();
    if r.rows.len() != modules.len() * c.rules.len()
        || (r.scope.kind == "module" && (modules.len() != 1 || !modules.contains(&r.scope.id)))
    {
        return Err(invalid("report-inventory"));
    }
    if r.profiles.is_empty()
        || r.profiles.len() > 256
        || !sorted(
            &r.profiles
                .iter()
                .map(|p| &p.profile_ref.id)
                .collect::<Vec<_>>(),
        )
    {
        return Err(invalid("report-profiles"));
    }
    for p in &r.profiles {
        let (s, i) = header("architecture-profile-resolved");
        if p.schema_version != s
            || p.identity != i
            || p.source_digest != r.document_digest
            || p.catalog_ref != catalog_ref()
            || p.profile_ref.digest != hash(&p.rules)
            || p.chain.is_empty()
            || p.chain.len() > 8
            || p.chain.last() != Some(&p.profile_ref)
            || p.rules.len() != c.rules.len()
            || !sorted(&p.rules.iter().map(|v| &v.id).collect::<Vec<_>>())
            || p.rules
                .iter()
                .zip(&c.rules)
                .any(|(v, r)| !selection_valid(v, r))
        {
            return Err(invalid("report-profile"));
        }
    }
    for row in &r.rows {
        let rule = c
            .rules
            .iter()
            .find(|v| v.id == row.rule_id)
            .ok_or_else(|| invalid("report-rule"))?;
        if row.measurement != rule.measurement
            || row.producer != rule.producer
            || row.rationale != rule.rationale
            || row.alternative != rule.alternative
            || row.condition_digest
                != hash(&(
                    row.rule_id.as_str(),
                    row.module.as_str(),
                    &row.coverage,
                    &row.value,
                    &row.witnesses,
                    &row.limit,
                ))
            || !token(&row.module)
            || row.witnesses.len() > 10_000
            || !sorted(&row.witnesses)
            || row.witnesses.iter().any(|v| !token(v))
        {
            return Err(invalid("report-row"));
        }
        let disabled = row.coverage == Coverage::Disabled;
        if !selection_valid(
            &Selection {
                id: row.rule_id.clone(),
                enabled: !disabled,
                severity: row.severity,
                required: row.required,
                limit: row.limit.clone(),
            },
            rule,
        ) || (!disabled
            && row.coverage != rule.coverage
            && !(rule.measurement == "minimumRequiredSemanticTokens"
                && row.coverage == Coverage::Partial))
            || (disabled && row.value != State::Unsupported)
            || (row.coverage == Coverage::Unsupported && row.value != State::Unsupported)
            || (row.coverage == Coverage::Complete && row.value.value().is_none())
            || row
                .value
                .value()
                .is_some_and(|v| *v > 9_007_199_254_740_991)
            || ([
                "declaredSemanticIds",
                "fanOutModules",
                "publicOperationsWithoutScenario",
            ]
            .contains(&row.measurement.as_str())
                && !disabled
                && row.value.value() != Some(&(row.witnesses.len() as u64)))
        {
            return Err(invalid("report-coverage-selection"));
        }
        let expected = if disabled {
            "disabled"
        } else if row.required && row.coverage != Coverage::Complete {
            "evidence-gap"
        } else if row.coverage == Coverage::Complete
            && row
                .value
                .value()
                .zip(row.limit.value())
                .is_some_and(|(v, l)| *v > l.maximum)
        {
            "violation"
        } else {
            "observed"
        };
        if row.disposition != expected
            && !(row.disposition == "adopted"
                && expected == "violation"
                && r.baseline_ref.value().is_some())
        {
            return Err(invalid("report-disposition"));
        }
    }
    let bad = r
        .rows
        .iter()
        .any(|v| matches!(v.disposition.as_str(), "evidence-gap" | "violation"));
    let adopted = r.rows.iter().any(|v| v.disposition == "adopted");
    if (r.assessment == "denied" && !bad)
        || (r.assessment == "enforced-core" && (bad || adopted))
        || (r.assessment == "adopting" && (bad || !adopted))
    {
        return Err(invalid("report-assessment"));
    }
    Ok(())
}
pub fn render<T: Serialize>(value: &T, denied: bool) -> DomainResult {
    let json =
        serde_json::json!({"status":if denied{"denied"}else{"valid"},"architectureProfile":value})
            .to_string();
    let human = serde_json::to_string_pretty(value).expect("typed output");
    if denied {
        DomainResult::denied_json(
            json,
            human,
            diagnostic(
                "architecture-profile.policy-denied",
                "selected-obligations",
                Status::Denied,
            ),
        )
    } else {
        DomainResult::validation(json, human, vec![])
    }
}
pub fn render_report(report: &Report) -> DomainResult {
    if report.assessment == "denied" {
        return render(report, true);
    }
    let diagnostics = if report
        .rows
        .iter()
        .any(|v| matches!(v.coverage, Coverage::Partial | Coverage::Unsupported))
    {
        diagnostic(
            "architecture-profile.evidence-incomplete",
            "core-coverage",
            Status::Valid,
        )
        .as_slice()
        .to_vec()
    } else {
        vec![]
    };
    DomainResult::validation(
        serde_json::json!({"status":"valid","architectureProfile":report}).to_string(),
        serde_json::to_string_pretty(report).expect("typed report"),
        diagnostics,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_callers_cannot_bypass_document_admission() {
        let mut d = embedded();
        d.catalog_ref.digest = hash(&"foreign");
        assert!(resolve(&d, "ai-strict").is_err());
        assert!(snapshot(&d).is_err());
        assert!(policy_diff(&embedded(), &d).is_err());
        let mut d = embedded();
        d.assignments.push(Assignment {
            scope: Scope {
                kind: "path".into(),
                id: "planner".into(),
            },
            profile_ref: resolve(&embedded(), "ai-strict").unwrap().profile_ref,
        });
        assert!(resolve(&d, "ai-strict").is_err());
    }
    #[test]
    fn catalog_and_resolved_core_boundaries() {
        let d = embedded();
        let c = catalog();
        assert_eq!(c.rules.len(), 15);
        assert_eq!(
            c.registry_ref.digest,
            digest(crate::diagnostics::registry::SUCCESSOR_REGISTRY_BYTES)
        );
        let strict = resolve(&d, "ai-strict").unwrap();
        assert_eq!(
            strict
                .chain
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["legacy-observed", "contracted-standard", "ai-strict"]
        );
        assert_eq!(strict.rules.iter().filter(|r| r.required).count(), 3);
        for rule in c.rules.iter().filter(|r| r.blocking_basis == "advisory") {
            assert!(strict
                .rules
                .iter()
                .find(|r| r.id == rule.id)
                .unwrap()
                .limit
                .value()
                .is_none());
        }
    }
    #[test]
    fn advisory_rules_cannot_be_required_by_documents_or_typed_callers() {
        let d = embedded();
        let standard = resolve(&d, "contracted-standard").unwrap();
        for rule in catalog()
            .rules
            .iter()
            .filter(|r| r.blocking_basis == "advisory")
        {
            let mut document = d.clone();
            let mut selection = standard
                .rules
                .iter()
                .find(|s| s.id == rule.id)
                .unwrap()
                .clone();
            selection.required = true;
            document.profiles.push(Profile {
                id: "required-advisory".into(),
                version: "1".into(),
                extends: State::known(standard.profile_ref.clone()),
                rules: vec![selection],
            });
            document.profiles.sort_by(|a, b| a.id.cmp(&b.id));
            assert!(
                parse(&serde_json::to_vec(&document).unwrap()).is_err(),
                "{}",
                rule.id
            );
            assert!(
                resolve(&document, "required-advisory").is_err(),
                "{}",
                rule.id
            );
            assert!(snapshot(&document).is_err(), "{}", rule.id);
            assert!(policy_diff(&d, &document).is_err(), "{}", rule.id);
        }
        let managed = resolve(&d, "managed-generated").unwrap();
        let generation = catalog()
            .rules
            .into_iter()
            .find(|r| r.id == "architecture.deterministic-generation")
            .unwrap();
        assert_eq!(generation.blocking_basis, "semantic");
        assert_eq!(generation.coverage, Coverage::Unsupported);
        assert!(
            managed
                .rules
                .iter()
                .find(|r| r.id == generation.id)
                .unwrap()
                .required
        );
    }

    #[test]
    fn multibyte_json_errors_are_structured_and_legal_strings_still_decode() {
        for text in ["\u{e9}", "{\"x\":\u{e9}}", "\u{feff}{}"] {
            let result = parse(text.as_bytes()).unwrap_err();
            assert_eq!(result.status(), Status::Invalid);
            assert_eq!(result.diagnostics().len(), 1);
            assert_eq!(
                result.diagnostics()[0].id(),
                "architecture-profile.input-invalid"
            );
        }
        let text = format!(
            "{{\"schemaVersion\":\"{}\",\"identity\":\"{}\",\"text\":\"\\u00e9\u{6f22}\u{1f600}\"}}",
            header("unicode-control").0,
            header("unicode-control").1
        );
        let value: serde_json::Value = decode(text.as_bytes(), "unicode-control").unwrap();
        assert_eq!(value["text"], "\u{e9}\u{6f22}\u{1f600}");
    }
    #[test]
    fn error_selection_and_every_inherited_weakening_dimension_fail_closed() {
        for member in ["enabled", "required", "severity", "limit"] {
            let mut d = embedded();
            let standard = d
                .profiles
                .iter_mut()
                .find(|p| p.id == "contracted-standard")
                .unwrap();
            standard.rules.push(Selection {
                id: "architecture.stable-semantic-ids".into(),
                enabled: member != "enabled",
                required: member != "required",
                severity: if member == "severity" {
                    Severity::Info
                } else {
                    Severity::Error
                },
                limit: if member == "limit" {
                    State::known(Limit {
                        maximum: 0,
                        calibration_ref: "fixture/1".into(),
                    })
                } else {
                    State::Unknown
                },
            });
            standard.rules.sort_by(|a, b| a.id.cmp(&b.id));
            assert!(parse(&serde_json::to_vec(&d).unwrap()).is_err(), "{member}");
        }
        let base = Selection {
            id: "architecture.bounded-context".into(),
            enabled: true,
            required: true,
            severity: Severity::Warning,
            limit: State::known(Limit {
                maximum: 10,
                calibration_ref: "fixture/1".into(),
            }),
        };
        let mut child = base.clone();
        child.limit = State::known(Limit {
            maximum: 11,
            calibration_ref: "fixture/1".into(),
        });
        assert!(weaker(&base, &child));
        child.limit = State::Unknown;
        assert!(weaker(&base, &child));
        child = base.clone();
        child.limit = State::known(Limit {
            maximum: 9,
            calibration_ref: "fixture/1".into(),
        });
        assert!(!weaker(&base, &child));
    }
    #[test]
    fn duplicate_keys_nested_members_and_parent_cycles_are_not_erased() {
        let bytes = String::from_utf8(PROFILES_BYTES.to_vec()).unwrap();
        let duplicate = bytes.replacen(
            "\"assignments\": []",
            "\"assignments\": [], \"assignments\": []",
            1,
        );
        assert!(parse(duplicate.as_bytes()).is_err());
        let mut d = serde_json::to_value(embedded()).unwrap();
        d["profiles"][0]["rules"][0]["extension"] = true.into();
        assert!(parse(&serde_json::to_vec(&d).unwrap()).is_err());
        let mut d = embedded();
        d.profiles
            .iter_mut()
            .find(|p| p.id == "legacy-observed")
            .unwrap()
            .extends = State::known(Reference {
            id: "ai-strict".into(),
            version: "1".into(),
            digest: hash(&0),
        });
        assert!(parse(&serde_json::to_vec(&d).unwrap()).is_err());
    }
    #[test]
    fn successor_is_an_exact_union_and_old_producer_diagnostics_keep_their_pin() {
        let old = crate::diagnostics::DiagnosticRegistry::embedded().unwrap();
        let new = crate::diagnostics::DiagnosticRegistry::successor().unwrap();
        assert_eq!(new.len(), old.len() + 8);
        for entry in old.entries() {
            assert_eq!(new.entry(entry.id()), Some(entry));
        }
        let d = crate::diagnostics::normalize::build("cli.usage", None, None, DataObject::new())
            .unwrap();
        assert_eq!(d.registry_version, "0.6.4");
        let set = diagnostic(
            "architecture-profile.policy-denied",
            "fixture",
            Status::Denied,
        );
        assert_eq!(set.as_slice()[0].registry_version, "0.6.5");
    }
}
