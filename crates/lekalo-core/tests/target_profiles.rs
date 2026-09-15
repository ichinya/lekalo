//! Issue #29 integration gate: the composable target profile contract
//! executed against production decoding and resolution.
//!
//! The committed fixtures under `tests/fixtures/target-profile/` are the
//! machine-readable form of the acceptance criteria: the issue's Node and
//! Laravel profiles plus a Go runtime reuse the storage, transport, and
//! deployment components unchanged; incompatible combinations, unknown
//! components, unknown bases, and inheritance weakening are refused with
//! their registered rules; the resolved snapshot is deterministic,
//! digest-bound, and projects exactly the closed schema shape. Nothing
//! here is simulated: the same `decode`/`resolve`/`portability` seam the
//! lock and the adapter protocol consume runs on every vector.

use lekalo_core::target_profile::component::{Axis, Support};
use lekalo_core::target_profile::document;
use lekalo_core::target_profile::portability::{portability, Verdict};
use lekalo_core::target_profile::resolution::resolve;
use lekalo_core::target_profile::ProfileFailure;

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/target-profile"
);

fn read(relative: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURES}/{relative}")).expect("fixture is readable")
}

fn resolve_fixture(relative: &str) -> lekalo_core::target_profile::resolution::ResolvedProfile {
    let document = document::decode(&read(relative)).expect("decodes");
    resolve(&document).expect("resolves").remove(0)
}

#[test]
fn the_issue_profiles_resolve_with_reusable_components() {
    let node = resolve_fixture("valid/node.json");
    let laravel = resolve_fixture("valid/laravel.json");
    let go = resolve_fixture("valid/go.json");

    for profile in [&node, &laravel, &go] {
        assert_eq!(profile.components.len(), 6);
        assert_eq!(profile.provenance.len(), 6);
        assert!(profile.overrides_applied.is_empty());
    }
    // Storage, transport, and deployment are reusable between Node,
    // PHP, and Go: identical components on those axes.
    for axis in [Axis::Storage, Axis::Transport, Axis::Deployment] {
        assert_eq!(node.component(axis), laravel.component(axis));
        assert_eq!(laravel.component(axis), go.component(axis));
    }
    // Runtime-bound axes change with the runtime.
    assert_ne!(node.component(Axis::Runtime), go.component(Axis::Runtime));
    assert_ne!(
        node.component(Axis::Testing),
        laravel.component(Axis::Testing)
    );
}

#[test]
fn the_monorepo_document_resolves_every_profile_deterministically() {
    let document = document::decode(&read("valid/monorepo.json")).expect("decodes");
    let first = resolve(&document).expect("resolves");
    let second = resolve(&document).expect("resolves");
    assert_eq!(first, second, "two runs are byte-identical");
    let ids: Vec<_> = first.iter().map(|profile| profile.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "laravel-postgres-http",
            "node-edge",
            "node-mysql",
            "node-postgres-http"
        ],
        "one snapshot per monorepo profile, in id order"
    );
    let edge = first.iter().find(|p| p.id == "node-edge").expect("edge");
    assert_eq!(
        edge.overrides_applied,
        [
            "deployment.replicas".to_owned(),
            "deployment.reproducible".to_owned()
        ]
    );
    let mysql = first.iter().find(|p| p.id == "node-mysql").expect("mysql");
    assert_eq!(mysql.overrides_applied, ["storage.pooling".to_owned()]);
    assert_eq!(
        mysql.capability("storage.pooling"),
        Some(Support::Partial),
        "the explicit override records the weaker accepted state"
    );
}

#[test]
fn the_resolved_snapshot_matches_the_committed_golden() {
    let node = resolve_fixture("valid/node.json");
    let golden = read("valid/resolved/node-postgres-http.expect.json");
    let expected: serde_json::Value = serde_json::from_slice(&golden).expect("golden json");
    let projected =
        serde_json::to_value(&node).expect("the resolved snapshot is a closed serializable value");
    assert_eq!(
        projected, expected,
        "resolved projection matches the golden"
    );
}

#[test]
fn decode_level_refusals_reject_every_invalid_fixture() {
    for name in [
        "bad-axis.json",
        "bad-id-grammar.json",
        "bad-version-format.json",
        "empty-profiles.json",
        "override-missing-accept.json",
        "unknown-member.json",
        "version-0-0-0.json",
        "wrong-schema-version.json",
    ] {
        let failure = document::decode(&read(&format!("invalid/{name}")));
        assert!(
            matches!(failure, Err(ProfileFailure::DocumentInvalid { .. })),
            "{name} must refuse as document-invalid"
        );
    }
}

#[test]
fn semantic_refusals_carry_the_registered_rules() {
    let mut checked = 0;
    for entry in std::fs::read_dir(format!("{FIXTURES}/semantic")).expect("semantic dir") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        if !name.ends_with(".json") || name.ends_with(".expect.json") {
            continue;
        }
        let expect: serde_json::Value = serde_json::from_slice(&read(&format!(
            "semantic/{}",
            name.replace(".json", ".expect.json")
        )))
        .expect("expect json");
        let expected_rule = expect["rule"].as_str().expect("rule token");
        // Some vectors refuse at decode (document-invalid), the rest at
        // resolution; both must land on the committed rule.
        let rule = match document::decode(&read(&format!("semantic/{name}"))) {
            Err(failure) => failure.rule().to_owned(),
            Ok(document) => resolve(&document)
                .expect_err("the vector must refuse")
                .rule()
                .to_owned(),
        };
        assert_eq!(
            rule, expected_rule,
            "{name} refuses with the committed rule"
        );
        checked += 1;
    }
    assert!(checked > 0, "semantic vectors are present");
}

#[test]
fn portability_between_node_and_laravel_names_the_changed_components() {
    let node = resolve_fixture("valid/node.json");
    let laravel = resolve_fixture("valid/laravel.json");
    let report = portability(&node, &laravel);
    assert_eq!(report.axes.len(), 6);
    for entry in &report.axes {
        let expected = match entry.axis {
            Axis::Storage | Axis::Transport | Axis::Deployment => Verdict::Reused,
            Axis::Runtime | Axis::Testing | Axis::Analysis => Verdict::Changed,
        };
        assert_eq!(entry.verdict, expected, "{:?} verdict", entry.axis);
    }
    let runtime = report
        .axes
        .iter()
        .find(|entry| entry.axis == Axis::Runtime)
        .expect("runtime entry");
    assert!(
        runtime
            .capability_changes
            .iter()
            .any(|change| change.id == "runtime.async"
                && change.source.map(|support| support.as_str()) == Some("full")
                && change.target.map(|support| support.as_str()) == Some("partial")),
        "the report names the async weakening"
    );
}
