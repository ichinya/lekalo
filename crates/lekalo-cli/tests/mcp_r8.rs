//! Round-8 review regressions through the real MCP tool/resource wire.
#[path = "mcp_harness.rs"]
mod mcp_harness;

use mcp_harness::{alias_free, call, frame, lifecycle, ServedRoot, SessionOutput};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

fn git(project: &Path, args: &[&str]) -> std::process::Output {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=MCP regression",
            "-c",
            "user.email=mcp@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(alias_free(project))
        .output()
        .expect("Git is required by the MCP regression gate");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn init_repo(project: &Path) {
    git(project, &["init", "--quiet"]);
    git(project, &["-c", "core.autocrlf=false", "add", "."]);
    git(project, &["commit", "--quiet", "-m", "fixture"]);
}

fn admit_git(root: &ServedRoot) {
    let config = root.root.join("mcp.local.json");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("\"changedMode\": false", "\"changedMode\": true"),
    )
    .unwrap();
}

fn status_frames() -> Vec<String> {
    [
        lifecycle(),
        vec![
            call(10, "lekalo_target_status", json!({"projectId":"planner"})),
            frame(
                11,
                "resources/read",
                json!({"uri":"lekalo://planner/targets"}),
            ),
        ],
    ]
    .concat()
}

fn resource(session: &SessionOutput) -> Value {
    serde_json::from_str(
        session.response(11)["result"]["contents"][0]["text"]
            .as_str()
            .expect("resource answered"),
    )
    .unwrap()
}

fn assert_git_available(session: &SessionOutput) {
    let output = session.tool(10);
    assert_eq!(output["status"], "valid", "{output}");
    assert_eq!(
        output["result"]["doctor"]["revisions"]["git"]["state"], "ok",
        "{output}"
    );
    assert!(
        session.response(11)["result"].is_object(),
        "{}",
        session.stdout
    );
}

/// Git for Windows also runs shebang scripts for configured helpers.
/// The positive control proves the exact fixture really executes there.
fn helper(directory: &Path, marker: &Path) -> String {
    let script = directory.join("helper.sh");
    let quoted_marker = alias_free(directory)
        .join(marker.file_name().unwrap())
        .display()
        .to_string()
        .replace('\\', "/")
        .replace('\'', "'\\''");
    fs::write(
        &script,
        format!("#!/bin/sh\nprintf started >> '{quoted_marker}'\ncat\n"),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    // The config value is a shell command, not a protocol-controlled argv.
    format!(
        "'{}'",
        alias_free(&script)
            .display()
            .to_string()
            .replace('\\', "/")
            .replace('\'', "'\\''")
    )
}

#[test]
fn repository_fsmonitor_cannot_execute_or_write_outside_the_grant() {
    let root = ServedRoot::new();
    let project = root.root.join("planner");
    init_repo(&project);
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("fsmonitor-marker");
    let command = helper(outside.path(), &marker);
    git(&project, &["config", "core.fsmonitor", &command]);
    git(
        &project,
        &["-c", "core.fsmonitorHookVersion=2", "status", "--porcelain"],
    );
    assert!(
        marker.exists(),
        "the hostile helper control must actually run"
    );
    fs::remove_file(&marker).unwrap();
    admit_git(&root);
    let inventory = root.inventory();
    let started = Instant::now();
    let session = root.serve_with_catalog(Some("mcp.local.json"), &status_frames(), &[]);
    assert_git_available(&session);
    assert!(
        !marker.exists(),
        "MCP must not execute repository-configured helpers"
    );
    assert_eq!(
        root.inventory(),
        inventory,
        "the Git-enabled session remains read-only"
    );
    assert!(started.elapsed() < Duration::from_secs(30));
}

#[test]
fn clean_filters_are_disabled_for_status_and_changed_impact() {
    let root = ServedRoot::new();
    let project = root.root.join("planner");
    init_repo(&project);
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("filter-marker");
    let command = helper(outside.path(), &marker);
    git(&project, &["config", "filter.hostile.clean", &command]);
    git(&project, &["config", "filter.hostile.process", &command]);
    // The positive control selects the one-shot clean helper. The real
    // MCP read below must disable the process form as well.
    git(&project, &["config", "--unset", "filter.hostile.process"]);
    git(&project, &["config", "filter.hostile.required", "true"]);
    fs::write(
        project.join(".git/info/attributes"),
        "*.yaml filter=hostile\n",
    )
    .unwrap();
    let model = project.join("lekalo/modules/planner/commands.yaml");
    let mut source = fs::read_to_string(&model).unwrap();
    source.push_str("\n# filter comparison fixture\n");
    fs::write(&model, source).unwrap();
    git(&project, &["diff", "--name-status", "HEAD"]);
    assert!(
        marker.exists(),
        "a name-only diff really invokes this clean filter"
    );
    fs::remove_file(&marker).unwrap();
    git(&project, &["config", "filter.hostile.process", &command]);
    admit_git(&root);
    let inventory = root.inventory();
    let frames = [
        status_frames(),
        vec![call(
            12,
            "lekalo_impact",
            json!({"projectId":"planner", "mode":"changed", "baseRef":"HEAD", "worktree":true}),
        )],
    ]
    .concat();
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    assert_git_available(&session);
    assert!(session.tool(12)["result"].is_object(), "{}", session.stdout);
    assert!(
        !marker.exists(),
        "status and diff must never start a clean filter"
    );
    assert_eq!(root.inventory(), inventory);
}

#[test]
fn admitted_git_commit_and_dirty_facts_move_tool_and_resource_pins() {
    let root = ServedRoot::new();
    let project = root.root.join("planner");
    init_repo(&project);
    admit_git(&root);
    let read = || {
        let session = root.serve_with_catalog(Some("mcp.local.json"), &status_frames(), &[]);
        assert_git_available(&session);
        (session.tool(10), resource(&session))
    };
    let (before, before_resource) = read();
    assert_eq!(
        before["result"]["doctor"]["revisions"]["git"]["dirty"],
        false
    );
    git(
        &project,
        &[
            "commit",
            "--quiet",
            "--allow-empty",
            "-m",
            "only Git facts changed",
        ],
    );
    let (committed, committed_resource) = read();
    assert_ne!(before["snapshotRef"], committed["snapshotRef"]);
    assert_ne!(before_resource["revision"], committed_resource["revision"]);
    assert_ne!(
        before["result"]["doctor"]["revisions"]["git"]["commit"],
        committed["result"]["doctor"]["revisions"]["git"]["commit"]
    );
    fs::write(project.join("untracked.txt"), "dirty fact only\n").unwrap();
    let (dirty, dirty_resource) = read();
    assert_eq!(dirty["result"]["doctor"]["revisions"]["git"]["dirty"], true);
    assert_ne!(committed["snapshotRef"], dirty["snapshotRef"]);
    assert_ne!(committed_resource["revision"], dirty_resource["revision"]);
}

#[test]
fn ungranted_git_facts_remain_unavailable_and_do_not_change_the_pin() {
    let root = ServedRoot::new();
    let project = root.root.join("planner");
    init_repo(&project);
    let read = || {
        root.serve_with_catalog(Some("mcp.local.json"), &status_frames(), &[])
            .tool(10)
    };
    let before = read();
    git(
        &project,
        &["commit", "--quiet", "--allow-empty", "-m", "not admitted"],
    );
    let after = read();
    assert_eq!(before["snapshotRef"], after["snapshotRef"]);
    assert_eq!(
        after["result"]["doctor"]["revisions"]["git"]["state"],
        "unavailable"
    );
}

#[test]
fn boundary_paths_and_the_hidden_file_class_refuse_on_both_surfaces() {
    let root = ServedRoot::new();
    let model = root
        .root
        .join("planner/lekalo/modules/planner/commands.yaml");
    let original = fs::read_to_string(&model).unwrap();
    let frames = [
        lifecycle(),
        vec![
            call(
                10,
                "lekalo_inspect",
                json!({"projectId":"planner", "symbol":"planner.focus_task"}),
            ),
            frame(
                11,
                "resources/read",
                json!({"uri":"lekalo://planner/symbol/planner.focus_task"}),
            ),
        ],
    ]
    .concat();
    for description in [
        "`/var/log/auth.log`",
        "</opt/company/config.json>",
        "x|/etc/shadow",
        "file:/etc/shadow",
        "file:///var/log/auth.log",
        "file://server/share/file.txt",
        "<\\\\server\\share\\canary>",
        "`\\\\server\\share\\canary`",
        ".pgpass",
        ".my.cnf",
        ".kube/config",
        ".docker/config.json",
        ".boto",
        ".s3cfg",
        ".vault-token",
        ".azure/config",
        ".git/config",
        ".curlrc",
        "nested/.service-auth/settings",
        "password=\"\"R8CANARY",
        "password=''R8CANARY",
    ] {
        fs::write(
            &model,
            original.replace(
                "\"Focus one task\"",
                &serde_json::to_string(description).unwrap(),
            ),
        )
        .unwrap();
        let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
        assert_eq!(
            session.tool(10)["failure"]["failureClass"],
            "sensitive-output",
            "{description}: {}",
            session.stdout
        );
        assert_eq!(
            session.response(11)["error"]["message"],
            "mcp.sensitive-output",
            "{description}: {}",
            session.stdout
        );
        assert!(!session.stdout.contains("R8CANARY"));
    }
    for description in [
        "https://example.test/a/b",
        "lekalo://planner/symbol/x",
        "src/main.rs",
        "planner.focus_task",
        "password=\"\"",
    ] {
        fs::write(
            &model,
            original.replace(
                "\"Focus one task\"",
                &serde_json::to_string(description).unwrap(),
            ),
        )
        .unwrap();
        let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
        assert_eq!(
            session.tool(10)["status"],
            "valid",
            "{description}: {}",
            session.stdout
        );
        assert!(
            session.response(11)["result"].is_object(),
            "{description}: {}",
            session.stdout
        );
    }
}

#[test]
fn integral_decimal_and_exponent_spellings_reach_the_wire_tools() {
    let root = ServedRoot::new();
    let frames = [lifecycle(), vec![
        call(10, "lekalo_context", json!({"projectId":"planner", "mode":"symbol", "symbol":"planner.focus_task", "budget":5000.0})),
        call(11, "lekalo_impact", json!({"projectId":"planner", "mode":"symbol", "symbol":"planner.focus_task", "depth":3e0})),
        call(12, "lekalo_inspect", json!({"projectId":"planner", "symbol":"planner.focus_task", "timeoutMs":1000.0})),
        call(13, "lekalo_context", json!({"projectId":"planner", "mode":"symbol", "symbol":"planner.focus_task", "budget":5000.5})),
    ]].concat();
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    for id in [10, 11, 12] {
        assert_eq!(session.tool(id)["status"], "valid", "{}", session.stdout);
    }
    assert_eq!(session.response(13)["error"]["code"], -32602);
}

#[test]
fn ids_that_cannot_reserve_a_refusal_are_answered_without_black_holes() {
    let root = ServedRoot::new();
    for (id, cap) in [
        ("x".repeat(2008), "2048"),
        ("\u{0001}".repeat(100), "2048"),
        ("x".repeat(400), "512"),
    ] {
        let frames = [
            lifecycle(),
            vec![
                json!({"jsonrpc":"2.0", "id":id, "method":"tools/list", "params":{}}).to_string(),
                frame(20, "ping", json!({})),
            ],
        ]
        .concat();
        let session = root.serve_with_catalog(None, &frames, &["--max-response-bytes", cap]);
        let responses = session.responses();
        let refusal = responses
            .iter()
            .find(|response| response["id"].is_null())
            .expect("oversized id has an explicit bounded refusal");
        assert_eq!(refusal["error"]["code"], -32600);
        assert_eq!(refusal["error"]["message"], "mcp.request-limit");
        assert!(session.response(20)["result"].is_object());
        assert!(session
            .stdout
            .lines()
            .all(|line| line.len() < cap.parse::<usize>().unwrap()));
        assert!(!responses.iter().any(|response| response["id"] == id));
    }
    // An admitted, escape-expanded id still correlates an oversized result.
    let id = "\u{0001}".repeat(50);
    let frames = [
        lifecycle(),
        vec![json!({"jsonrpc":"2.0", "id":id, "method":"tools/list"}).to_string()],
    ]
    .concat();
    let session = root.serve_with_catalog(None, &frames, &["--max-response-bytes", "2048"]);
    let response = session
        .responses()
        .into_iter()
        .find(|response| response["id"] == id)
        .expect("admitted id is correlated");
    assert_eq!(response["error"]["message"], "mcp.output-limit");
}
