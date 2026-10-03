//! Acceptance probes against the production core, independent of schema fixtures.
use lekalo_core::{
    coupling::{self, wire::State, Profile, Request, Selection},
    ir::{Compilation, Definition, Portability, Visibility},
    loader::{normalize_model, LoadSelection},
};
fn planner() -> Compilation {
    let m = normalize_model(&LoadSelection {
        project: Some("tests/fixtures/coupling/planner".into()),
    })
    .expect("fixture loads");
    lekalo_core::ir::compile(&m).expect("fixture compiles")
}
fn request(id: &str) -> Request {
    Request {
        selection: Selection::Symbol(id.into()),
        field: None,
        context_budget: None,
        source_revision: None,
        model_digest: None,
    }
}
fn report(c: &Compilation, id: &str) -> coupling::wire::Report {
    coupling::analyze(&request(id), &Profile::default(), c, None).expect("measured")
}
fn row<'a>(r: &'a coupling::wire::Report, id: &str) -> &'a coupling::wire::Subject {
    r.subjects.iter().find(|s| s.subject == id).unwrap()
}

#[test]
fn unique_neighbors_and_union_arithmetic() {
    let r = report(&planner(), "planner.due_window");
    let s = row(&r, "type:planner.due_window");
    assert_eq!(s.metrics["fanOutSymbols"], State::known(1)); // two actual field edge occurrences
    assert_eq!(s.metrics["fanOutModules"], State::known(0)); // local neighbors are not foreign modules
    assert_eq!(
        row(&r, "project:planner").impact.public_contracts,
        s.impact.public_contracts
    );
    coupling::compare::validate(&r).expect("arithmetic and exact witnesses validate");
    assert!(r
        .witnesses
        .iter()
        .any(|w| w.edges.iter().any(|e| e.occurrence > 0)));
}

#[test]
fn internal_bridge_reaches_public_and_supporting_is_separate() {
    let mut c = planner();
    for d in &mut c.project.definitions {
        if let Definition::ValueObject(v) = d {
            if v.id.as_str() == "planner.due_window" {
                v.common.visibility = Some(Visibility::Module);
            }
        }
    }
    for d in &mut c.project.definitions {
        if let Definition::Entity(v) = d {
            if v.id.as_str() == "planner.task" {
                v.fields[3].r#type = v.fields[4].r#type.clone();
            }
        }
    }
    let r = report(&c, "planner.due_date");
    let s = row(&r, "type:planner.due_date");
    assert!(s
        .impact
        .internal_symbols
        .contains(&"type:planner.due_window".into()));
    assert!(s
        .impact
        .public_contracts
        .contains(&"entity:planner.task".into()));
    assert!(!s
        .impact
        .public_contracts
        .iter()
        .any(|s| s.starts_with("effect:") || s.starts_with("scenario:")));
    let w = r
        .witnesses
        .iter()
        .find(|w| w.root == "type:planner.due_date" && w.subject == "entity:planner.task")
        .unwrap();
    assert_eq!(w.edges.len(), 2);
}

#[test]
fn centrality_is_visible_and_advisory_by_default() {
    let c = planner();
    let r = report(&c, "planner.task");
    let s = row(&r, "entity:planner.task");
    assert_eq!(s.metrics["fanInSymbols"], State::known(6));
    assert_eq!(s.metrics["publicContractsAffected"], State::known(8));
    assert_eq!(s.metrics["sharedMutableResources"], State::known(1));
    assert!(coupling::compare::denial(&r.profile, &r, None).is_empty());
    assert!(r
        .findings
        .iter()
        .any(|f| f.rule == "coupling.shared-mutable-state"));
    assert!(!r.findings.iter().any(|f| f.rule == "coupling.fan-exceeded"));
}

#[test]
fn field_fallback_is_possible_not_exact_or_lower_bound() {
    let c = planner();
    let mut req = request("planner.task");
    req.field = Some("state".into());
    let r = coupling::analyze(&req, &Profile::default(), &c, None).unwrap();
    let s = row(&r, "entity:planner.task#field.state");
    assert_eq!(s.metrics["publicContractsAffected"], State::Unknown);
    assert_eq!(s.metrics["fanInSymbols"], State::Unknown);
    assert_eq!(s.metrics["fanOutSymbols"], State::known(1));
    assert!(!s.possible.is_empty());
    assert!(s.lower_bounds.is_empty());
    assert!(!r.complete);
    coupling::compare::validate(&r).unwrap();
}

#[test]
fn unknown_pairs_never_become_comparable_zeroes() {
    let r = report(&planner(), "planner.task");
    let bytes = serde_json::to_vec(&r).unwrap();
    let c = coupling::compare::compare(&r, &r, &bytes).unwrap();
    let unavailable = c
        .rows
        .iter()
        .find(|d| d.subject == "entity:planner.task" && d.metric == "affectedTests")
        .unwrap();
    assert_eq!(unavailable.state, "incomparable");
    assert_eq!(unavailable.absolute, State::Unknown);
}

#[test]
fn strict_baseline_regression_and_threshold_edits_preserve_trend() {
    let base_comp = planner();
    let base = report(&base_comp, "planner.task");
    let bytes = serde_json::to_vec(&base).unwrap();
    let mut candidate = base_comp;
    let task_ref = candidate
        .project
        .definitions
        .iter()
        .find_map(|d| {
            if let Definition::Query(q) = d {
                if q.id.as_str() == "planner.list_tasks" {
                    q.returns.clone()
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();
    for d in &mut candidate.project.definitions {
        if let Definition::ValueObject(v) = d {
            if v.id.as_str() == "planner.due_window" {
                v.fields[0].r#type = task_ref.clone();
            }
        }
    }
    let mut p = Profile::default();
    p.gate.mode = "strict".into();
    p.gate.fail_on = vec!["baseline-regression".into()];
    p.gate.baseline_ready_ref = State::known(format!(
        "sha256:{}",
        lekalo_core::digest::sha256_hex(&bytes)
    ));
    p.project.regression_limits = vec![coupling::profile::RegressionLimit {
        metric: "fanInSymbols".into(),
        absolute_increase: 0,
        relative_increase: State::Unknown,
    }];
    p.project.limits = vec![coupling::profile::Limit {
        metric: "fanInSymbols".into(),
        maximum: 999,
    }];
    let current = coupling::analyze(&request("planner.task"), &p, &candidate, None).unwrap();
    let comparison = coupling::compare::compare(&base, &current, &bytes).unwrap();
    assert!(comparison.comparable);
    assert!(comparison.regressions > 0);
    assert!(coupling::compare::denial(&p, &current, Some(&comparison))
        .contains(&"baseline-regression".into()));
    assert!(coupling::compare::denial(&p, &current, None).contains(&"baseline-required".into()));
}

#[test]
fn closed_baselines_reject_forged_scalars_nulls_and_duplicate_keys() {
    let mut r = report(&planner(), "planner.task");
    r.subjects[0]
        .metrics
        .insert("publicContractsAffected".into(), State::known(999));
    assert!(coupling::compare::parse_baseline(&serde_json::to_vec(&r).unwrap()).is_err());
    assert!(coupling::Profile::parse(br#"{"schemaVersion":"x","schemaVersion":"y"}"#).is_err());
    assert!(coupling::wire::decode::<State<u64>>(br#"{"state":"unknown","value":null}"#).is_err());
    assert!(coupling::wire::decode::<State<u64>>(br#"{"state":"unknown","value":3}"#).is_err());
}

#[test]
fn transitive_target_exposure_is_a_fact_with_paths() {
    let mut c = planner();
    for d in &mut c.project.definitions {
        if let Definition::Scalar(s) = d {
            if s.id.as_str() == "planner.due_date" {
                s.common.portability = Some(Portability::TargetSpecific);
            }
        }
    }
    let r = report(&c, "planner.task");
    let s = row(&r, "entity:planner.task");
    assert_eq!(s.metrics["publicTargetExposure"], State::known(1));
    assert!(r
        .findings
        .iter()
        .any(|f| f.rule == "coupling.public-target-exposure"));
    assert!(r
        .witnesses
        .iter()
        .any(|w| w.direction == "forward" && w.subject == "type:planner.due_date"));
}

#[test]
fn context_plan_keeps_required_facts_and_capsule_simulation() {
    let c = planner();
    let mut req = request("planner.task");
    req.context_budget = Some(1);
    let r = coupling::analyze(&req, &Profile::default(), &c, None).unwrap();
    let budget: serde_json::Value =
        serde_json::from_str(r.planning.context_budget.value().unwrap()).unwrap();
    assert_eq!(budget["identity"], "dev.lekalo.context-budget-report@0.6.3");
    assert!(!r.planning.public_contracts.is_empty());
    assert!(!r.planning.resources.is_empty());
}

#[test]
fn cycle_series_are_separate() {
    let mut c = planner();
    let task = c
        .project
        .definitions
        .iter()
        .find(|d| d.id().as_str() == "planner.task")
        .unwrap()
        .id()
        .clone();
    let user = c
        .project
        .definitions
        .iter()
        .find(|d| d.id().as_str() == "notify.user")
        .unwrap()
        .id()
        .clone();
    for d in &mut c.project.definitions {
        if let Definition::Entity(e) = d {
            let other = match e.id.as_str() {
                "planner.task" => Some(user.clone()),
                "notify.user" => Some(task.clone()),
                _ => None,
            };
            if let Some(other) = other {
                e.fields[1].r#type = lekalo_core::ir::TypeRef::Ref(other);
            }
        }
    }
    // Pure accepted-IR probe: the loader separately rejects cyclic imports.
    let r = report(&c, "planner.task");
    assert!(r.cycles.iter().any(|c| c.series == "symbols"));
    assert!(r.cycles.iter().any(|c| c.series == "modules"));
    assert!(r
        .findings
        .iter()
        .any(|f| f.rule == "coupling.cross-module-cycle"));
    coupling::compare::validate(&r).unwrap();
}

#[test]
fn transaction_spread_uses_explicit_domain_groups() {
    use coupling::{evidence::Attachment, profile::DomainGroup};
    let mut c = planner();
    let user = c
        .project
        .definitions
        .iter()
        .find(|d| d.id().as_str() == "notify.user")
        .unwrap()
        .id()
        .clone();
    for d in &mut c.project.definitions {
        if let Definition::Effect(e) = d {
            if e.id.as_str() == "planner.create_task" {
                e.entity = user.clone();
            }
        }
    }
    let mut req = request("notify.user");
    req.model_digest = Some(format!("sha256:{}", "a".repeat(64)));
    let base = coupling::analyze(&req, &Profile::default(), &c, None).unwrap();
    let effects = lekalo_core::effects::build(&c.project).unwrap();
    let mut tx: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/transaction-concurrency/valid/planner-focus-race.json"
    ))
    .unwrap();
    tx["modelRef"]["digest"] = serde_json::json!(req.model_digest.as_ref().unwrap());
    tx["irRef"]["digest"] = serde_json::json!(base.provenance.inputs.ir_digest);
    tx["effectGraphRef"]["digest"] = serde_json::json!(effects.ir_digest());
    let mut op = tx["operations"][0].clone();
    let key = effects
        .operation_edges(
            &lekalo_core::effects::OperationId::from_qualified("operation:planner.focus_task")
                .unwrap(),
        )
        .into_iter()
        .find(|e| e.key().to_canonical_string().contains("|create|"))
        .unwrap()
        .key()
        .to_canonical_string();
    op["atomicEffectGroups"][0]["effectRefs"] = serde_json::json!([key]);
    op["preconditions"] = serde_json::json!([]);
    tx["operations"] = serde_json::json!([op]);
    tx["invariants"] = serde_json::json!([]);
    tx["concurrencyCases"] = serde_json::json!([]);
    let bytes = serde_json::to_string(&tx).unwrap();
    let evidence = coupling::Evidence {
        schema_version: "lekalo/coupling-evidence/v0.6.4".into(),
        identity: "dev.lekalo.coupling-evidence@0.6.4".into(),
        inputs: base.provenance.inputs,
        artifacts: State::Unknown,
        trace: State::Unknown,
        queries: State::Unknown,
        transactions: State::known(Attachment {
            digest: format!(
                "sha256:{}",
                lekalo_core::digest::sha256_hex(bytes.as_bytes())
            ),
            bytes,
        }),
        replicas: vec![],
    };
    let missing = coupling::analyze(&req, &Profile::default(), &c, Some(&evidence)).unwrap();
    assert_eq!(
        row(&missing, "entity:notify.user").metrics["transactionSpread"],
        State::Unknown
    );
    let p = Profile {
        domain_groups: vec![
            DomainGroup {
                id: "planning".into(),
                modules: vec!["planner".into()],
            },
            DomainGroup {
                id: "notification".into(),
                modules: vec!["notify".into()],
            },
        ],
        ..Profile::default()
    };
    let r = coupling::analyze(&req, &p, &c, Some(&evidence)).unwrap();
    assert_eq!(
        row(&r, "entity:notify.user").metrics["transactionSpread"],
        State::known(1)
    );
    assert!(r
        .findings
        .iter()
        .any(|f| f.rule == "coupling.transaction-spread"));
    coupling::compare::validate(&r).unwrap();
}
