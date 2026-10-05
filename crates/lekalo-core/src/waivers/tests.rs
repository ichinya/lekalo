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
fn fingerprint_pin<'a>(f: &'a mut Fingerprint, name: &str) -> &'a mut State<String> {
    match name {
        "model" => &mut f.model,
        "ir" => &mut f.ir,
        "adapter" => &mut f.adapter,
        "revision" => &mut f.revision,
        "capabilities" => &mut f.capabilities,
        _ => unreachable!(),
    }
}
#[test]
fn waivers_native_applicable_pins_cannot_be_declared_unavailable() {
    let target = target(false);
    let policy = TargetState { profile: &target };
    for name in ["model", "ir", "adapter", "revision", "capabilities"] {
        for missing in [State::Unsupported, State::Unknown, State::Withheld] {
            let mut f = fact("transport.streaming");
            *fingerprint_pin(&mut f.fingerprint, name) = missing;
            // Bind the approval to the actual incomplete fact, so refusal cannot
            // be explained by a changed fact/approval rather than applicability.
            let (s, i) = pair(f, &policy);
            let a = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None).unwrap();
            assert_eq!(a.entries[0].status, "unverifiable", "{name}");
            assert!(!a.entries[0].effective, "{name}");
            assert_eq!(a.entries[0].reason_codes, ["fingerprint-unverifiable"]);
            assert_eq!(a.summary.waived, 0);
            assert_eq!(a.findings[0].fact, i.facts[0]);
            assert_eq!(a.findings[0].waiver, State::Unknown);
        }
    }
    let mut all_missing = fact("transport.streaming");
    for name in ["model", "ir", "adapter", "revision", "capabilities"] {
        *fingerprint_pin(&mut all_missing.fingerprint, name) = State::Unsupported;
    }
    let (s, i) = pair(all_missing, &policy);
    assert_eq!(
        audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None)
            .unwrap()
            .summary
            .waived,
        0
    );
}
#[test]
fn waivers_model_inapplicability_does_not_hide_required_pins() {
    let target = target(false);
    let policy = TargetState { profile: &target };
    let mut model = fact("transport.streaming");
    model.target = "model".into();
    model.id = crate::ai_lint::model_finding_id(&model.subject, &model.symbol);
    model.fingerprint.revision = State::Known(input::hash(&(
        model.fingerprint.model.known().unwrap(),
        model.fingerprint.ir.known().unwrap(),
    )));
    model.fingerprint.adapter = State::Unsupported;
    model.fingerprint.capabilities = State::Unsupported;
    let (s, i) = pair(model.clone(), &policy);
    let a = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None).unwrap();
    assert_eq!(a.summary.waived, 1);
    assert_eq!(a.findings[0].fact, model);
    for name in ["model", "ir", "revision", "adapter", "capabilities"] {
        for missing in [State::Unknown, State::Withheld, State::Unsupported] {
            if matches!(missing, State::Unsupported) && matches!(name, "adapter" | "capabilities") {
                continue;
            }
            let mut f = model.clone();
            *fingerprint_pin(&mut f.fingerprint, name) = missing;
            let (s, i) = pair(f, &policy);
            let a = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None).unwrap();
            assert_eq!(a.entries[0].status, "unverifiable", "{name}");
            assert_eq!(a.summary.waived, 0);
        }
    }
}
#[test]
fn waivers_model_domain_cannot_be_claimed_by_relabelling_a_native_fact() {
    let target = target(false);
    let policy = TargetState { profile: &target };
    for missing in [false, true] {
        let mut native = fact("transport.streaming");
        native.id = input::hash(&(
            "hidden.string-reference",
            &native.subject,
            &native.target,
            &native.symbol,
        ));
        if missing {
            native.fingerprint.capabilities = State::Unsupported;
        }
        native.target = "model".into();
        let (s, i) = pair(native, &policy);
        let a = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None).unwrap();
        assert_eq!(a.entries[0].status, "unverifiable");
        assert_eq!(a.entries[0].reason_codes, ["fingerprint-unverifiable"]);
        assert_eq!(a.summary.waived, 0);
        assert!(!a.entries[0].effective);
        assert_eq!(a.findings[0].fact, i.facts[0]);
    }
}
#[test]
fn waivers_model_domain_requires_the_complete_producer_binding() {
    let target = target(false);
    let policy = TargetState { profile: &target };
    let mut model = fact("transport.streaming");
    model.target = "model".into();
    model.id = crate::ai_lint::model_finding_id(&model.subject, &model.symbol);
    model.fingerprint.revision = State::Known(input::hash(&(
        model.fingerprint.model.known().unwrap(),
        model.fingerprint.ir.known().unwrap(),
    )));
    model.fingerprint.adapter = State::Unsupported;
    model.fingerprint.capabilities = State::Unsupported;
    for changed in ["id", "subject", "symbol", "revision", "adapter", "path"] {
        let mut f = model.clone();
        match changed {
            "id" => f.id = input::hash(&"native finding"),
            "subject" => f.subject = "planner.other".into(),
            "symbol" => f.symbol = State::Known("planner.other".into()),
            "revision" => f.fingerprint.revision = State::Known(input::hash(&"native revision")),
            "adapter" => f.fingerprint.adapter = State::Known(input::hash(&"native adapter")),
            "path" => f.path = State::Known("src/events.ts".into()),
            _ => unreachable!(),
        }
        // Approval and whole-fact digest agree with the changed claim: only the
        // producer-domain binding prevents acceptance, including known pins.
        let (mut s, i) = pair(f, &policy);
        s.entries[0].scope = Scope {
            kind: ScopeKind::Project,
            id: "planner".into(),
        };
        s.entries[0].approval_ref.subject_digest = approval_subject(&s.entries[0]);
        let a = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None).unwrap();
        assert_eq!(a.entries[0].status, "unverifiable", "{changed}");
        assert_eq!(a.summary.waived, 0, "{changed}");
    }
}
#[test]
fn waivers_model_lint_binding_cannot_claim_a_native_only_selector() {
    let config: crate::ai_lint::Config = input::parse(include_bytes!(
        "../../../../tests/fixtures/ai-lint-config/golden/config.json"
    ))
    .unwrap();
    let policy = policy::LintState {
        config: &config,
        id: "ci",
    };
    let mut f = fact("unused");
    f.target = "model".into();
    f.id = crate::ai_lint::model_finding_id(&f.subject, &f.symbol);
    f.fingerprint.revision = State::Known(input::hash(&(
        f.fingerprint.model.known().unwrap(),
        f.fingerprint.ir.known().unwrap(),
    )));
    f.fingerprint.adapter = State::Unsupported;
    f.fingerprint.capabilities = State::Unsupported;
    f.source_confidence = State::Known("high".into());
    f.selector.kind = SelectorKind::Rule;
    for (id, accepted) in [
        ("hidden.string-reference", false),
        (crate::ai_lint::MODEL_DEPTH_RULE, true),
    ] {
        f.selector.id = id.into();
        let (s, i) = pair(f.clone(), &policy);
        let a = audit(&s, &i, &policy, "2026-10-04T00:00:00Z", 0, None).unwrap();
        assert_eq!(a.entries[0].effective, accepted);
        assert_eq!(a.summary.waived, u64::from(accepted));
        assert_eq!(a.findings[0].fact, f);
        if !accepted {
            assert_eq!(a.entries[0].status, "unverifiable");
            assert_eq!(a.entries[0].reason_codes, ["fingerprint-unverifiable"]);
        }
    }
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
