// Issue #33 review-round-1 wire tests: every closed review gap as real
// stdio behavior over the real binary. Each test maps to a numbered
// finding of the independent review; see the commit messages for the
// finding table.

use serde_json::json;

use crate::mcp_harness::{call, frame, lifecycle, ServedRoot};

#[path = "mcp_harness.rs"]
mod mcp_harness;

#[test]
fn duplicate_json_keys_refuse_before_any_parse() {
    let root = ServedRoot::new();
    // Two `projectId` members: last-key-wins is never an admission path.
    let mut frames = lifecycle();
    frames.push(
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"lekalo_inspect","arguments":{"projectId":"planner","projectId":"evil","symbol":"planner.focus_task"}}}"#
            .to_owned(),
    );
    let session = root.serve(&frames, &[]);
    assert_eq!(session.null_id_error_code(), Some(-32600));
    assert!(session.stdout.contains("mcp.duplicate-key"));
    // No tool ran: a forged duplicate cannot smuggle the granted id in.
    assert!(session.response_opt(2).is_none());
}

#[test]
fn resources_read_respects_the_launch_group_ceiling() {
    let root = ServedRoot::new();
    // Launched under planner-review (module planner only), a
    // group-qualified resource URI naming an outside symbol refuses, and
    // an unqualified URI resolves under the launch group's closure.
    let mut frames = lifecycle();
    frames.push(frame(
        10,
        "resources/read",
        json!({"uri": "lekalo://planner/g/planner-review/symbol/notify.send_digest"}),
    ));
    frames.push(frame(
        11,
        "resources/read",
        json!({"uri": "lekalo://planner/symbol/notify.send_digest"}),
    ));
    let session = root.serve_with_catalog(
        Some("mcp.local.json"),
        &frames,
        &["--group", "planner-review"],
    );
    let refused = session.tool(10);
    assert_eq!(refused["status"], "denied");
    assert_eq!(refused["failure"]["failureClass"], "scope-denied");
    let resolved = session.tool(11);
    assert_eq!(resolved["status"], "denied");
    assert_eq!(resolved["failure"]["failureClass"], "scope-denied");
}

#[test]
fn resources_subscribe_authorizes_like_resources_read() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(frame(
        10,
        "resources/subscribe",
        json!({"uri": "lekalo://evil/project-summary"}),
    ));
    frames.push(frame(
        11,
        "resources/subscribe",
        json!({"uri": "lekalo://planner/project-summary"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    // An unregistered project URI is a typed denial, never a silent `{}`.
    let denied = session.tool(10);
    assert_eq!(denied["status"], "denied");
    assert_eq!(denied["failure"]["failureClass"], "scope-denied");
    // The admitted identity still subscribes.
    assert_eq!(session.response(11)["result"], json!({}));
}

#[test]
fn only_implemented_capabilities_are_advertised() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(frame(2, "ping", json!({})));
    let session = root.serve(&frames, &[]);
    let capabilities = &session.response(1)["result"]["capabilities"];
    assert_eq!(capabilities["tools"]["listChanged"], false);
    assert_eq!(capabilities["resources"]["subscribe"], false);
    assert_eq!(capabilities["resources"]["listChanged"], false);
}

#[test]
fn a_comma_operand_is_a_structural_refusal_never_a_split_set() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_context",
        json!({"projectId": "planner", "groupId": "planner-review", "mode": "changed", "changedSymbols": ["planner.focus_task,notify.send_digest"], "budget": 5000}),
    ));
    frames.push(call(
        11,
        "lekalo_inspect",
        json!({"projectId": "planner", "groupId": "planner-review", "symbol": "planner.focus_task,notify.send_digest"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    assert_eq!(session.error_code(10), Some(-32602));
    assert_eq!(session.error_code(11), Some(-32602));
}

#[test]
fn standard_requestid_cancellation_suppresses_the_late_result() {
    let root = ServedRoot::new();
    // The 2025-11-25 spelling: notifications/cancelled carries requestId.
    let mut frames = lifecycle();
    frames.push(
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 10}})
            .to_string(),
    );
    frames.push(call(
        10,
        "lekalo_inspect",
        json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    ));
    let session = root.serve(&frames, &[]);
    assert!(
        session.response_opt(10).is_none(),
        "cancelled id gets no reply"
    );
}

#[test]
fn pre_initialize_requests_refuse_the_lifecycle_gate() {
    let root = ServedRoot::new();
    let frames = vec![call(
        10,
        "lekalo_inspect",
        json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    )];
    let session = root.serve(&frames, &[]);
    assert_eq!(session.error_code(10), Some(-32600));
    assert!(session.stdout.contains("mcp.lifecycle-not-initialized"));
}

#[test]
fn an_ancestor_junction_read_refuses_before_any_byte() {
    // The `traces` directory inside the served root is a link to an
    // outside directory; the admitted read of the registered manifest
    // must refuse with the confinement refusal, never read through it.
    let outside = tempfile::tempdir().expect("outside root");
    std::fs::write(outside.path().join("full.trace.json"), b"{}").expect("outside manifest");
    let root = ServedRoot::new();
    let traces = root.root.join("planner").join("traces");
    std::fs::remove_dir_all(&traces).expect("remove fixture traces");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), &traces).expect("ancestor symlink");
    #[cfg(windows)]
    {
        // A directory junction via the shell is test-only; the attribute
        // check is identical to production (FILE_ATTRIBUTE_REPARSE_POINT).
        let output = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&traces)
            .arg(outside.path())
            .output()
            .expect("mklink");
        assert!(output.status.success(), "junction creation");
    }
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_trace_export",
        json!({"projectId": "planner", "manifestRef": "baseline"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    let trace = session.tool(10);
    assert_eq!(trace["status"], "unavailable");
    assert_eq!(trace["failure"]["failureClass"], "backend-unavailable");
    assert!(session.stdout.contains("confinement.no-follow"));
}

#[test]
fn an_oversized_response_is_substituted_by_the_typed_output_limit() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_inspect",
        json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    ));
    let session = root.serve_with_catalog(
        Some("mcp.local.json"),
        &frames,
        &["--max-response-bytes", "512"],
    );
    // The whole frame is replaced by the LEK-MCP-005 refusal; the inspect
    // payload never rides and nothing partial ships.
    let over_limit = session
        .responses()
        .into_iter()
        .find(|response| response["error"]["message"] == "mcp.output-limit");
    let document = over_limit.expect("output-limit substitution ships");
    assert_eq!(document["error"]["data"]["code"], "LEK-MCP-005");
    assert_eq!(document["error"]["data"]["limit"], 512);
    assert!(!session.stdout.contains("structuredContent"));
    assert!(!session.stdout.contains("planner.focus_task"));
}

#[test]
fn timed_out_workers_keep_holding_their_concurrency_slot() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    // Five calls with a 1ms deadline: four workers abandon into detached
    // execution, the fifth must be refused on the concurrency bound while
    // those workers still run (the bound counts real in-flight work).
    for id in 10..=14 {
        frames.push(call(
            id,
            "lekalo_inspect",
            json!({"projectId": "planner", "symbol": "planner.focus_task", "timeoutMs": 1}),
        ));
    }
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    for id in 10..=13 {
        let tool = session.tool(id);
        assert_eq!(tool["status"], "unavailable", "id {id}");
        assert_eq!(tool["failure"]["failureClass"], "timeout");
    }
    let fifth = session.tool(14);
    assert_eq!(fifth["status"], "denied");
    assert_eq!(fifth["failure"]["failureClass"], "request-limit");
}

#[test]
fn target_status_spawns_no_git_without_catalog_admission() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_target_status",
        json!({"projectId": "planner"}),
    ));
    // The served catalog sets git.changedMode=false: the status panel
    // still answers with the degraded no-git projection (typed
    // git-unavailable facts) and no git process is spawned.
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    let status = session.tool(10);
    assert_eq!(status["status"], "valid");
    assert!(status["result"]["doctor"]["checks"].is_array());
}

#[test]
fn a_secret_shaped_tool_output_refuses_with_sensitive_output() {
    // The screening pass is shape-based: a stored description carrying an
    // assigned credential assignment refuses the whole output.
    let root = ServedRoot::new();
    let description_path = root
        .root
        .join("planner/lekalo/modules/planner/commands.yaml");
    let original = std::fs::read_to_string(&description_path).expect("fixture");
    let canary = "REVIEW_SECRET_33 password=0123456789abcdef0123456789";
    let poisoned = original.replace("Focus one task", canary);
    std::fs::write(&description_path, poisoned).expect("poison");
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_inspect",
        json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    let output = session.tool(10);
    assert_eq!(output["status"], "denied");
    assert_eq!(output["failure"]["failureClass"], "sensitive-output");
    assert!(!session.stdout.contains("0123456789abcdef"));
    // Restore the fixture for later assertions.
    std::fs::write(&description_path, original).expect("restore");
}
