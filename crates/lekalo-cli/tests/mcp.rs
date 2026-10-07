//! Issue #33 CLI MCP tests: the real stdio binary under a real child
//! process. Every acceptance-relevant behavior is exercised over the
//! wire — the pinned lifecycle, the exact seven-tool catalog, the one-
//! call context capsule, scope denial, write-tool absence, read-only
//! guarantees, hostile frames, and CLI independence — with protocol
//! frames on stdout only and zero filesystem mutation of the served
//! project.
//!
//! Every child runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const PLANNER: &str = "tests/fixtures/inspect/planner";
const TRACE: &str = "tests/fixtures/trace/full.trace.json";

// ---- Harness. --------------------------------------------------------------

/// The alias-free spelling of one path (the selection policy denies
/// 8.3-alias directories before any command logic).
fn alias_free(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

fn workspace_root() -> PathBuf {
    alias_free(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free_path_free(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

fn alias_free_path_free(dir: &Path) -> PathBuf {
    alias_free(dir)
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

fn exit_code(output: &Output) -> u8 {
    output.status.code().expect("exit code") as u8
}

/// A disposable served root: a fresh copy of the hermetic planner plus
/// the trace manifest under `traces/`, and the trusted catalog that
/// admits it. The server child runs with its CWD at this root.
struct ServedRoot {
    root: PathBuf,
}

impl ServedRoot {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temp root").keep();
        let planner = root.join("planner");
        copy_dir(&workspace_root().join(PLANNER), &planner);
        std::fs::create_dir_all(planner.join("traces")).expect("traces dir");
        std::fs::copy(
            workspace_root().join(TRACE),
            planner.join("traces/full.trace.json"),
        )
        .expect("trace fixture");
        let config = r#"{
  "schemaVersion": "lekalo/mcp-server-config/v0.8.0",
  "identity": "dev.lekalo.mcp-server-config@0.8.0",
  "projects": [{"id": "planner", "root": "planner"}],
  "groups": [{"id": "planner-review", "projectId": "planner", "modules": ["planner"]}],
  "manifests": [{"id": "baseline", "projectId": "planner", "path": "traces/full.trace.json"}],
  "git": {"changedMode": false}
}"#;
        std::fs::write(root.join("mcp.local.json"), config).expect("catalog config");
        Self { root }
    }

    /// The `--project` operand for single-project startup.
    fn selector(&self) -> &'static str {
        "planner"
    }

    /// Run one server session over the given frames (each already a
    /// complete LF-terminated string list), from the served root, with
    /// the plain single-project catalog.
    fn serve(&self, frames: &[String], extra_args: &[&str]) -> SessionOutput {
        self.serve_with_catalog(None, frames, extra_args)
    }

    fn serve_with_catalog(
        &self,
        config: Option<&str>,
        frames: &[String],
        extra_args: &[&str],
    ) -> SessionOutput {
        let mut args: Vec<String> =
            vec!["mcp".to_owned(), "serve".to_owned(), "--stdio".to_owned()];
        match config {
            Some(path) => {
                args.push("--catalog-config".to_owned());
                args.push(path.to_owned());
            }
            None => {
                args.push("--project".to_owned());
                args.push(self.selector().to_owned());
            }
        }
        args.extend(extra_args.iter().map(|argument| (*argument).to_owned()));
        let mut child = Command::new(env!("CARGO_BIN_EXE_lekalo"))
            .args(&args)
            .current_dir(alias_free(&self.root))
            .env_remove("LEKALO_PROJECT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the server");
        {
            let stdin = child.stdin.as_mut().expect("stdin");
            for frame in frames {
                stdin.write_all(frame.as_bytes()).expect("write frame");
                stdin.write_all(b"\n").expect("write frame LF");
            }
        }
        let output = child.wait_with_output().expect("server session");
        SessionOutput {
            stdout: String::from_utf8(output.stdout).expect("stdout utf8"),
            stderr: String::from_utf8(output.stderr).expect("stderr utf8"),
            code: output.status.code().unwrap_or_default(),
        }
    }

    /// The full recursive file inventory of the served root with each
    /// file's sha256. Path spellings are lowercase forward-slash relative
    /// names so the before/after comparison is platform-stable.
    fn inventory(&self) -> Vec<(String, String)> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).expect("read dir") {
                let entry = entry.expect("entry");
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, base, out);
                } else {
                    let bytes = std::fs::read(&path).expect("read file");
                    let relative = path
                        .strip_prefix(base)
                        .expect("inside root")
                        .to_string_lossy()
                        .replace('\\', "/")
                        .to_lowercase();
                    out.push((relative, format!("{:x}", md_slice(&bytes))));
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out);
        out.sort();
        out
    }
}

/// FNV-1a over the bytes: a stable, dependency-free content digest for
/// inventory equality (not a published contract digest).
fn md_slice(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("entry");
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &destination);
        } else {
            std::fs::copy(&path, &destination).expect("copy file");
        }
    }
}

struct SessionOutput {
    stdout: String,
    stderr: String,
    code: i32,
}

impl SessionOutput {
    /// The parsed JSON-RPC responses; stdout must be frames only.
    fn responses(&self) -> Vec<serde_json::Value> {
        let mut parsed = Vec::new();
        for line in self.stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("stdout line is one JSON frame: {error}: {line}"));
            parsed.push(value);
        }
        parsed
    }

    fn response(&self, id: i64) -> serde_json::Value {
        self.responses()
            .into_iter()
            .find(|response| response["id"] == id)
            .unwrap_or_else(|| panic!("no response for id {id}"))
    }

    /// The structured tool output for one tools/call id.
    fn tool(&self, id: i64) -> serde_json::Value {
        self.response(id)["result"]["structuredContent"].clone()
    }

    fn error_code(&self, id: i64) -> Option<i64> {
        self.response(id)["error"]["code"].as_i64()
    }

    /// The error code of the null-id bounded response (over-limit or
    /// malformed frames cannot carry a safe id).
    fn null_id_error_code(&self) -> Option<i64> {
        self.responses()
            .iter()
            .find(|response| response["id"].is_null())
            .and_then(|response| response["error"]["code"].as_i64())
    }
}

fn frame(id: i64, method: &str, params: serde_json::Value) -> String {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

fn initialize(id: i64) -> String {
    frame(
        id,
        "initialize",
        serde_json::json!({"protocolVersion": "2025-11-25"}),
    )
}

fn call(id: i64, name: &str, arguments: serde_json::Value) -> String {
    frame(
        id,
        "tools/call",
        serde_json::json!({"name": name, "arguments": arguments}),
    )
}

fn lifecycle() -> Vec<String> {
    vec![
        initialize(1),
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
    ]
}

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
    // initialize, ping, the bounded parse error (null id), unknown method.
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
    assert!(
        initialize_response["result"]["capabilities"]["resources"]["subscribe"]
            .as_bool()
            .expect("subscribe capability")
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
    assert_eq!(responses.len(), 3);
    assert_eq!(session.null_id_error_code(), Some(-32600));
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
    // graph show: the frozen query payload.
    let graph = session.tool(12);
    assert_eq!(graph["status"], "valid");
    assert!(graph["result"]["graph"]["node"].is_object());
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
    // Subscribe/unsubscribe answered.
    assert_eq!(session.response(20)["result"], serde_json::json!({}));
    assert_eq!(session.response(21)["result"], serde_json::json!({}));
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
