use super::*;
use policy::{ProfileState, TargetState, ValidationState};

fn target(serverless: bool) -> crate::target_profile::resolution::ResolvedProfile {
    let mut value: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../tests/fixtures/target-profile/valid/node.json"
    ))
    .unwrap();
    if serverless {
        value["profiles"][0]["components"]["deployment"] = "serverless".into();
    }
    let document =
        crate::target_profile::document::decode(&serde_json::to_vec(&value).unwrap()).unwrap();
    crate::target_profile::resolution::resolve(&document)
        .unwrap()
        .remove(0)
}
fn fact(id: &str) -> Fact {
    Fact {
        id: "gap".into(),
        selector: Selector {
            kind: SelectorKind::Capability,
            id: id.into(),
        },
        subject: "planner.focus_task".into(),
        target: "node-typescript".into(),
        symbol: State::Known("planner.focus_task".into()),
        module: State::Known("planner".into()),
        path: State::Unknown,
        condition_digest: input::hash(&id),
        fingerprint: Fingerprint {
            model: State::Known(input::hash(&"model")),
            ir: State::Known(input::hash(&"ir")),
            adapter: State::Known(input::hash(&"adapter")),
            revision: State::Known(input::hash(&"revision")),
            capabilities: State::Known(input::hash(&"capabilities")),
        },
        source_outcome: "unsupported".into(),
        source_severity: State::Known("warning".into()),
        source_confidence: State::Unknown,
        evidence_refs: vec![input::hash(&"producer")],
    }
}
fn pair(f: Fact, p: &dyn ProfileState) -> (Store, Input) {
    let i = Input {
        schema_version: format!("lekalo/waiver-input/v{VERSION}"),
        identity: format!("dev.lekalo.waiver-input@{VERSION}"),
        project_id: "planner".into(),
        profile_ref: p.reference(),
        lock_ref: State::Unknown,
        complete: true,
        facts: vec![f.clone()],
    };
    let mut e = Entry {
        id: "waiver".into(),
        finding_id: f.id.clone(),
        fact_digest: input::hash(&f),
        selector: f.selector,
        scope: Scope {
            kind: ScopeKind::Symbol,
            id: "planner.focus_task".into(),
        },
        subject: f.subject,
        target: f.target,
        profile_ref: p.reference(),
        condition_digest: f.condition_digest,
        owner: "owner".into(),
        approver: "reviewer".into(),
        approval_ref: Approval {
            id: "decision/88".into(),
            subject_digest: String::new(),
        },
        reason: "Temporarily accept the supplied capability gap.".into(),
        created_at: "2026-10-04T00:00:00Z".into(),
        expires_at: State::Known("2026-11-01T00:00:00Z".into()),
        review_after: State::Unknown,
        source_ref: SourceRef {
            kind: "issue".into(),
            id: "ichinya/lekalo#88".into(),
            digest: State::Unknown,
        },
        accepted_risk: Risk::CapabilityGap,
        fingerprint: f.fingerprint,
        lifecycle: Lifecycle::Active,
        supersedes: State::Unknown,
    };
    e.approval_ref.subject_digest = approval_subject(&e);
    let mut s = empty_store("planner".into());
    s.entries.push(e);
    (s, i)
}
#[test]
fn waivers_optional_capability_keeps_the_unsupported_fact() {
    let target = target(false);
    let policy = TargetState { profile: &target };
    let (s, i) = pair(fact("transport.streaming"), &policy);
    let a = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 604800, None).unwrap();
    assert_eq!(a.summary.waived, 1);
    assert_eq!(a.findings[0].fact, i.facts[0]);
    assert_eq!(a.findings[0].fact.source_outcome, "unsupported");
    assert_eq!(a.findings[0].effective_gate, "accepted-risk");
}
#[test]
fn waivers_capability_requirement_comes_from_resolved_components() {
    for (serverless, accepted) in [(false, true), (true, false)] {
        let target = target(serverless);
        let policy = TargetState { profile: &target };
        let (s, i) = pair(fact("storage.pooling"), &policy);
        let a = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 604800, None).unwrap();
        assert_eq!(a.entries[0].effective, accepted);
        assert_eq!(a.denied, !accepted);
    }
}
#[test]
fn waivers_unknown_capability_policy_cannot_accept() {
    let target = target(false);
    let policy = TargetState { profile: &target };
    let (s, i) = pair(fact("absent.capability"), &policy);
    let a = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None).unwrap();
    assert_eq!(a.entries[0].status, "unverifiable");
    assert!(a.denied);
}
#[test]
fn waivers_broad_scope_cannot_cross_occurrences_or_reused_ids() {
    let target = target(false);
    let policy = TargetState { profile: &target };
    let (mut s, mut i) = pair(fact("transport.streaming"), &policy);
    s.entries[0].scope = Scope {
        kind: ScopeKind::Project,
        id: "planner".into(),
    };
    s.entries[0].approval_ref.subject_digest = approval_subject(&s.entries[0]);
    i.facts[0].symbol = State::Known("planner.other".into());
    let reused = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None).unwrap();
    assert_eq!(reused.entries[0].status, "stale");
    assert_eq!(reused.summary.waived, 0);
    assert!(reused.entries[0].mismatched_pins.contains(&"fact".into()));
    i.facts[0].id = "replacement".into();
    let replaced = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None).unwrap();
    assert_eq!(replaced.summary.waived, 0);
    assert_eq!(replaced.entries[0].status, "orphan");
}
#[test]
fn waivers_validation_severity_and_non_downgrade_are_consumed() {
    let profile = crate::validator::ValidationProfile::embedded_default().unwrap();
    let p = ValidationState {
        profile,
        digest: input::hash(&"default"),
    };
    let mut f = fact("unused");
    f.selector = Selector {
        kind: SelectorKind::Rule,
        id: "semantic.public-output-private-type".into(),
    };
    let (s, i) = pair(f, &p);
    let a = audit(&s, &i, &p, "2026-10-04T00:00:00Z", 0, None).unwrap();
    assert_eq!(a.entries[0].status, "non-waivable");
    assert_eq!(a.findings[0].severity.known().unwrap(), "error");
    assert_eq!(a.findings[0].fact.source_outcome, "unsupported");
}
#[test]
fn waivers_time_and_closed_decode_do_not_accept_aliases() {
    for invalid in [
        "2026-02-30T00:00:00Z",
        "2026-10-04T24:00:00Z",
        "2026-10-04T+1:00:00Z",
        "2026-10-04T00:00:60Z",
        "2026-10-04T00:00:00+00:00",
        "２０２６-10-04T00:00:00Z",
    ] {
        assert_eq!(timestamp(invalid), None);
    }
    assert_eq!(
        evaluation_time("2026-10-04").unwrap(),
        "2026-10-04T23:59:59Z"
    );
    for bytes in [
        br#"{"id":1,"id":2}"#.as_slice(),
        br#"{"id":1,"i\u0064":2}"#.as_slice(),
        br#"{"id":1} {}"#.as_slice(),
    ] {
        assert!(parse::<serde_json::Value>(bytes).is_err());
    }
}
#[test]
fn waivers_atomic_write_refuses_changed_source_and_other_homes() {
    let dir = tempfile::tempdir().unwrap();
    let s = empty_store("planner".into());
    let plan = store::plan_id(None, &s);
    store::apply(dir.path(), "lekalo.waivers.json", None, &s, &plan).unwrap();
    assert_eq!(
        store::read(dir.path(), "lekalo.waivers.json")
            .unwrap()
            .unwrap(),
        canonical(&s)
    );
    assert!(store::apply(dir.path(), "lekalo.waivers.json", None, &s, &plan).is_err());
    for home in [
        "package.json",
        "../outside.json",
        "lekalo/project.yaml",
        "waivers-update.guard.json",
    ] {
        assert!(store::apply(dir.path(), home, None, &s, &plan).is_err());
    }
    let _guard = store::test_guard(dir.path()).unwrap();
    assert!(store::apply(
        dir.path(),
        "lekalo.waivers.json",
        Some(&canonical(&s)),
        &s,
        &store::plan_id(Some(&canonical(&s)), &s)
    )
    .is_err());
}
