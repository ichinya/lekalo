//! Authorization core tests (issue #25).
//!
//! Golden canonical bytes, the evaluation vector matrix, closed-schema
//! rejections, bounds at N and N+1, strict review outcomes per fixture
//! variant, contribution facts, and the #23 scenario-assertion seam.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::authorization::{
    ActorType, Decision, DenyReason, Document, Evaluation, FieldAccess, Literal, Principal,
    Request, RequestScope, ResourceView, Review,
};
use lekalo_core::loader::{normalize_model, LoadSelection};
use serde_json::{json, Value as Json};

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

const FIXTURE_ROOT: &str = "tests/fixtures/authorization";
const GOLDEN: &str = "tests/fixtures/authorization/golden/planner.canonical.json";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn in_workspace<T>(work: impl FnOnce() -> T) -> T {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let outcome = work();
    std::env::set_current_dir(original).expect("restore cwd");
    outcome
}

fn compile_fixture(name: &str) -> lekalo_core::ir::Compilation {
    in_workspace(|| {
        let selection = LoadSelection {
            project: Some(format!("{FIXTURE_ROOT}/{name}")),
        };
        let model = match normalize_model(&selection) {
            Ok(model) => model,
            Err(outcome) => panic!("{name}: load failed: {}", outcome.to_json_string()),
        };
        match lekalo_core::ir::compile(&model) {
            Ok(compilation) => compilation,
            Err(failure) => panic!("{name}: IR failed: {}", {
                failure
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.code.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            }),
        }
    })
}

fn read_document(name: &str) -> Document {
    in_workspace(|| {
        let root = workspace_root().join(FIXTURE_ROOT).join(name);
        lekalo_core::authorization::parse::read_document(&root)
            .expect("document parses")
            .expect("document exists")
    })
}

fn planner_document() -> Document {
    read_document("planner")
}

fn principal_from(vector: &Json) -> Principal {
    let actor = ActorType::from_key(vector["actor"].as_str().expect("actor")).expect("actor key");
    let mut capabilities = std::collections::BTreeSet::new();
    for value in vector["capabilities"].as_array().unwrap_or(&Vec::new()) {
        capabilities.insert(value.as_str().expect("capability").to_owned());
    }
    let mut roles = std::collections::BTreeSet::new();
    for value in vector["roles"].as_array().unwrap_or(&Vec::new()) {
        roles.insert(value.as_str().expect("role").to_owned());
    }
    Principal {
        actor,
        authenticated: vector["authenticated"].as_bool().unwrap_or(false),
        id: vector["id"].as_str().unwrap_or_default().to_owned(),
        tenant_id: vector["tenant_id"].as_str().map(str::to_owned),
        workspace_id: vector["workspace_id"].as_str().map(str::to_owned),
        capabilities,
        roles,
        job: vector["job"].as_str().map(str::to_owned),
    }
}

fn literal_from(value: &Json) -> Literal {
    if let Some(text) = value.as_str() {
        return Literal::Str(text.to_owned());
    }
    if let Some(number) = value.as_i64() {
        return Literal::Int(number);
    }
    if let Some(flag) = value.as_bool() {
        return Literal::Bool(flag);
    }
    panic!("unsupported literal: {value}");
}

fn request_from(vector: &Json) -> Request {
    let mut input = std::collections::BTreeMap::new();
    for (key, value) in vector["input"].as_object().expect("input object") {
        input.insert(key.clone(), literal_from(value));
    }
    let resource = vector["resource"].as_object().map(|resource| ResourceView {
        entity: resource["entity"].as_str().expect("entity").to_owned(),
        fields: resource["fields"]
            .as_object()
            .expect("fields object")
            .iter()
            .map(|(key, value)| (key.clone(), literal_from(value)))
            .collect(),
    });
    let field = vector["field"].as_array().map(|pair| {
        let access = match pair[1].as_str().expect("access") {
            "read" => FieldAccess::Read,
            _ => FieldAccess::Write,
        };
        (pair[0].as_str().expect("field").to_owned(), access)
    });
    Request {
        operation: vector["operation"].as_str().expect("operation").to_owned(),
        input,
        resource,
        scope: RequestScope::default(),
        field,
    }
}

fn vectors() -> Vec<Json> {
    let text = in_workspace(|| {
        std::fs::read_to_string(workspace_root().join(FIXTURE_ROOT).join("vectors.json"))
            .expect("vectors fixture")
    });
    let value: Json = serde_json::from_str(&text).expect("vectors json");
    value["vectors"]
        .as_array()
        .expect("vectors array")
        .to_owned()
}

fn document_from_json(value: &Json) -> Result<Document, lekalo_core::diagnostics::DiagnosticSet> {
    Document::from_json(value)
}

#[test]
fn canonical_bytes_are_golden_and_repeat_identically() {
    let document = planner_document();
    let canonical = document.canonical_json();
    assert_eq!(
        canonical,
        document.canonical_json(),
        "canonical bytes repeat"
    );
    in_workspace(|| {
        let golden_path = workspace_root().join(GOLDEN);
        if std::env::var("LEKALO_AUTHORIZATION_REGENERATE").as_deref() == Ok("1") {
            std::fs::create_dir_all(golden_path.parent().expect("golden parent"))
                .expect("golden dir");
            std::fs::write(&golden_path, format!("{canonical}\n")).expect("write golden");
        }
        let golden = std::fs::read_to_string(&golden_path).expect("golden canonical json");
        assert_eq!(canonical, golden.trim_end(), "canonical bytes match golden");
    });
}

#[test]
fn canonical_form_is_independent_of_source_order() {
    let document = planner_document();
    // Parse a JSON form whose policies appear in reverse order: the
    // canonical bytes must not move.
    let mut value = document.to_json();
    let policies = value["policies"].as_array().expect("policies").clone();
    let mut reversed = policies.clone();
    reversed.reverse();
    value["policies"] = Json::Array(reversed);
    let permuted = document_from_json(&value).expect("permuted document parses");
    assert_eq!(permuted.canonical_json(), document.canonical_json());
    let _ = policies;
}

#[test]
fn closed_schema_rejects_unknown_fields_actors_and_duplicates() {
    let mut value = planner_document().to_json();
    value["surprise"] = json!(true);
    assert!(
        document_from_json(&value).is_err(),
        "unknown field rejected"
    );

    let mut value = planner_document().to_json();
    value["policies"][0]["actor"] = json!("identity.robot");
    assert!(
        document_from_json(&value).is_err(),
        "unknown actor rejected"
    );

    let mut value = planner_document().to_json();
    let mut policy = value["policies"][0].clone();
    policy["id"] = value["policies"][1]["id"].clone();
    value["policies"][0] = policy;
    assert!(
        document_from_json(&value).is_err(),
        "duplicate policy id rejected"
    );

    let mut value = planner_document().to_json();
    value["policies"][0]["error_ref"] = json!("not-an-id");
    assert!(
        document_from_json(&value).is_err(),
        "error ref grammar rejected"
    );
}

#[test]
fn closed_bounds_hold_at_n_and_reject_at_n_plus_one() {
    let mut value = planner_document().to_json();
    // Condition depth: depth 4 accepted, depth 5 rejected.
    let nested = json!({"all": [{"all": [{"all": [{"left": "resource.state", "op": "equals", "right": "archived"}]}]}]});
    value["policies"][0]["conditions"] = nested;
    assert!(document_from_json(&value).is_ok(), "depth 4 accepted");
    let too_deep = json!({"all": [{"all": [{"all": [{"all": [{"left": "resource.state", "op": "equals", "right": "archived"}]}]}]}]});
    value["policies"][0]["conditions"] = too_deep;
    assert!(document_from_json(&value).is_err(), "depth 5 rejected");

    // Policy count bound: 256 accepted, 257 rejected.
    let mut value = planner_document().to_json();
    let template = value["policies"][0].clone();
    let mut policies = Vec::new();
    for index in 0..256usize {
        let mut policy = template.clone();
        policy["id"] = json!(format!("planner.policy.bulk_{index:04}"));
        policies.push(policy);
    }
    value["policies"] = Json::Array(policies);
    assert!(document_from_json(&value).is_ok(), "256 policies accepted");
    let mut policies = value["policies"].as_array().expect("policies").clone();
    let mut extra = template.clone();
    extra["id"] = json!("planner.policy.bulk_over");
    policies.push(extra);
    value["policies"] = Json::Array(policies);
    assert!(document_from_json(&value).is_err(), "257 policies rejected");
}

#[test]
fn evaluation_vector_matrix_matches_expectations() {
    let document = planner_document();
    let vectors = vectors();
    assert!(vectors.len() >= 12, "the required matrix is present");
    for vector in &vectors {
        let name = vector["name"].as_str().expect("vector name");
        let evaluation = lekalo_core::authorization::evaluate::evaluate(
            &document,
            &principal_from(vector),
            &request_from(vector),
        );
        let expect = vector["expect"].as_str().expect("expect");
        match (expect, &evaluation) {
            ("allowed", Evaluation::Allowed { policy }) => {
                if let Some(expected_policy) = vector["expect_policy"].as_str() {
                    assert_eq!(policy, expected_policy, "{name}: policy mismatch");
                }
            }
            ("denied", Evaluation::Denied { reason, policy, .. }) => {
                if let Some(expected_policy) = vector["expect_policy"].as_str() {
                    assert_eq!(
                        policy.as_deref(),
                        Some(expected_policy),
                        "{name}: denying policy"
                    );
                }
                if let Some(expected_reason) = vector["expect_reason"].as_str() {
                    let rendered = match reason {
                        DenyReason::MissingAuthentication => "MissingAuthentication",
                        DenyReason::NoMatch => "NoMatch",
                        DenyReason::DenyPolicy => "DenyPolicy",
                        DenyReason::Scope => "Scope",
                        DenyReason::Capability => "Capability",
                        DenyReason::Role => "Role",
                        DenyReason::Ownership => "Ownership",
                        DenyReason::Condition => "Condition",
                        DenyReason::Field => "Field",
                        DenyReason::Composition => "Composition",
                    };
                    assert_eq!(rendered, expected_reason, "{name}: reason mismatch");
                }
            }
            _ => panic!("{name}: expected {expect}, got {evaluation:?}"),
        }
    }
}

#[test]
fn strict_review_outcomes_follow_the_fixture_variants() {
    let covered = read_document("planner");
    let compilation = compile_fixture("planner");
    assert!(matches!(
        lekalo_core::authorization::review::review(&compilation, Some(&covered), true),
        Review::Ok
    ));

    // Uncovered: every protected operation blocks without a document.
    let bare = compile_fixture("noauth");
    match lekalo_core::authorization::review::review(&bare, None, true) {
        Review::Denied(set) => {
            let rendered = set.as_slice().iter().map(|d| d.id()).collect::<Vec<_>>();
            assert!(
                rendered
                    .iter()
                    .all(|id| *id == "authorization.effect-unprotected"),
                "only effect-unprotected diagnostics: {rendered:?}"
            );
        }
        other => panic!("noauth strict must deny, got {other:?}"),
    }
    assert!(
        matches!(
            lekalo_core::authorization::review::review(&bare, None, false),
            Review::Ok
        ),
        "default profile is advisory"
    );

    // Mapping evidence below full cannot pass strict.
    let partial = read_document("mapping-partial");
    let compilation = compile_fixture("mapping-partial");
    match lekalo_core::authorization::review::review(&compilation, Some(&partial), true) {
        Review::Denied(set) => {
            assert!(
                set.as_slice()
                    .iter()
                    .any(|d| d.id() == "authorization.mapping-stale"),
                "mapping-stale reported"
            );
        }
        other => panic!("partial mapping strict must deny, got {other:?}"),
    }

    // A stale model pin cannot pass strict.
    let stale = read_document("stale");
    let compilation = compile_fixture("stale");
    match lekalo_core::authorization::review::review(&compilation, Some(&stale), true) {
        Review::Denied(_) => {}
        other => panic!("stale strict must deny, got {other:?}"),
    }
}

#[test]
fn malformed_documents_fail_parse_and_references_fail_review() {
    // A malformed document is rejected at parse time (invalid, exit 1)
    // in both profiles — review never sees a partial document.
    in_workspace(|| {
        let root = workspace_root().join(FIXTURE_ROOT).join("malformed");
        assert!(
            lekalo_core::authorization::parse::read_document(&root).is_err(),
            "malformed document rejected"
        );
    });

    // An unknown operation reference is invalid in every profile.
    let mut value = planner_document().to_json();
    value["policies"][0]["applies_to"] = json!(["planner.does_not_exist"]);
    let document = document_from_json(&value).expect("unknown ref still parses");
    let compilation = compile_fixture("planner");
    for strict in [true, false] {
        match lekalo_core::authorization::review::review(&compilation, Some(&document), strict) {
            Review::Invalid(set) => assert!(
                set.as_slice()
                    .iter()
                    .any(|d| d.id() == "authorization.ref-unresolved"),
                "ref-unresolved reported"
            ),
            other => panic!("strict={strict}: expected invalid, got {other:?}"),
        }
    }

    // The uncovered variant parses and passes the default profile but
    // blocks strict with effect-unprotected.
    let uncovered = read_document("uncovered");
    let compilation = compile_fixture("uncovered");
    assert!(matches!(
        lekalo_core::authorization::review::review(&compilation, Some(&uncovered), false),
        Review::Ok
    ));
    match lekalo_core::authorization::review::review(&compilation, Some(&uncovered), true) {
        Review::Denied(set) => assert!(
            set.as_slice()
                .iter()
                .any(|d| d.id() == "authorization.effect-unprotected"),
            "effect-unprotected reported"
        ),
        other => panic!("uncovered strict must deny, got {other:?}"),
    }
}

#[test]
fn facts_are_sorted_and_classification_matches_the_matrix() {
    use lekalo_core::authorization::facts::{authorizes_facts, classify_changes, mapping_changes};
    let document = planner_document();
    let facts = authorizes_facts(&document);
    assert!(
        facts.windows(2).all(|pair| pair[0] <= pair[1]),
        "facts sorted"
    );
    assert!(!facts.is_empty());

    // Actor broadening is security-breaking.
    let mut broader = document.to_json();
    let member = broader["policies"]
        .as_array()
        .expect("policies")
        .iter()
        .position(|p| p["id"] == "planner.policy.task_write_owner")
        .expect("owner policy");
    broader["policies"][member]["actor"] = json!("public");
    let broader = document_from_json(&broader).expect("broader parses");
    let changes = classify_changes(&document, &broader);
    let owner = changes
        .iter()
        .find(|change| change.policy == "planner.policy.task_write_owner");
    assert!(owner.is_some(), "owner policy change classified");
    assert_eq!(
        owner.expect("change").breaking.as_str(),
        "security-breaking"
    );

    // Removing a deny is security-breaking; narrowing writes is behavioral.
    let mut without_deny = document.to_json();
    without_deny["policies"] = Json::Array(
        without_deny["policies"]
            .as_array()
            .expect("policies")
            .iter()
            .filter(|p| p["id"] != "planner.policy.deny_archived")
            .cloned()
            .collect(),
    );
    let without_deny = document_from_json(&without_deny).expect("parses");
    let changes = classify_changes(&document, &without_deny);
    assert!(changes.iter().any(|change| change
        .labels
        .contains(&lekalo_core::authorization::facts::ChangeLabel::DenyRemoved)
        && change.breaking.as_str() == "security-breaking"));

    // Mapping state changes classify independently.
    let changes = mapping_changes(&document, &read_document("mapping-partial"));
    assert!(changes.iter().all(|change| change
        .labels
        .contains(&lekalo_core::authorization::facts::ChangeLabel::MappingStateChanged)));
}

#[test]
fn scenario_authorization_assertions_are_satisfied_by_the_evaluator() {
    use lekalo_core::scenario::assertion::{Assertion, AuthOutcome};
    use lekalo_core::scenario::id::NamespacedId;
    use lekalo_core::scenario::reference::{Ref, RefKind, RefTarget};

    fn actor_ref(id: &str) -> Ref {
        Ref {
            kind: RefKind::Actor,
            id: RefTarget::Namespaced(NamespacedId::parse(id).expect("namespaced id")),
            path: None,
            expected_type: None,
        }
    }
    fn assertion(policy: &str, outcome: AuthOutcome) -> Assertion {
        Assertion::Authorization {
            actor: actor_ref("core.actors/fixture"),
            policy: NamespacedId::parse(policy).expect("policy id"),
            outcome,
        }
    }
    let document = planner_document();
    let vectors = vectors();
    for vector in &vectors {
        let Some(expect_policy) = vector["expect_policy"].as_str() else {
            continue;
        };
        let Some(expect_reason) = vector["expect_reason"].as_str() else {
            continue;
        };
        if expect_reason != "DenyPolicy" {
            continue;
        }
        // The deny-overrides-allow vector must satisfy a denied assertion
        // against the denying policy, and fail an allowed assertion.
        let principal = principal_from(vector);
        let request = request_from(vector);
        assert!(
            lekalo_core::authorization::evaluate::satisfies_scenario_assertion(
                &document,
                &assertion(expect_policy, AuthOutcome::Denied),
                &principal,
                &request,
            ),
            "{}: denied assertion satisfied",
            vector["name"].as_str().expect("name")
        );
        assert!(
            !lekalo_core::authorization::evaluate::satisfies_scenario_assertion(
                &document,
                &assertion(expect_policy, AuthOutcome::Allowed),
                &principal,
                &request,
            )
        );
    }
    let _ = Decision::Allow;
}
