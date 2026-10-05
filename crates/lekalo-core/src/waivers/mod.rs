//! Scoped, expiring governance over immutable facts. No IO, clocks or execution.
mod json;
pub mod lint;
pub mod policy;
pub mod store;
pub mod wire;
use crate::ai_lint::input::{self, hash, is_digest, token};
use crate::{diagnostics::DiagnosticSet, result::Status, DomainResult};
use policy::ProfileState;
use std::collections::{BTreeMap, BTreeSet};
pub use wire::*;
pub const VERSION: &str = "0.6.5";
pub const MAX_ENTRIES: usize = 10_000;
pub const MAX_WINDOW: u64 = 31_536_000;
#[cfg(test)]
mod tests;

pub fn failure(detail: &str) -> DomainResult {
    crate::ai_lint::diagnostic::failure("ai-lint.waiver-invalid", detail)
}
pub fn canonical<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut b = input::canonical(value);
    b.push(b'\n');
    b
}
pub fn parse<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, DomainResult> {
    if bytes.len() > input::MAX_BYTES {
        return Err(failure("waivers-input-limit"));
    }
    let value = json::parse(bytes).map_err(|_| failure("waivers-closed-json"))?;
    serde_json::from_value(value).map_err(|_| failure("waivers-closed-shape"))
}
pub fn timestamp(s: &str) -> Option<i64> {
    if s.len() != 20
        || !s.is_ascii()
        || !input::date(&s[..10])
        || s.as_bytes()[10] != b'T'
        || s.as_bytes()[13] != b':'
        || s.as_bytes()[16] != b':'
        || s.as_bytes()[19] != b'Z'
    {
        return None;
    }
    let read = |a, b| s.get(a..b)?.parse::<i64>().ok();
    if ![&s[11..13], &s[14..16], &s[17..19]]
        .iter()
        .all(|v| v.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let (y, m, d, h, n, se) = (
        read(0, 4)?,
        read(5, 7)?,
        read(8, 10)?,
        read(11, 13)?,
        read(14, 16)?,
        read(17, 19)?,
    );
    if h > 23 || n > 59 || se > 59 || h < 0 || n < 0 || se < 0 {
        return None;
    }
    Some(crate::expressions::types::days_from_civil(y, m, d) * 86400 + h * 3600 + n * 60 + se)
}
pub fn evaluation_time(s: &str) -> Result<String, DomainResult> {
    let v = if input::date(s) {
        format!("{s}T23:59:59Z")
    } else {
        s.to_owned()
    };
    timestamp(&v).ok_or_else(|| failure("waivers-as-of"))?;
    Ok(v)
}
fn state_digest(s: &State<String>) -> bool {
    s.known().map_or(true, |s| is_digest(s))
}
fn profile_valid(p: &ProfileRef) -> bool {
    token(&p.id) && token(&p.version) && is_digest(&p.digest)
}
fn selector_valid(s: &Selector) -> bool {
    if !token(&s.id) {
        return false;
    }
    if s.kind == SelectorKind::Rule {
        crate::diagnostics::registry::DiagnosticRegistry::embedded()
            .ok()
            .and_then(|r| {
                r.entry(&s.id)
                    .map(|e| e.lifecycle() == crate::diagnostics::registry::Lifecycle::Active)
            })
            .unwrap_or(false)
    } else {
        true
    }
}
fn semantic_id(s: &str, segments: usize) -> bool {
    s.len() <= 191
        && s.split('.').count() == segments
        && s.split('.').all(|p| {
            !p.is_empty()
                && p.len() <= 63
                && p.as_bytes()[0].is_ascii_lowercase()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
}
fn scope_valid(s: &Scope) -> bool {
    match s.kind {
        ScopeKind::Project | ScopeKind::Module => semantic_id(&s.id, 1),
        ScopeKind::Symbol => (2..=3).any(|n| semantic_id(&s.id, n)),
        ScopeKind::Path => s.id.len() <= 512 && crate::project_fs::path_violation(&s.id).is_none(),
        ScopeKind::Target | ScopeKind::Profile => token(&s.id),
    }
}
fn fingerprint_valid(f: &Fingerprint) -> bool {
    [&f.model, &f.ir, &f.adapter, &f.revision, &f.capabilities]
        .into_iter()
        .all(state_digest)
}
fn deadline(e: &Entry) -> Option<&str> {
    e.expires_at
        .known()
        .into_iter()
        .chain(e.review_after.known())
        .map(String::as_str)
        .min()
}
/// Approval integrity binds every decision field except the receipt itself.
/// It is a local review reference, not a cryptographic issuer authentication.
pub fn approval_subject(e: &Entry) -> String {
    let mut v = serde_json::to_value(e).expect("typed entry");
    let object = v.as_object_mut().expect("object");
    object.remove("approvalRef");
    object.remove("lifecycle");
    hash(&v)
}
pub fn validate_store(s: &Store) -> Result<(), DomainResult> {
    if s.schema_version != format!("lekalo/ai-lint-waivers/v{VERSION}")
        || s.identity != format!("dev.lekalo.ai-lint-waivers@{VERSION}")
    {
        return Err(crate::ai_lint::diagnostic::failure(
            "ai-lint.version-unsupported",
            "waivers-contract",
        ));
    }
    if !semantic_id(&s.project_id, 1)
        || s.registry_ref != input::registry_ref()
        || s.entries.len() > MAX_ENTRIES
    {
        return Err(failure("waivers-store-pins"));
    }
    let mut ids = BTreeSet::new();
    let mut successors = BTreeSet::new();
    for e in &s.entries {
        if !ids.insert(&e.id)
            || !token(&e.id)
            || !token(&e.finding_id)
            || !is_digest(&e.fact_digest)
            || !selector_valid(&e.selector)
            || !token(&e.subject)
            || !token(&e.target)
            || !profile_valid(&e.profile_ref)
            || !is_digest(&e.condition_digest)
            || !token(&e.owner)
            || !token(&e.approver)
            || e.owner == e.approver
            || !token(&e.approval_ref.id)
            || !is_digest(&e.approval_ref.subject_digest)
            || !input::prose(&e.reason)
            || !fingerprint_valid(&e.fingerprint)
            || !token(&e.source_ref.id)
            || !matches!(e.source_ref.kind.as_str(), "issue" | "decision")
            || !state_digest(&e.source_ref.digest)
        {
            return Err(failure("waivers-entry-invalid"));
        }
        if !scope_valid(&e.scope) {
            return Err(failure("waivers-exact-scope"));
        }
        let created = timestamp(&e.created_at).ok_or_else(|| failure("waivers-created-at"))?;
        let until = deadline(e).ok_or_else(|| failure("waivers-deadline-required"))?;
        for date in e
            .expires_at
            .known()
            .into_iter()
            .chain(e.review_after.known())
        {
            if timestamp(date).map_or(true, |t| t < created) {
                return Err(failure("waivers-deadline-invalid"));
            }
        }
        if timestamp(until).is_none() || e.approval_ref.subject_digest != approval_subject(e) {
            return Err(failure("waivers-approval-binding"));
        }
        if let Some(prior) = e.supersedes.known() {
            if !token(prior) || prior == &e.id || !successors.insert(prior) {
                return Err(failure("waivers-lineage-conflict"));
            }
        }
    }
    let map: BTreeMap<_, _> = s.entries.iter().map(|e| (e.id.as_str(), e)).collect();
    for e in &s.entries {
        if let Some(id) = e.supersedes.known() {
            if map
                .get(id.as_str())
                .map_or(true, |p| p.lifecycle != Lifecycle::Superseded)
            {
                return Err(failure("waivers-lineage-missing"));
            }
        }
        if e.lifecycle == Lifecycle::Superseded && !successors.contains(&e.id) {
            return Err(failure("waivers-lineage-missing"));
        }
        let mut visited = BTreeSet::new();
        let mut current = e;
        while let Some(p) = current.supersedes.known() {
            if !visited.insert(p) {
                return Err(failure("waivers-lineage-cycle"));
            }
            current = map
                .get(p.as_str())
                .ok_or_else(|| failure("waivers-lineage-missing"))?;
        }
    }
    Ok(())
}
pub fn empty_store(project_id: String) -> Store {
    Store {
        schema_version: format!("lekalo/ai-lint-waivers/v{VERSION}"),
        identity: format!("dev.lekalo.ai-lint-waivers@{VERSION}"),
        project_id,
        registry_ref: input::registry_ref(),
        entries: Vec::new(),
    }
}
pub fn validate_input(i: &Input) -> Result<(), DomainResult> {
    if i.schema_version != format!("lekalo/waiver-input/v{VERSION}")
        || i.identity != format!("dev.lekalo.waiver-input@{VERSION}")
        || !semantic_id(&i.project_id, 1)
        || !profile_valid(&i.profile_ref)
        || !state_digest(&i.lock_ref)
        || i.facts.len() > MAX_ENTRIES
    {
        return Err(failure("waivers-facts-pins"));
    }
    let mut ids = BTreeSet::new();
    for f in &i.facts {
        if !ids.insert(&f.id)
            || !token(&f.id)
            || !selector_valid(&f.selector)
            || !token(&f.subject)
            || !token(&f.target)
            || !is_digest(&f.condition_digest)
            || !fingerprint_valid(&f.fingerprint)
            || !matches!(
                f.source_outcome.as_str(),
                "warning" | "unsupported" | "unknown" | "invalid" | "security" | "data-loss"
            )
            || f.symbol
                .known()
                .is_some_and(|v| !(2..=3).any(|n| semantic_id(v, n)))
            || f.module.known().is_some_and(|v| !semantic_id(v, 1))
            || f.source_severity
                .known()
                .is_some_and(|v| !matches!(v.as_str(), "info" | "warning" | "error"))
            || f.source_confidence.known().is_some_and(|v| {
                !matches!(v.as_str(), "unknown" | "low" | "medium" | "high" | "exact")
            })
            || f.path
                .known()
                .is_some_and(|p| p.len() > 512 || crate::project_fs::path_violation(p).is_some())
            || f.evidence_refs.len() > 32
            || !f.evidence_refs.iter().all(|s| is_digest(s))
            || f.evidence_refs.iter().collect::<BTreeSet<_>>().len() != f.evidence_refs.len()
        {
            return Err(failure("waivers-fact-invalid"));
        }
    }
    Ok(())
}
fn scope_matches(e: &Entry, f: &Fact, i: &Input) -> bool {
    match e.scope.kind {
        ScopeKind::Project => e.scope.id == i.project_id,
        ScopeKind::Module => f.module.known() == Some(&e.scope.id),
        ScopeKind::Symbol => f.symbol.known() == Some(&e.scope.id),
        ScopeKind::Path => f.path.known() == Some(&e.scope.id),
        ScopeKind::Target => f.target == e.scope.id,
        ScopeKind::Profile => i.profile_ref.id == e.scope.id,
    }
}
fn pin_differences(
    a: &Fingerprint,
    b: &Fingerprint,
    required: policy::FingerprintRequirements,
) -> (Vec<String>, bool) {
    let mut mismatch = Vec::new();
    let mut unknown = false;
    for (name, a, b, required) in [
        ("model", &a.model, &b.model, required.model),
        ("ir", &a.ir, &b.ir, required.ir),
        ("adapter", &a.adapter, &b.adapter, required.adapter),
        ("revision", &a.revision, &b.revision, required.revision),
        (
            "capabilities",
            &a.capabilities,
            &b.capabilities,
            required.capabilities,
        ),
    ] {
        if required && (a.known().is_none() || b.known().is_none()) {
            unknown = true;
        }
        match (a, b) {
            (State::Known(a), State::Known(b)) if a != b => mismatch.push(name.into()),
            (State::Known(_), State::Unsupported) | (State::Unsupported, State::Known(_)) => {
                mismatch.push(name.into())
            }
            (State::Known(_), State::Known(_)) | (State::Unsupported, State::Unsupported) => {}
            _ => unknown = true,
        }
    }
    (mismatch, unknown)
}
pub fn compare(base: &Store, candidate: &Store) -> Result<Vec<Change>, DomainResult> {
    validate_store(base)?;
    validate_store(candidate)?;
    if base.project_id != candidate.project_id {
        return Err(failure("waivers-diff-project"));
    }
    let before: BTreeMap<_, _> = base.entries.iter().map(|e| (&e.id, e)).collect();
    let after: BTreeMap<_, _> = candidate.entries.iter().map(|e| (&e.id, e)).collect();
    let ids: BTreeSet<_> = before.keys().chain(after.keys()).copied().collect();
    let mut result = Vec::new();
    for id in ids {
        let b = before.get(id).map(hash);
        let a = after.get(id).map(hash);
        if a != b {
            result.push(Change {
                id: id.clone(),
                kind: match (&b, &a) {
                    (None, _) => "added",
                    (_, None) => "removed",
                    _ => "changed",
                }
                .into(),
                before: b.map_or(State::Unknown, State::Known),
                after: a.map_or(State::Unknown, State::Known),
            });
        }
    }
    Ok(result)
}
pub fn audit(
    s: &Store,
    i: &Input,
    p: &dyn ProfileState,
    as_of: &str,
    window: u64,
    base: Option<&Store>,
) -> Result<Audit, DomainResult> {
    validate_store(s)?;
    validate_input(i)?;
    let now = timestamp(as_of).ok_or_else(|| failure("waivers-as-of"))?;
    if s.project_id != i.project_id || i.profile_ref != p.reference() || window > MAX_WINDOW {
        return Err(failure("waivers-evaluation-pins"));
    }
    // The exact subject/condition bounds every named broad scope to an approved occurrence.
    if s.entries.len().saturating_mul(i.facts.len()) > input::MAX_WORK {
        return Err(failure("waivers-work-limit"));
    }
    let mut entries = Vec::new();
    let mut applied = BTreeMap::new();
    let mut summary = Summary::default();
    for e in &s.entries {
        let candidates: Vec<_> = i
            .facts
            .iter()
            .filter(|f| {
                f.selector == e.selector
                    && f.id == e.finding_id
                    && f.subject == e.subject
                    && f.target == e.target
                    && scope_matches(e, f, i)
            })
            .collect();
        if candidates.len() > 1 {
            return Err(failure("waivers-occurrence-ambiguous"));
        }
        let fact = candidates.first().copied();
        let mut reasons = Vec::new();
        let mut mismatch = Vec::new();
        let mut status = "active";
        if e.profile_ref != i.profile_ref {
            mismatch.push("profile".into());
        }
        let state = fact.and_then(|f| p.rule(&e.selector, f));
        let nonwaivable = state.as_ref().is_some_and(|r| {
            !r.enabled
                || !r.waivable
                || r.required_evidence
                || r.severity == crate::diagnostics::types::Severity::Error
        }) || fact.is_some_and(|f| {
            f.source_severity.known().is_some_and(|s| s == "error")
                || matches!(
                    f.source_outcome.as_str(),
                    "invalid" | "security" | "data-loss"
                )
        });
        if let Some(f) = fact {
            if e.fact_digest != hash(f) {
                mismatch.push("fact".into());
            }
            if e.condition_digest != f.condition_digest {
                mismatch.push("condition".into());
            }
            let (mut pins, unknown) = pin_differences(
                &e.fingerprint,
                &f.fingerprint,
                p.fingerprint_requirements(f),
            );
            mismatch.append(&mut pins);
            if unknown || !policy::admitted_producer_domain(f) || !p.producer_domain_admitted(f) {
                reasons.push("fingerprint-unverifiable".into());
            }
        }
        if !mismatch.is_empty() {
            reasons.push("fingerprint-changed".into());
        }
        if timestamp(deadline(e).expect("validated")).expect("validated") < now {
            reasons.push("expired".into());
        }
        if timestamp(&e.created_at).expect("validated") > now {
            reasons.push("not-created".into());
        }
        if nonwaivable {
            reasons.push("profile-non-waivable".into());
        }
        if e.lifecycle != Lifecycle::Active {
            reasons.push(
                match e.lifecycle {
                    Lifecycle::Revoked => "revoked",
                    Lifecycle::Superseded => "superseded",
                    Lifecycle::Active => unreachable!(),
                }
                .into(),
            );
        }
        if nonwaivable {
            status = "non-waivable";
            summary.non_waivable += 1;
        } else if e.lifecycle == Lifecycle::Revoked {
            status = "revoked";
            summary.revoked += 1;
        } else if e.lifecycle == Lifecycle::Superseded {
            status = "superseded";
            summary.superseded += 1;
        } else if reasons.iter().any(|r| r == "expired") {
            status = "expired";
            summary.expired += 1;
        } else if !mismatch.is_empty() {
            status = "stale";
            summary.stale += 1;
        } else if reasons
            .iter()
            .any(|r| r == "fingerprint-unverifiable" || r == "not-created")
            || fact.is_some() && state.is_none()
        {
            status = "unverifiable";
            summary.unverifiable += 1;
        } else if fact.is_none() {
            status = if i.complete { "orphan" } else { "unexamined" };
            if i.complete {
                summary.orphan += 1;
            } else {
                summary.unexamined += 1;
            }
        } else if timestamp(deadline(e).expect("validated")).expect("validated")
            <= now.saturating_add(window as i64)
        {
            status = "expiring";
            summary.expiring += 1;
        } else {
            summary.active_waivers += 1;
        }
        let effective = matches!(status, "active" | "expiring");
        if effective {
            let f = fact.expect("effective has fact");
            if applied.insert(&f.id, e).is_some() {
                return Err(failure("waivers-overlapping-acceptance"));
            }
        }
        reasons.sort();
        mismatch.sort();
        entries.push(AuditEntry {
            id: e.id.clone(),
            status: status.into(),
            effective,
            deadline: deadline(e).expect("validated").into(),
            reason_codes: reasons,
            mismatched_pins: mismatch,
            finding: fact.map_or(State::Unknown, |f| State::Known(f.id.clone())),
            entry_digest: hash(e),
            approval_ref: e.approval_ref.clone(),
            supersedes: e.supersedes.clone(),
        });
    }
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    let mut findings = Vec::new();
    let mut denied = false;
    for f in &i.facts {
        let state = p.rule(&f.selector, f);
        let original = if matches!(
            f.source_outcome.as_str(),
            "invalid" | "security" | "data-loss"
        ) || f.source_severity.known().is_some_and(|s| s == "error")
            || state.as_ref().map_or(true, |r| r.blocking)
        {
            "denied"
        } else {
            "advisory"
        };
        let waiver = applied.get(&f.id);
        let effective = if waiver.is_some() {
            "accepted-risk"
        } else {
            original
        };
        denied |= effective == "denied";
        findings.push(Disposition {
            fact: f.clone(),
            fact_digest: hash(f),
            severity: state.map_or(State::Unknown, |r| State::Known(r.severity.as_str().into())),
            original_gate: original.into(),
            effective_gate: effective.into(),
            waiver: waiver.map_or(State::Unknown, |e| State::Known(e.id.clone())),
            waiver_entry_digest: waiver.map_or(State::Unknown, |e| State::Known(hash(e))),
        });
    }
    findings.sort_by(|a, b| a.fact.id.cmp(&b.fact.id));
    summary.raw = findings.len() as u64;
    summary.waived = applied.len() as u64;
    summary.active = summary.raw - summary.waived;
    let mut result = Audit {
        schema_version: format!("lekalo/waiver-audit/v{VERSION}"),
        identity: format!("dev.lekalo.waiver-audit@{VERSION}"),
        project_id: s.project_id.clone(),
        registry_ref: input::registry_ref(),
        profile_ref: p.reference(),
        store_digest: hash(s),
        input_digest: hash(i),
        lock_ref: i.lock_ref.clone(),
        as_of: as_of.into(),
        expiring_window_seconds: window,
        entries,
        findings,
        summary,
        base_store_digest: base.map_or(State::Unknown, |b| State::Known(hash(b))),
        changes: base.map(|b| compare(b, s)).transpose()?.unwrap_or_default(),
        denied,
        done_digest: String::new(),
    };
    let mut projection = serde_json::to_value(&result).expect("audit");
    projection
        .as_object_mut()
        .expect("object")
        .remove("doneDigest");
    result.done_digest = hash(&projection);
    Ok(result)
}
pub fn render(a: &Audit, check: bool) -> DomainResult {
    let human = format!(
        "Waivers: {} raw, {} active, {} accepted; {} expired, {} stale",
        a.summary.raw, a.summary.active, a.summary.waived, a.summary.expired, a.summary.stale
    );
    let json = serde_json::json!({"status":if check&&a.denied{"denied"}else{"valid"},"audit":a})
        .to_string();
    if check && a.denied {
        let d =
            crate::ai_lint::diagnostic::summary("ai-lint.policy-denied", "waivers-profile-gate");
        DomainResult::denied_json(
            json,
            human,
            DiagnosticSet::try_from_unsorted(vec![d], Status::Denied).expect("registered"),
        )
    } else {
        DomainResult::graph(json, human, Vec::new())
    }
}
