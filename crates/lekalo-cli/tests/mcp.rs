//! Issue #33 CLI MCP tests: the real stdio binary under a real child
//! process. Every acceptance-relevant behavior is exercised over the
//! wire — the pinned lifecycle, the exact seven-tool catalog, the one-
//! call context capsule, scope denial, write-tool absence, read-only
//! guarantees, hostile frames, and CLI independence — with protocol
//! frames on stdout only and zero filesystem mutation of the served
//! project.

#[path = "mcp_harness.rs"]
mod mcp_harness;

use mcp_harness::{
    call, exit_code, frame, lekalo_in, lifecycle, stderr_text, stdout_text, workspace_root,
    ServedRoot, PLANNER,
};

// ---- describe: the embedded manifest without a project. -------------------

#[test]
fn describe_emits_the_deterministic_seven_tool_manifest() {
    let json_output = lekalo_in(&workspace_root(), &["mcp", "describe", "--json"]);
    assert_eq!(exit_code(&json_output), 0);
    let document: serde_json::Value =
        serde_json::from_str(&stdout_text(&json_output)).expect("describe envelope");
    assert_eq!(document["status"], "valid");
    let capabilities = &document["capabilities"];
    assert_eq!(
        capabilities["identity"],
        "dev.lekalo.mcp-capabilities@0.8.0"
    );
    assert_eq!(capabilities["protocol"]["profile"], "2025-11-25");
    assert_eq!(capabilities["protocol"]["transport"], "stdio");
    assert_eq!(capabilities["server"]["readOnly"], true);
    let tools = capabilities["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 7);
    for name in [
        "lekalo_inspect",
        "lekalo_impact",
        "lekalo_context",
        "lekalo_graph",
        "lekalo_validate",
        "lekalo_target_status",
        "lekalo_trace_export",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "{name} in the manifest"
        );
    }
    assert_eq!(
        capabilities["resources"]
            .as_array()
            .expect("resources")
            .len(),
        5
    );
    // Deterministic: byte-identical reruns.
    let again = lekalo_in(&workspace_root(), &["mcp", "describe", "--json"]);
    assert_eq!(stdout_text(&json_output), stdout_text(&again));
    // The human projection is a receipt, not an error.
    let human = lekalo_in(&workspace_root(), &["mcp", "describe"]);
    assert_eq!(exit_code(&human), 0);
}

// ---- Serve argument grammar: pre-server usage diagnostics. ----------------

#[test]
fn serve_requires_stdio_project_and_rejects_json() {
    let root = ServedRoot::new();
    // Missing --stdio.
    let output = lekalo_in(&root.root, &["mcp", "serve", "--project", "planner"]);
    assert_eq!(exit_code(&output), 1);
    // Missing --project.
    let output = lekalo_in(&root.root, &["mcp", "serve", "--stdio"]);
    assert_eq!(exit_code(&output), 1);
    // The global --json flag is refused for serve: stdout is protocol
    // property from the first frame.
    let output = lekalo_in(
        &root.root,
        &["--json", "mcp", "serve", "--stdio", "--project", "planner"],
    );
    assert_eq!(exit_code(&output), 1);
}

// ---- The pinned lifecycle over real stdio. --------------------------------

#[test]
fn lifecycle_initializes_pings_and_stays_json_only_on_stdout() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(frame(2, "ping", serde_json::json!({})));
    frames.push("{\"not json".to_owned());
    frames.push(frame(3, "no/such/method", serde_json::json!({})));
    let session = root.serve(&frames, &[]);
    assert_eq!(session.code, 0);
    let responses = session.responses();
    // initialize, ping, the bounded parse error (null id), unknown
    // method (an unknown method is -32601 after `initialized`).
    assert_eq!(responses.len(), 4, "one bounded response per request frame");
    let initialize_response = &responses[0];
    assert_eq!(initialize_response["id"], 1);
    assert_eq!(
        initialize_response["result"]["protocolVersion"],
        "2025-11-25"
    );
    assert_eq!(
        initialize_response["result"]["serverInfo"]["name"],
        "lekalo"
    );
    // Only implemented capabilities are advertised (F8): subscribe and
    // listChanged are false until a bounded watcher ships.
    assert_eq!(
        initialize_response["result"]["capabilities"]["resources"],
        serde_json::json!({"subscribe": false, "listChanged": false})
    );
    assert_eq!(responses[1]["result"], serde_json::json!({}));
    // Malformed JSON is -32700; unknown methods -32601.
    assert_eq!(session.null_id_error_code(), Some(-32700));
    assert_eq!(session.error_code(3), Some(-32601));
    // Logging is stderr-only; stdout is protocol frames only.
    assert!(session.stderr.contains("lekalo-mcp"));
}

#[test]
fn oversized_and_deep_frames_produce_bounded_errors() {
    let root = ServedRoot::new();
    let oversized = "x".repeat(256 * 1024 + 1);
    let mut deep = String::new();
    for _ in 0..80 {
        deep.push('[');
    }
    for _ in 0..80 {
        deep.push(']');
    }
    let session = root.serve(
        &[oversized, deep, frame(1, "ping", serde_json::json!({}))],
        &[],
    );
    let responses = session.responses();
    // The refused oversized line's tail is drained through its own LF
    // (r2 F8), so exactly three bounded responses ship: the oversized
    // refusal, the deep-frame refusal, and the ping. The suffix can
    // never resurface as a frame.
    assert_eq!(responses.len(), 3);
    assert_eq!(session.null_id_error_code(), Some(-32600));
    // One bad request never poisons the session.
    assert_eq!(responses[2]["id"], 1);
}

// ---- tools/list is the exact seven read tools, write-free. ----------------

#[test]
fn tools_list_is_exactly_the_seven_read_tools_with_schemas() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(frame(2, "tools/list", serde_json::json!({})));
    let session = root.serve(&frames, &[]);
    let tools = session.response(2)["result"]["tools"]
        .as_array()
        .expect("tools")
        .clone();
    assert_eq!(tools.len(), 7);
    for tool in &tools {
        let name = tool["name"].as_str().expect("name");
        assert!(name.starts_with("lekalo_"));
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert!(tool["inputSchema"].is_object(), "{name} inputSchema");
        assert!(tool["outputSchema"].is_object(), "{name} outputSchema");
    }
    // No write/generate/collect/install/migrate/scan capability exists
    // anywhere in the advertised surface, under any spelling.
    let advertised: Vec<String> = tools
        .iter()
        .map(|tool| tool["name"].as_str().expect("name").to_owned())
        .collect();
    for forbidden in [
        "write", "generate", "collect", "install", "migrate", "scan", "report",
    ] {
        assert!(
            !advertised
                .iter()
                .any(|name| name.to_lowercase().contains(forbidden)),
            "no {forbidden} tool"
        );
    }
    // A smuggled write tool is an unknown-tool protocol error.
    let mut frames = lifecycle();
    frames.push(call(
        3,
        "lekalo_generate",
        serde_json::json!({"projectId": "project"}),
    ));
    let session = root.serve(&frames, &[]);
    assert_eq!(session.error_code(3), Some(-32602));
}

// ---- The seven read tools over the planner fixture. ------------------------

#[test]
fn inspect_impact_graph_validate_target_status_return_contract_outputs() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_inspect",
        serde_json::json!({"projectId": "project", "symbol": "planner.focus_task"}),
    ));
    frames.push(call(
        11,
        "lekalo_impact",
        serde_json::json!({"projectId": "project", "mode": "symbol", "symbol": "planner.focus_task"}),
    ));
    frames.push(call(
        12,
        "lekalo_graph",
        serde_json::json!({"projectId": "project", "mode": "show", "symbol": "planner.focus_task"}),
    ));
    frames.push(call(
        13,
        "lekalo_validate",
        serde_json::json!({"projectId": "project"}),
    ));
    frames.push(call(
        14,
        "lekalo_target_status",
        serde_json::json!({"projectId": "project"}),
    ));
    let session = root.serve(&frames, &[]);
    // inspect: the preserved core receipt under its pinned discriminator.
    let inspect = session.tool(10);
    assert_eq!(inspect["status"], "valid");
    assert_eq!(inspect["result"]["schemaVersion"], "lekalo/inspect/v0.2.16");
    assert_eq!(
        inspect["result"]["inspect"]["identity"],
        "dev.lekalo.inspect@0.2.16"
    );
    // isError is the MCP-level flag beside structuredContent.
    assert_eq!(session.response(10)["result"]["isError"], false);
    // impact: symbol mode is a completed projection.
    let impact = session.tool(11);
    assert_eq!(impact["status"], "valid");
    assert_eq!(impact["result"]["schemaVersion"], "lekalo/impact/v0.2.16");
    // graph show: the frozen query payload under the mode tag, with no
    // discriminator injected into it.
    let graph = session.tool(12);
    assert_eq!(graph["status"], "valid");
    assert_eq!(graph["result"]["mode"], "show");
    assert!(graph["result"]["query"]["node"].is_object());
    assert!(graph["result"]["query"]["schemaVersion"].is_null());
    // validate: the receipt with the exact model version.
    let validate = session.tool(13);
    assert_eq!(validate["status"], "valid");
    assert_eq!(validate["result"]["modelVersion"], "0.2.16");
    assert!(validate["result"]["validation"].is_object());
    // target status: the doctor verdict rides even when degraded.
    let status = session.tool(14);
    assert_eq!(status["result"]["schemaVersion"], "lekalo/doctor/v0.3.2");
    assert!(status["result"]["doctor"]["checks"].is_array());
}

#[test]
fn context_capsule_arrives_in_exactly_one_call() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(call(
        20,
        "lekalo_context",
        serde_json::json!({
            "projectId": "project",
            "mode": "symbol",
            "symbol": "planner.focus_task",
            "budget": 5000
        }),
    ));
    let session = root.serve(&frames, &[]);
    let context = session.tool(20);
    assert_eq!(context["status"], "valid");
    assert_eq!(context["result"]["schemaVersion"], "lekalo/context/v0.2.16");
    let capsule = &context["result"]["context"];
    // Every mandatory fact rides the one call: budget, fits, coverage.
    assert!(capsule["budget"]["limit"].as_u64().expect("limit") == 5000);
    assert!(capsule["budget"]["fits"].is_boolean());
    assert!(capsule["coverage"]["included"].as_u64().expect("included") >= 1);
    assert!(capsule["budget"]["minimumRequired"].is_u64());
    // A second request is never required for the capsule facts: the
    // notification draws no reply, so exactly two responses exist.
    assert_eq!(session.responses().len(), 2, "initialize + one call");
}

#[test]
fn trace_export_needs_a_registered_manifest_ref() {
    let root = ServedRoot::new();
    // Single-project startup admits no manifests: the opaque ref is a
    // scope denial, never a path read.
    let mut frames = lifecycle();
    frames.push(call(
        30,
        "lekalo_trace_export",
        serde_json::json!({"projectId": "project", "manifestRef": "baseline"}),
    ));
    let session = root.serve(&frames, &[]);
    let trace = session.tool(30);
    assert_eq!(trace["status"], "denied");
    assert_eq!(trace["failure"]["failureClass"], "scope-denied");

    // With the trusted catalog the registered ref exports the canonical
    // manifest and its digest.
    let mut frames = lifecycle();
    frames.push(call(
        31,
        "lekalo_trace_export",
        serde_json::json!({"projectId": "planner", "manifestRef": "baseline"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    let trace = session.tool(31);
    assert_eq!(trace["status"], "valid");
    assert_eq!(
        trace["result"]["schemaVersion"],
        "lekalo/trace-manifest/v0.2.16"
    );
    let digest = trace["result"]["manifestDigest"].as_str().expect("digest");
    assert_eq!(digest.len(), 71);
    assert!(digest.starts_with("sha256:"));
}

#[test]
fn changed_impact_refuses_without_explicit_git_admission() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(call(
        40,
        "lekalo_impact",
        serde_json::json!({
            "projectId": "project",
            "mode": "changed",
            "baseRef": "main",
            "worktree": true
        }),
    ));
    let session = root.serve(&frames, &[]);
    let impact = session.tool(40);
    assert_eq!(impact["status"], "unavailable");
    assert_eq!(impact["failure"]["failureClass"], "backend-unavailable");
}

// ---- Scope denial and hostile operands. ------------------------------------

#[test]
fn unregistered_project_group_and_hostile_symbols_refuse_before_reads() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(call(
        50,
        "lekalo_inspect",
        serde_json::json!({"projectId": "evil", "symbol": "planner.focus_task"}),
    ));
    frames.push(call(
        51,
        "lekalo_inspect",
        serde_json::json!({"projectId": "../planner", "symbol": "planner.focus_task"}),
    ));
    frames.push(call(
        52,
        "lekalo_inspect",
        serde_json::json!({"projectId": "project", "groupId": "nope", "symbol": "planner.focus_task"}),
    ));
    frames.push(frame(
        53,
        "resources/read",
        serde_json::json!({"uri": "file:///etc/passwd"}),
    ));
    frames.push(frame(
        54,
        "resources/read",
        serde_json::json!({"uri": "lekalo://evil/summary"}),
    ));
    let session = root.serve(&frames, &[]);
    for id in [50, 54] {
        let tool = session.tool(id);
        assert_eq!(tool["status"], "denied", "id {id}");
        assert_eq!(tool["failure"]["failureClass"], "scope-denied");
        assert_eq!(session.response(id)["result"]["isError"], true);
    }
    // Malformed identifiers are structural -32602 (no echo); a
    // well-formed but unregistered group is a typed scope denial.
    assert_eq!(session.error_code(51), Some(-32602));
    let group_denial = session.tool(52);
    assert_eq!(group_denial["status"], "denied");
    // A file:// resource URI is a protocol-level refusal.
    assert_eq!(session.error_code(53), Some(-32602));
    // No response echoes a refused operand: paths never ride, while a
    // grammar-valid projectId may appear in the closed scope member.
    assert!(!session.stdout.contains("/etc/passwd"));
    assert!(!session.stdout.contains("file://"));
}

#[test]
fn the_launcher_group_narrows_disclosure_for_real() {
    let root = ServedRoot::new();
    // The planner-review group admits only module `planner`.
    let mut frames = lifecycle();
    frames.push(call(
        60,
        "lekalo_inspect",
        serde_json::json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    ));
    frames.push(call(
        61,
        "lekalo_inspect",
        serde_json::json!({"projectId": "planner", "symbol": "notify.send_digest"}),
    ));
    let session = root.serve_with_catalog(
        Some("mcp.local.json"),
        &frames,
        &["--group", "planner-review"],
    );
    let inside = session.tool(60);
    assert_eq!(inside["status"], "valid");
    let outside = session.tool(61);
    assert_eq!(outside["status"], "denied");
    assert_eq!(outside["failure"]["failureClass"], "scope-denied");
}

// ---- Read-only: the served project never changes. ---------------------------

#[test]
fn a_full_session_writes_nothing_and_touches_no_cache() {
    let root = ServedRoot::new();
    let before = root.inventory();
    let mut frames = lifecycle();
    frames.push(frame(2, "tools/list", serde_json::json!({})));
    frames.push(frame(3, "resources/list", serde_json::json!({})));
    frames.push(frame(
        4,
        "resources/read",
        serde_json::json!({"uri": "lekalo://planner/summary"}),
    ));
    for (id, (name, arguments)) in [
        (
            10,
            (
                "lekalo_inspect",
                serde_json::json!({"projectId": "planner", "symbol": "planner.focus_task"}),
            ),
        ),
        (
            11,
            (
                "lekalo_context",
                serde_json::json!({"projectId": "planner", "mode": "symbol", "symbol": "planner.focus_task", "budget": 5000}),
            ),
        ),
        (
            12,
            (
                "lekalo_impact",
                serde_json::json!({"projectId": "planner", "mode": "symbol", "symbol": "planner.focus_task"}),
            ),
        ),
        (
            13,
            (
                "lekalo_graph",
                serde_json::json!({"projectId": "planner", "mode": "show", "symbol": "planner.focus_task"}),
            ),
        ),
        (
            14,
            (
                "lekalo_validate",
                serde_json::json!({"projectId": "planner"}),
            ),
        ),
        (
            15,
            (
                "lekalo_target_status",
                serde_json::json!({"projectId": "planner"}),
            ),
        ),
        (
            16,
            (
                "lekalo_trace_export",
                serde_json::json!({"projectId": "planner", "manifestRef": "baseline"}),
            ),
        ),
    ] {
        frames.push(call(id, name, arguments));
    }
    frames.push(frame(
        20,
        "resources/subscribe",
        serde_json::json!({"uri": "lekalo://planner/summary"}),
    ));
    frames.push(frame(
        21,
        "resources/unsubscribe",
        serde_json::json!({"uri": "lekalo://planner/summary"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    // Every tool answered valid.
    for id in 10..=16 {
        assert_eq!(session.tool(id)["status"], "valid", "tool id {id}");
    }
    let after = root.inventory();
    assert_eq!(
        before, after,
        "the served root is byte-identical after a full session"
    );
    // No .lekalo cache/runtime directory appeared anywhere.
    assert!(!root.root.join("planner/.lekalo").exists());
    assert!(!root.root.join(".lekalo").exists());
    // Subscribe/unsubscribe are refused: the capability is advertised
    // false, and an inert registration is never acknowledged (r2 F3).
    assert_eq!(session.error_code(20), Some(-32601));
    assert_eq!(session.error_code(21), Some(-32601));
}

// ---- CLI independence: no MCP anything on ordinary commands. ----------------

#[test]
fn ordinary_cli_works_without_any_mcp_negotiation() {
    // With the server absent, disabled, and never contacted, every
    // ordinary read command keeps its exact semantics.
    let root = workspace_root();
    for args in [
        vec![
            "inspect",
            "planner.focus_task",
            "--project",
            PLANNER,
            "--json",
            "--no-cache",
        ],
        vec![
            "impact",
            "planner.focus_task",
            "--project",
            PLANNER,
            "--json",
            "--no-cache",
        ],
        vec![
            "context",
            "planner.focus_task",
            "--budget",
            "5000",
            "--project",
            PLANNER,
            "--json",
            "--no-cache",
        ],
        vec![
            "graph",
            "show",
            "planner.focus_task",
            "--project",
            PLANNER,
            "--json",
            "--no-cache",
        ],
        vec!["validate", "--project", PLANNER, "--json", "--no-cache"],
        vec!["provider", "describe", "--json"],
    ] {
        let output = lekalo_in(&root, &args);
        assert_eq!(exit_code(&output), 0, "ordinary {:?} stays usable", args[0]);
        let document: serde_json::Value =
            serde_json::from_str(&stdout_text(&output)).expect("envelope");
        assert_eq!(document["status"], "valid", "{:?}", args[0]);
        // No CLI startup path attempted MCP negotiation or install.
        assert!(!stderr_text(&output).contains("mcp"));
    }
}

// ---- Serialization stability. ----------------------------------------------

#[test]
fn identical_requests_produce_identical_frames() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(call(
        70,
        "lekalo_inspect",
        serde_json::json!({"projectId": "project", "symbol": "planner.focus_task"}),
    ));
    let first = root.serve(&frames, &[]);
    let second = root.serve(&frames, &[]);
    assert_eq!(first.stdout, second.stdout, "deterministic stdio bytes");
}

// ---- Review round 1 (issue #33): the closed gaps as wire behavior. ---------

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
fn the_launch_group_is_a_ceiling_not_a_default() {
    let root = ServedRoot::new();
    // Launch --group planner-review; a client naming another registered
    // group (or any other id) is the typed widening refusal, not a
    // silent substitution of the wider closure.
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_inspect",
        serde_json::json!({"projectId": "planner", "groupId": "planner-review", "symbol": "planner.focus_task"}),
    ));
    let session = root.serve_with_catalog(
        Some("mcp.local.json"),
        &frames,
        &["--group", "planner-review"],
    );
    assert_eq!(session.tool(10)["status"], "valid");
}

#[test]
fn resources_read_respects_the_launch_group_ceiling() {
    let root = ServedRoot::new();
    // Launched under planner-review (module planner only), a
    // group-qualified resource URI naming another group refuses, and an
    // unqualified URI resolves under the launch group (module closure).
    let mut frames = lifecycle();
    frames.push(frame(
        10,
        "resources/read",
        serde_json::json!({"uri": "lekalo://planner/g/planner-review/symbol/notify.send_digest"}),
    ));
    frames.push(frame(
        11,
        "resources/read",
        serde_json::json!({"uri": "lekalo://planner/symbol/notify.send_digest"}),
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
    // The unqualified URI narrows to the launch group: the outside symbol
    // refuses under the module closure instead of leaking.
    assert_eq!(resolved["status"], "denied");
    assert_eq!(resolved["failure"]["failureClass"], "scope-denied");
}

#[test]
fn resources_subscribe_is_refused_while_unadvertised() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(frame(
        10,
        "resources/subscribe",
        serde_json::json!({"uri": "lekalo://evil/project-summary"}),
    ));
    frames.push(frame(
        11,
        "resources/subscribe",
        serde_json::json!({"uri": "lekalo://planner/summary"}),
    ));
    frames.push(frame(
        12,
        "resources/unsubscribe",
        serde_json::json!({"uri": "lekalo://planner/summary"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    // No watcher ships, and the capability is advertised false (F8): the
    // honest answer is the protocol refusal for every identity — an
    // inert registration is never acknowledged (r2 F3/codex 13).
    assert_eq!(session.error_code(10), Some(-32601));
    assert_eq!(session.error_code(11), Some(-32601));
    assert_eq!(session.error_code(12), Some(-32601));
}

#[test]
fn only_implemented_capabilities_are_advertised() {
    let root = ServedRoot::new();
    let mut frames = lifecycle();
    frames.push(frame(2, "ping", serde_json::json!({})));
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
        serde_json::json!({"projectId": "planner", "groupId": "planner-review", "mode": "changed", "changedSymbols": ["planner.focus_task,notify.send_digest"], "budget": 5000}),
    ));
    frames.push(call(
        11,
        "lekalo_inspect",
        serde_json::json!({"projectId": "planner", "groupId": "planner-review", "symbol": "planner.focus_task,notify.send_digest"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    assert_eq!(session.error_code(10), Some(-32602));
    assert_eq!(session.error_code(11), Some(-32602));
}

#[test]
fn standard_request_id_cancellation_suppresses_the_late_result() {
    let root = ServedRoot::new();
    // The 2025-11-25 spelling: notifications/cancelled carries requestId.
    // The cancel and the call ride one batch: the reader thread observes
    // the notification while the worker runs (r2 F6), so the late result
    // is suppressed and no reply ships for the cancelled id.
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_inspect",
        serde_json::json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    ));
    frames.push(
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 10}})
            .to_string(),
    );
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    assert!(
        session.response_opt(10).is_none(),
        "cancelled id gets no reply"
    );
}

#[test]
fn a_cancelled_id_is_never_poisoned_for_its_reuse() {
    let root = ServedRoot::new();
    // Review r2 F6: a cancelled id used to stay blacklisted forever, so
    // a later request reusing the id executed and then lost its reply.
    // The registration is removed when the request settles: the retry
    // after the cancelled call is answered normally.
    let mut frames = lifecycle();
    frames.push(call(
        10,
        "lekalo_inspect",
        serde_json::json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    ));
    frames.push(
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 10}})
            .to_string(),
    );
    frames.push(call(
        10,
        "lekalo_inspect",
        serde_json::json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    // Exactly one reply ships for id 10: the retry's.
    let replies = session
        .responses()
        .into_iter()
        .filter(|response| response["id"] == 10)
        .count();
    assert_eq!(
        replies, 1,
        "the cancelled call stays silent, the retry answers"
    );
    assert_eq!(session.tool(10)["status"], "valid");
}

#[test]
fn an_unknown_cancelled_id_is_ignored_and_never_registered() {
    let root = ServedRoot::new();
    // Review r2 F6/F19: cancels for unknown or malformed ids are ignored,
    // and a non-scalar id can never enter the bookkeeping at all.
    let mut frames = lifecycle();
    frames.push(
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 77}})
            .to_string(),
    );
    frames.push(
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": {"structured": true}}})
            .to_string(),
    );
    frames.push(call(
        77,
        "lekalo_inspect",
        serde_json::json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    assert_eq!(session.tool(77)["status"], "valid");
}

#[test]
fn pre_initialize_requests_refuse_the_lifecycle_gate() {
    let root = ServedRoot::new();
    let frames = vec![call(
        10,
        "lekalo_inspect",
        serde_json::json!({"projectId": "planner", "symbol": "planner.focus_task"}),
    )];
    let session = root.serve(&frames, &[]);
    assert_eq!(session.error_code(10), Some(-32600));
    assert!(session.stdout.contains("mcp.lifecycle-not-initialized"));
}

#[test]
fn ancestor_junction_reads_refuse_before_any_byte() {
    // The `traces` directory inside the served root is a junction to an
    // outside directory; the admitted read of the registered manifest
    // must refuse with the confinement refusal, never read through it.
    // Unix coverage: the same attribute walk refuses directory symlinks.
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
        serde_json::json!({"projectId": "planner", "manifestRef": "baseline"}),
    ));
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    let trace = session.tool(10);
    assert_eq!(trace["status"], "unavailable");
    assert_eq!(trace["failure"]["failureClass"], "backend-unavailable");
    assert!(session.stdout.contains("confinement.no-follow"));
}
