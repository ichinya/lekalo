//! Issue #33 r4 union regression probes over the real stdio server.
//! Every fixture lives in a disposable served root; no fixture or source
//! file is modified in the checkout.

#[path = "mcp_harness.rs"]
mod mcp_harness;

use mcp_harness::{call, frame, initialize, lifecycle, ServedRoot};
use serde_json::{json, Value};
use std::fs;

fn tool(session: &mcp_harness::SessionOutput, id: i64) -> Value {
    session.response(id)["result"]["structuredContent"].clone()
}

#[test]
fn canonical_credentials_refuse_on_tool_and_resource_surfaces() {
    let root = ServedRoot::new();
    let path = root
        .root
        .join("planner/lekalo/modules/planner/commands.yaml");
    let source = fs::read_to_string(&path).unwrap();
    for canary in [
        "clientSecret=synthcamelsecret1",
        "api-key=synthapikey2",
        "password='synthetic-secret-q3'",
        "D:/PrivateReview/finance.txt",
        "/srv/private/finance.txt",
    ] {
        fs::write(&path, source.replace("Focus one task", canary)).unwrap();
        let mut frames = lifecycle();
        frames.push(call(
            10,
            "lekalo_inspect",
            json!({"projectId":"planner","symbol":"planner.focus_task"}),
        ));
        frames.push(frame(
            11,
            "resources/read",
            json!({"uri":"lekalo://planner/symbol/planner.focus_task"}),
        ));
        let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
        assert_eq!(
            tool(&session, 10)["failure"]["failureClass"],
            "sensitive-output",
            "{canary}"
        );
        assert_eq!(
            session.response(11)["error"]["message"],
            "mcp.sensitive-output",
            "{canary}"
        );
        assert!(
            !session.stdout.contains(canary),
            "no canary can ship: {canary}"
        );
    }
}

#[test]
fn failure_diagnostics_never_cross_the_launch_group() {
    let root = ServedRoot::new();
    let path = root
        .root
        .join("planner/lekalo/modules/notify/entities.yaml");
    let mut source = fs::read_to_string(&path).unwrap();
    source.push_str("  - id: notify.probe_bad\n    kind: value-object\n    version: 1\n    description: \"broken reference probe\"\n    fields:\n      - name: broken\n        type: \"notify.nonexistent_thing\"\n        required: true\n");
    fs::write(path, source).unwrap();
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_inspect",
        json!({"projectId":"planner","symbol":"planner.focus_task"}),
    ));
    frames.push(call(
        11,
        "lekalo_impact",
        json!({"projectId":"planner","mode":"symbol","symbol":"planner.focus_task"}),
    ));
    let session = root.serve_with_catalog(
        Some("mcp.local.json"),
        &frames,
        &["--group", "planner-review"],
    );
    for id in [10, 11] {
        let response = session.response(id);
        assert_eq!(response["result"]["isError"], true, "id {id}");
        assert_ne!(tool(&session, id)["status"], "valid", "id {id}");
    }
    assert!(!session.stdout.contains("notify.probe_bad"));
    assert!(!session.stdout.contains("notify.nonexistent_thing"));
}

#[test]
fn model_and_lock_hardlink_aliases_cannot_serve_outside_bytes() {
    let root = ServedRoot::new();
    let outside = tempfile::tempdir().unwrap();
    let model = root
        .root
        .join("planner/lekalo/modules/planner/commands.yaml");
    let bytes = fs::read_to_string(&model).unwrap();
    let external_model = outside.path().join("commands.yaml");
    fs::write(
        &external_model,
        bytes.replace("Focus one task", "outside-model-canary"),
    )
    .unwrap();
    fs::remove_file(&model).unwrap();
    fs::hard_link(&external_model, &model).unwrap();
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_inspect",
        json!({"projectId":"planner","symbol":"planner.focus_task"}),
    ));
    frames.push(frame(
        11,
        "resources/read",
        json!({"uri":"lekalo://planner/symbol/planner.focus_task"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    assert_ne!(tool(&session, 10)["status"], "valid");
    assert!(session.response(11)["error"].is_object());
    assert!(!session.stdout.contains("outside-model-canary"));

    fs::remove_file(&model).unwrap();
    fs::write(&model, bytes).unwrap();
    let external_lock = outside.path().join("lekalo.lock");
    fs::write(&external_lock, b"outside-lock-canary").unwrap();
    fs::hard_link(&external_lock, root.root.join("planner/lekalo.lock")).unwrap();
    let mut frames = lifecycle();
    frames.push(call(
        12,
        "lekalo_target_status",
        json!({"projectId":"planner"}),
    ));
    frames.push(frame(
        13,
        "resources/read",
        json!({"uri":"lekalo://planner/targets"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    assert_ne!(tool(&session, 12)["status"], "valid");
    assert!(session.response(13)["error"].is_object());
    assert!(!session.stdout.contains("outside-lock-canary"));
}

#[test]
fn every_status_selector_has_real_checks_and_no_empty_ready_receipt() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    for (id, field, selection) in [
        (10, "targetId", "lock"),
        (11, "targetId", "inventory"),
        (12, "targetId", "trust"),
        (13, "profileId", "profiles"),
    ] {
        let mut args = json!({"projectId":"planner"});
        args[field] = json!(selection);
        frames.push(call(id, "lekalo_target_status", args));
    }
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    for (id, expected) in [
        (10, "lock.freshness"),
        (11, "adapters.inventory"),
        (12, "adapters.trust"),
        (13, "capabilities.profiles"),
    ] {
        let output = tool(&session, id);
        assert_eq!(output["status"], "valid", "id {id}");
        let checks = output["result"]["doctor"]["checks"].as_array().unwrap();
        assert!(
            !checks.is_empty(),
            "no fabricated empty ready panel: id {id}"
        );
        assert!(
            checks.iter().any(|check| check["id"] == expected),
            "real panel {expected}"
        );
    }
}

#[test]
fn handshake_and_grammar_refuse_invalid_frames_before_state_or_work() {
    let root = ServedRoot::new();
    let mut frames = vec![
        frame(1, "initialize", json!("garbage")),
        initialize(2),
        json!({"method":"notifications/initialized"}).to_string(),
        frame(3, "tools/list", json!({})),
        json!({"jsonrpc":"1.0","method":"notifications/initialized"}).to_string(),
        frame(4, "tools/list", json!({})),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),
        frame(5, "tools/list", json!({})),
    ];
    for (id, name, args) in [
        (
            10,
            "lekalo_context",
            json!({"projectId":"planner","mode":"symbol","symbol":"planner.focus_task","budget":5000,"changedSymbolCount":0}),
        ),
        (
            11,
            "lekalo_context",
            json!({"projectId":"planner","mode":"symbol","symbol":"planner.focus_task","budget":1000001}),
        ),
        (
            12,
            "lekalo_context",
            json!({"projectId":"planner","mode":"symbol","symbol":"planner.focus/task","budget":5000}),
        ),
        (
            13,
            "lekalo_impact",
            json!({"projectId":"planner","mode":"symbol","symbol":"planner.focus_task","relations":["bad/relation"]}),
        ),
        (
            14,
            "lekalo_impact",
            json!({"projectId":"planner","mode":"symbol","symbol":"planner.focus_task","profile":"UPPER"}),
        ),
    ] {
        frames.push(call(id, name, args));
    }
    frames.push(call(
        15,
        "lekalo_graph",
        json!({"projectId":"planner","mode":"export","symbol":"planner.focus_task"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    assert_eq!(session.error_code(1), Some(-32602));
    assert_eq!(
        session.response(2)["result"]["protocolVersion"],
        "2025-11-25"
    );
    assert_eq!(session.error_code(3), Some(-32600));
    assert_eq!(session.error_code(4), Some(-32600));
    assert_eq!(
        session.response(5)["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        7
    );
    for id in 10..=14 {
        assert_eq!(
            session.error_code(id),
            Some(-32602),
            "schema-invalid id {id}"
        );
    }
    assert_eq!(
        tool(&session, 15)["failure"]["failureClass"],
        "invalid-input"
    );
}

#[test]
fn status_snapshot_ref_moves_with_cache_health_input() {
    let root = ServedRoot::new();
    let frames = [
        lifecycle(),
        vec![call(
            10,
            "lekalo_target_status",
            json!({"projectId":"planner"}),
        )],
    ]
    .concat();
    let first = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    let before = tool(&first, 10);
    assert_eq!(before["status"], "valid");
    let cache = root.root.join("planner/.lekalo/cache/cache.sqlite");
    fs::create_dir_all(cache.parent().unwrap()).unwrap();
    fs::write(&cache, b"synthetic corrupt sqlite store").unwrap();
    let second = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    let after = tool(&second, 10);
    assert_eq!(after["status"], "valid");
    assert_ne!(before["snapshotRef"], after["snapshotRef"]);
    assert_ne!(
        before["result"]["doctor"]["checks"],
        after["result"]["doctor"]["checks"]
    );
}

#[test]
fn status_snapshot_ref_pins_bindings_manifest_and_observed_artifact_bytes() {
    let root = ServedRoot::new();
    let frames = [
        lifecycle(),
        vec![call(
            10,
            "lekalo_target_status",
            json!({"projectId":"planner"}),
        )],
    ]
    .concat();
    let snapshot = || {
        let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
        let output = tool(&session, 10);
        assert_eq!(output["status"], "valid");
        output["snapshotRef"].as_str().unwrap().to_owned()
    };
    let original = snapshot();
    let bindings = root
        .root
        .join("planner/.lekalo/consumer/bindings/projection.json");
    fs::create_dir_all(bindings.parent().unwrap()).unwrap();
    fs::write(&bindings, b"first binding").unwrap();
    let first = snapshot();
    assert_ne!(original, first, "binding creation must move the pin");
    fs::write(&bindings, b"second binding").unwrap();
    let second = snapshot();
    assert_ne!(first, second, "binding content must move the pin");

    let manifest = root
        .root
        .join("planner/.lekalo/generated/manifests/ownership.json");
    let artifact = root.root.join("planner/apps/api/src/planner/focus-task.ts");
    fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/artifacts/wire/valid/one-generated.manifest.json"
        ),
        &manifest,
    )
    .unwrap();
    fs::write(&artifact, b"first observed artifact").unwrap();
    let third = snapshot();
    assert_ne!(second, third, "manifest creation must move the pin");
    fs::write(&artifact, b"second observed artifact").unwrap();
    assert_ne!(third, snapshot(), "artifact bytes must move the pin");
}

#[test]
fn resource_refusals_have_no_foreign_tool_owner() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(frame(
        10,
        "resources/read",
        json!({"uri":"lekalo://evil/summary"}),
    ));
    frames.push(frame(
        11,
        "resources/read",
        json!({"uri":"lekalo://planner/symbol/nonexistent.symbol"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    for id in [10, 11] {
        let response = session.response(id);
        assert!(
            response["error"].is_object(),
            "resource error on its own channel: id {id}"
        );
        assert!(
            response["result"].is_null(),
            "no tools/call envelope: id {id}"
        );
        assert!(!response.to_string().contains("mcp-tool-inspect-output"));
    }
}
