//! Round-7 MCP hardening regressions on the real tool/resource wire.
#[path = "mcp_harness.rs"]
mod mcp_harness;

use mcp_harness::{call, frame, lifecycle, ServedRoot, SessionOutput};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

const INVENTORY: &str = ".lekalo/adapters/inventory.json";
const REVOCATIONS: &str = ".lekalo/adapters/evidence/revocations.json";

fn trust_frames() -> Vec<String> {
    [
        lifecycle(),
        vec![call(
            10,
            "lekalo_target_status",
            json!({"projectId":"planner", "targetId":"trust"}),
        )],
    ]
    .concat()
}

fn trust(root: &ServedRoot) -> Value {
    root.serve_with_catalog(Some("mcp.local.json"), &trust_frames(), &[])
        .tool(10)
}

fn clean_bytes(logical: &str) -> Vec<u8> {
    if logical == INVENTORY {
        lekalo_core::adapter_package::Inventory::default()
            .to_wire()
            .unwrap()
    } else {
        br#"{"schemaVersion":"lekalo/adapter-revocations/v0.3.2","records":[]}"#.to_vec()
    }
}

#[test]
fn doctor_adapter_state_hardlinks_refuse_before_evaluation() {
    for logical in [INVENTORY, REVOCATIONS] {
        let root = ServedRoot::new();
        let outside = tempfile::tempdir().unwrap();
        let external = outside.path().join("state.json");
        fs::write(&external, clean_bytes(logical)).unwrap();
        let path = root.root.join("planner").join(logical);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::hard_link(&external, &path).unwrap();
        let output = trust(&root);
        assert_eq!(output["status"], "unavailable", "{logical}: {output}");
        assert_eq!(output["failure"]["failureClass"], "backend-unavailable");
    }
}

#[test]
fn doctor_adapter_state_has_bounded_reads() {
    for logical in [INVENTORY, REVOCATIONS] {
        let root = ServedRoot::new();
        let path = root.root.join("planner").join(logical);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::File::create(&path)
            .unwrap()
            .set_len(4 * 1024 * 1024 + 1)
            .unwrap();
        assert_eq!(
            trust(&root)["failure"]["failureClass"],
            "backend-unavailable"
        );
    }
}

#[test]
fn trust_snapshot_pins_presence_content_and_physical_identity() {
    for logical in [INVENTORY, REVOCATIONS] {
        let root = ServedRoot::new();
        let snapshot = || {
            let output = trust(&root);
            assert_eq!(output["status"], "valid", "{output}");
            output["snapshotRef"].as_str().unwrap().to_owned()
        };
        let absent = snapshot();
        let path = root.root.join("planner").join(logical);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let bytes = clean_bytes(logical);
        fs::write(&path, &bytes).unwrap();
        let present = snapshot();
        assert_ne!(absent, present, "{logical} presence");
        fs::write(&path, b"malformed state").unwrap();
        let malformed = snapshot();
        assert_ne!(
            present, malformed,
            "{logical} malformed content is pinned too"
        );
        fs::write(&path, &bytes).unwrap();
        let original_identity = snapshot();
        // Create the replacement while the old file exists, preventing
        // filesystem file-id reuse from weakening this identity control.
        let replacement = path.with_extension("replacement");
        fs::write(&replacement, &bytes).unwrap();
        fs::remove_file(&path).unwrap();
        fs::rename(&replacement, &path).unwrap();
        assert_ne!(
            original_identity,
            snapshot(),
            "{logical} same bytes, new file identity"
        );
    }
}

#[test]
fn residual_credentials_paths_and_dotfiles_refuse_on_both_surfaces() {
    let root = ServedRoot::new();
    let path = root
        .root
        .join("planner/lekalo/modules/planner/commands.yaml");
    let source = fs::read_to_string(&path).unwrap();
    let mut hostile = vec![
        r#"{"password":"'R7CANARY"}"#.to_owned(),
        r#"password='"R7CANARY'"#.to_owned(),
        r#"password="unterminatedR7CANARY"#.to_owned(),
    ];
    for path in [r"\\server\share\R7CANARY.txt", "/var/log/auth.log"] {
        for prefix in ["path=", "see (", "path:", "path;", "\n", "\t"] {
            hostile.push(format!("{prefix}{path}"));
        }
    }
    hostile.extend(
        [
            ".netrc",
            ".npmrc",
            ".pypirc",
            ".git-credentials",
            ".keystore",
        ]
        .map(str::to_owned),
    );
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
    for description in hostile {
        let changed = source.replace(
            "\"Focus one task\"",
            &serde_json::to_string(&description).unwrap(),
        );
        assert_ne!(changed, source, "fixture must inject the description");
        fs::write(&path, changed).unwrap();
        let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
        assert_eq!(
            session.tool(10)["failure"]["failureClass"],
            "sensitive-output",
            "{description}"
        );
        assert_eq!(
            session.response(11)["error"]["message"],
            "mcp.sensitive-output",
            "{description}"
        );
        assert!(!session.stdout.contains("R7CANARY"));
    }
    for description in [
        "https://example.com/a/b",
        "/definitions/20",
        "lekalo/modules/planner/commands.yaml",
        "password=\"\"",
        r#"{"password":""}"#,
    ] {
        fs::write(
            &path,
            source.replace(
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
        assert!(session.response(11)["result"].is_object(), "{description}");
    }
}

#[test]
fn admitted_git_status_never_refreshes_the_index_even_with_inherited_optional_locks() {
    let root = ServedRoot::new();
    let project = root.root.join("planner");
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(&project)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "--quiet"]);
    git(&["-c", "core.autocrlf=false", "add", "."]);
    git(&[
        "-c",
        "user.name=MCP regression",
        "-c",
        "user.email=mcp@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--quiet",
        "-m",
        "fixture",
    ]);
    let config = root.root.join("mcp.local.json");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("\"changedMode\": false", "\"changedMode\": true"),
    )
    .unwrap();
    let model = project.join("lekalo/modules/planner/commands.yaml");
    // Same bytes, changed stat information: normal Git status refreshes
    // the cached stat fields in .git/index in this precise situation.
    fs::File::options()
        .write(true)
        .open(&model)
        .unwrap()
        .set_modified(SystemTime::now() + Duration::from_secs(2))
        .unwrap();
    let index = project.join(".git/index");
    let before = fs::read(&index).unwrap();
    let mtime = fs::metadata(&index).unwrap().modified().unwrap();
    let inventory = root.inventory();
    let frames = [
        lifecycle(),
        vec![call(
            10,
            "lekalo_target_status",
            json!({"projectId":"planner"}),
        )],
    ]
    .concat();
    let mut child = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args([
            "mcp",
            "serve",
            "--stdio",
            "--catalog-config",
            "mcp.local.json",
        ])
        .current_dir(mcp_harness::alias_free(&root.root))
        .env_remove("LEKALO_PROJECT")
        .env("GIT_OPTIONAL_LOCKS", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    for frame in frames {
        writeln!(child.stdin.as_mut().unwrap(), "{frame}").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let session = SessionOutput {
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
        code: output.status.code().unwrap(),
    };
    let output = session.tool(10);
    assert_eq!(output["status"], "valid", "{output}");
    assert_eq!(
        output["result"]["doctor"]["revisions"]["git"]["state"], "ok",
        "{output}"
    );
    assert_eq!(
        output["result"]["doctor"]["revisions"]["git"]["dirty"], false,
        "{output}"
    );
    assert_eq!(
        fs::read(&index).unwrap(),
        before,
        "Git read must not rewrite index bytes"
    );
    assert_eq!(
        fs::metadata(&index).unwrap().modified().unwrap(),
        mtime,
        "Git read must not rewrite even identical index bytes"
    );
    assert_eq!(
        root.inventory(),
        inventory,
        "complete Git-enabled session is read-only"
    );
    // Positive control proves this fixture really exercises an optional
    // refresh write instead of accidentally testing an already-fresh index.
    let status = Command::new("git")
        .current_dir(&project)
        .env("GIT_OPTIONAL_LOCKS", "1")
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(status.status.success());
    assert_ne!(
        fs::read(&index).unwrap(),
        before,
        "fixture must trigger ordinary Git index refresh"
    );
}
