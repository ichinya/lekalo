//! Issue #149: production CLI discovery-scope custody and precedence.
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn put(root: &Path, path: &str, bytes: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repository");
    fs::create_dir(&root).unwrap();
    put(
        &root,
        "package.json",
        r#"{"name":"scope-fixture","workspaces":["apps/*","archives/*"]}"#,
    );
    put(
        &root,
        "apps/backend/composer.json",
        r#"{"name":"synthetic/backend","require":{"laravel/framework":"^11"},"scripts":{"build":"DO-NOT-EXECUTE"}}"#,
    );
    put(
        &root,
        "apps/backend/artisan",
        "<?php // synthetic; never executed\n",
    );
    put(
        &root,
        "apps/frontend/package.json",
        r#"{"name":"synthetic-frontend","devDependencies":{"vue":"1","vite":"1"},"scripts":{"build":"DO-NOT-EXECUTE","test":"DO-NOT-EXECUTE"}}"#,
    );
    put(
        &root,
        "apps/frontend/src/index.ts",
        "export const value = 1;\n",
    );
    put(&root, "apps/frontend/tests/check.ts", "// synthetic\n");
    put(
        &root,
        "archives/old/package.json",
        r#"{"name":"ARCHIVE-TRAP","dependencies":{"react":"1"},"scripts":{"lint":"DO-NOT-EXECUTE"}}"#,
    );
    // Reading this excluded subtree's ignore file must fail the negative
    // control, so a successful scoped run proves it was pruned first.
    put(&root, "archives/.gitignore", "[\n");
    put(
        &root,
        "ignored/package.json",
        r#"{"name":"IGNORED-TRAP","dependencies":{"react":"1"}}"#,
    );
    put(&root, ".env", "PRIVATE-SENTINEL-NOT-FOR-RECEIPTS\n");
    put(
        &root,
        "node_modules/hidden/package.json",
        r#"{"name":"PROTECTED-TRAP"}"#,
    );
    put(&root, ".gitignore", "ignored/\narchives/\n");
    scope(
        &root,
        &["apps/backend", "apps/frontend", "apps/frontend"],
        &["archives"],
    );
    (temp, root)
}

fn scope(root: &Path, includes: &[&str], excludes: &[&str]) {
    put(
        root,
        "discovery.json",
        &serde_json::to_string(&json!({
            "schemaVersion":"lekalo/discovery-scope/v0.9.0",
            "identity":"dev.lekalo.discovery-scope@0.9.0",
            "includes":includes,"excludes":excludes,"gitIgnore":"respect"
        }))
        .unwrap(),
    );
}

fn run(root: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .current_dir(root)
        .env_remove("LEKALO_PROJECT")
        .args([
            "--json",
            "init",
            "--adopt",
            "--project",
            ".",
            "--discovery-scope",
            "discovery.json",
            "--project-id",
            "mixed",
        ])
        .args(extra)
        .output()
        .unwrap()
}

fn value(output: &Output) -> Value {
    let bytes = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    serde_json::from_slice(bytes)
        .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(bytes)))
}

fn valid(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let result = value(output);
    assert_eq!(result["status"], "valid");
    result
}

fn inventory(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let relative = entry
                .path()
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if relative == "lekalo" {
                continue;
            }
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), out);
            } else {
                out.insert(relative, fs::read(entry.path()).unwrap());
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

#[test]
fn mixed_repository_prunes_archives_and_preserves_sources_across_preview_apply_and_replay() {
    let (_temp, root) = fixture();
    let before = inventory(&root);
    let preview = valid(&run(&root, &["--dry-run"]));
    assert!(!serde_json::to_string(&preview).unwrap().contains(".env"));
    assert!(!root.join("lekalo").exists());
    let detection = &preview["detection"];
    let manifests: Vec<&str> = detection["manifests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        manifests,
        ["apps/backend/composer.json", "apps/frontend/package.json"]
    );
    assert_eq!(
        detection["scope"]["includes"],
        json!(["apps/backend", "apps/frontend"])
    );
    assert_eq!(detection["scope"]["complete"], true);
    assert!(detection["scope"]["ignoreFiles"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| !v["path"].as_str().unwrap().starts_with("archives/")));
    assert!(detection["languages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["label"] != "react"));
    assert!(detection["gates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["evidence"].as_str().unwrap().starts_with("apps/")));
    let modules: Vec<&str> = detection["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["path"].as_str().unwrap())
        .collect();
    assert_eq!(modules, ["apps/backend", "apps/frontend"]);
    assert!(
        !String::from_utf8_lossy(&serde_json::to_vec(&preview).unwrap())
            .contains("PRIVATE-SENTINEL")
    );
    let applied = valid(&run(&root, &[]));
    let replay = valid(&run(&root, &[]));
    assert_eq!(applied["changed"], true);
    assert_eq!(replay["changed"], false);
    assert_eq!(
        preview["detection"]["scope"]["digest"],
        applied["detection"]["scope"]["digest"]
    );
    assert_eq!(
        applied["detection"]["scope"]["digest"],
        replay["detection"]["scope"]["digest"]
    );
    assert_eq!(inventory(&root), before);
}

#[test]
fn workspace_globs_cannot_reintroduce_excluded_or_gitignored_members() {
    let (_temp, root) = fixture();
    scope(&root, &["."], &["archives"]);
    let result = valid(&run(&root, &["--dry-run"]));
    let paths: Vec<&str> = result["detection"]["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["apps/backend", "apps/frontend"]);
    assert!(result["detection"]["scope"]["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["path"] == "ignored" && v["reason"] == "gitignored"));
    assert_eq!(result["detection"]["scope"]["complete"], true);
    // Exclusion changes must change the pinned policy, even without source edits.
    scope(&root, &["."], &["archives", "apps/frontend"]);
    let changed = valid(&run(&root, &["--dry-run"]));
    assert_ne!(
        result["detection"]["scope"]["digest"],
        changed["detection"]["scope"]["digest"]
    );
    assert_ne!(
        result["detection"]["scope"]["input"]["sha256"],
        changed["detection"]["scope"]["input"]["sha256"]
    );
    assert!(changed["detection"]["manifests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| !v["path"].as_str().unwrap().contains("frontend")));
}

#[test]
fn gitignore_negation_nested_rules_and_explicit_includes_have_deterministic_precedence() {
    let (_temp, root) = fixture();
    scope(&root, &["."], &["archives", "apps/frontend/blocked"]);
    put(
        &root,
        ".gitignore",
        "archives/\nignored/*\n!ignored/keep/\nnode_modules/\n!node_modules/\n",
    );
    put(&root, "ignored/keep/package.json", r#"{"name":"keep"}"#);
    put(&root, "apps/frontend/.gitignore", "local/\n!blocked/\n");
    put(
        &root,
        "apps/frontend/local/package.json",
        r#"{"name":"NESTED-IGNORED"}"#,
    );
    put(
        &root,
        "apps/frontend/blocked/package.json",
        r#"{"name":"EXCLUDED-DESPITE-NEGATION"}"#,
    );
    let result = valid(&run(&root, &["--dry-run"]));
    let names: Vec<&str> = result["detection"]["manifests"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["path"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"ignored/keep/package.json"));
    assert!(!names
        .iter()
        .any(|v| v.contains("local") || v.contains("blocked") || v.contains("node_modules")));
    // Explicit includes never override the Git ignore decision.
    scope(&root, &["apps/frontend/local"], &["archives"]);
    let ignored = valid(&run(&root, &["--dry-run"]));
    assert!(ignored["detection"]["manifests"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn absolute_traversal_and_protected_roots_receive_typed_refusals_without_writes() {
    let (_temp, root) = fixture();
    for path in [
        "/outside",
        "C:/outside",
        "../outside",
        "apps/../frontend",
        "apps\\frontend",
        ".git",
        "node_modules",
        ".env",
    ] {
        scope(&root, &[path], &[]);
        let output = run(&root, &["--dry-run"]);
        assert_eq!(
            output.status.code(),
            Some(3),
            "{path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result = value(&output);
        assert_eq!(result["status"], "denied");
        assert!(
            result["reasonCodes"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v.as_str().unwrap().starts_with("structure.")),
            "{result}"
        );
        assert!(!root.join("lekalo").exists());
    }
}

#[test]
fn excluded_ignore_file_is_an_actual_read_trap_and_scope_is_required_only_for_adoption() {
    let (_temp, root) = fixture();
    put(&root, ".gitignore", "");
    scope(&root, &["."], &[]);
    let unscoped_trap = run(&root, &["--dry-run"]);
    assert!(
        !unscoped_trap.status.success(),
        "negative control must open the invalid archive ignore file"
    );
    scope(&root, &["."], &["archives"]);
    valid(&run(&root, &["--dry-run"]));
    let orphan = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .current_dir(&root)
        .args([
            "--json",
            "init",
            "--discovery-scope",
            "discovery.json",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert_eq!(orphan.status.code(), Some(1));
    assert!(!root.join("lekalo").exists());
}

#[test]
fn builtin_scope_respects_repository_ignore_rules_without_a_new_input() {
    let (_temp, root) = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .current_dir(&root)
        .env_remove("LEKALO_PROJECT")
        .args([
            "--json",
            "init",
            "--adopt",
            "--project",
            ".",
            "--project-id",
            "mixed",
            "--dry-run",
        ])
        .output()
        .unwrap();
    let result = valid(&output);
    assert_eq!(result["detection"]["scope"]["source"], "builtin");
    assert_eq!(result["detection"]["scope"]["input"], Value::Null);
    assert!(result["detection"]["manifests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| !v["path"].as_str().unwrap().starts_with("archives/")
            && !v["path"].as_str().unwrap().starts_with("ignored/")));
    assert!(!root.join("lekalo").exists());
}

#[test]
fn hidden_service_copies_can_be_excluded_and_hidden_application_roots_can_be_selected() {
    let (_temp, root) = fixture();
    for directory in [".orca", ".bench", "debug"] {
        put(
            &root,
            &format!("{directory}/copy/package.json"),
            r#"{"name":"HIDDEN-ARCHIVE","dependencies":{"react":"1"}}"#,
        );
        put(&root, &format!("{directory}/.gitignore"), "[\n");
    }
    scope(&root, &["."], &["archives", ".orca", ".bench", "debug"]);
    let result = valid(&run(&root, &["--dry-run"]));
    assert_eq!(result["detection"]["scope"]["complete"], true);
    assert!(result["detection"]["languages"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["label"] != "react"));
    put(&root, ".client/package.json", r#"{"name":"hidden-client"}"#);
    scope(
        &root,
        &[".client"],
        &["archives", ".orca", ".bench", "debug"],
    );
    let selected = valid(&run(&root, &["--dry-run"]));
    assert_eq!(
        selected["detection"]["manifests"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        selected["detection"]["manifests"][0]["path"],
        ".client/package.json"
    );
}

#[test]
fn invalid_scope_shapes_and_versions_refuse_before_skeleton_writes() {
    let (_temp, root) = fixture();
    let baseline: Value =
        serde_json::from_slice(&fs::read(root.join("discovery.json")).unwrap()).unwrap();
    for (field, bad) in [
        ("schemaVersion", json!("lekalo/discovery-scope/v9.0.0")),
        ("identity", json!("other")),
        ("includes", json!([])),
        ("excludes", json!([7])),
        ("gitIgnore", json!("off")),
        ("unknown", json!(true)),
    ] {
        let mut input = baseline.clone();
        input[field] = bad;
        put(&root, "discovery.json", &input.to_string());
        let output = run(&root, &["--dry-run"]);
        assert_eq!(output.status.code(), Some(1), "{field}");
        assert_eq!(value(&output)["reasonCodes"], json!(["cli.usage"]));
        assert!(!root.join("lekalo").exists());
    }
}

#[test]
fn scoped_root_is_anchored_without_ancestor_manifest_probes() {
    let (temp, root) = fixture();
    put(
        temp.path(),
        "package.json",
        r#"{"name":"outer-workspace","workspaces":["repository"]}"#,
    );
    let result = valid(
        &Command::new(env!("CARGO_BIN_EXE_lekalo"))
            .current_dir(&root)
            .env_remove("LEKALO_PROJECT")
            .args([
                "--json",
                "init",
                "--adopt",
                "--discovery-scope",
                "discovery.json",
                "--project-id",
                "mixed",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(result["detection"]["basis"], "invocation-directory");
    assert!(root.join("lekalo/project.yaml").is_file());
    assert!(!temp.path().join("lekalo").exists());
}

#[test]
fn ignored_root_manifest_cannot_supply_project_identity_and_ignore_changes_update_provenance() {
    let (_temp, root) = fixture();
    let result = valid(
        &Command::new(env!("CARGO_BIN_EXE_lekalo"))
            .current_dir(&root)
            .env_remove("LEKALO_PROJECT")
            .args([
                "--json",
                "init",
                "--adopt",
                "--project",
                ".",
                "--discovery-scope",
                "discovery.json",
                "--dry-run",
            ])
            .output()
            .unwrap(),
    );
    assert_eq!(result["projectIdSource"]["source"], "directory-name");
    assert_eq!(result["projectId"], "repository");
    let previous = valid(&run(&root, &["--dry-run"]));
    put(
        &root,
        ".gitignore",
        "ignored/\narchives/\n# PRIVATE-IGNORE-COMMENT\n",
    );
    let changed = valid(&run(&root, &["--dry-run"]));
    assert_ne!(
        previous["detection"]["scope"]["digest"],
        changed["detection"]["scope"]["digest"]
    );
    assert_ne!(
        previous["detection"]["scope"]["ignoreFiles"],
        changed["detection"]["scope"]["ignoreFiles"]
    );
    assert!(!changed.to_string().contains("PRIVATE-IGNORE-COMMENT"));
}

#[test]
fn depth_and_entry_limits_are_reported_as_partial_discovery() {
    let (_temp, root) = fixture();
    scope(&root, &["."], &["archives"]);
    put(&root, "deep/a/b/c/d/package.json", r#"{"name":"too-deep"}"#);
    let deep = valid(&run(&root, &["--dry-run"]));
    assert_eq!(deep["detection"]["scope"]["complete"], false);
    assert!(deep["detection"]["scope"]["limits"]
        .as_array()
        .unwrap()
        .contains(&json!("depth-limit")));
    assert!(deep["detection"]["manifests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| !v["path"].as_str().unwrap().starts_with("deep/")));
    for i in 0..10_001 {
        put(&root, &format!("bulk/entry-{i:05}"), "");
    }
    put(
        &root,
        "zz-late/package.json",
        r#"{"name":"AFTER-ENTRY-LIMIT"}"#,
    );
    let bounded = valid(&run(&root, &["--dry-run"]));
    assert_eq!(bounded["detection"]["scope"]["complete"], false);
    assert!(bounded["detection"]["scope"]["limits"]
        .as_array()
        .unwrap()
        .contains(&json!("entry-limit")));
    assert!(bounded["detection"]["manifests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| !v["path"].as_str().unwrap().starts_with("zz-late/")));
    assert!(!root.join("lekalo").exists());
}

#[test]
fn rust_workspace_members_with_trailing_commas_stay_inside_the_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    put(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\n  \"crates/live\",\n  \"archives/old\",\n]\n",
    );
    put(
        root,
        "crates/live/Cargo.toml",
        "[package]\nname = \"live\"\nversion = \"0.1.0\"\n",
    );
    put(
        root,
        "archives/old/Cargo.toml",
        "[package]\nname = \"excluded\"\n",
    );
    scope(root, &["."], &["archives"]);
    let result = valid(&run(root, &["--dry-run"]));
    assert_eq!(result["detection"]["modules"][0]["path"], "crates/live");
    assert_eq!(result["detection"]["modules"].as_array().unwrap().len(), 1);
    assert_eq!(
        result["detection"]["workspaceRoots"][0]["members"],
        json!(["crates/live", "archives/old"])
    );
}

#[cfg(any(unix, windows))]
#[test]
fn selected_symlink_or_windows_junction_is_a_typed_refusal() {
    let (temp, root) = fixture();
    let target = temp.path().join("outside");
    put(&target, "package.json", r#"{"name":"OUTSIDE-SENTINEL"}"#);
    let link = root.join("linked");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    {
        let output = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:LEKALO_TEST_LINK -Target $env:LEKALO_TEST_TARGET | Out-Null"])
            .env("LEKALO_TEST_LINK", &link).env("LEKALO_TEST_TARGET", &target).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    scope(&root, &["linked"], &[]);
    let output = run(&root, &["--dry-run"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        value(&output)["reasonCodes"],
        json!(["structure.path-link"])
    );
    assert!(!value(&output).to_string().contains("OUTSIDE-SENTINEL"));
    assert!(!root.join("lekalo").exists());
    assert!(!target.join("lekalo").exists());
}

#[cfg(windows)]
#[test]
fn windows_scope_case_alias_is_refused_before_discovery() {
    let (_temp, root) = fixture();
    put(&root, "CaseAlias/package.json", r#"{"name":"wrong-case"}"#);
    scope(&root, &["CaseAlias"], &[]);
    let correctly_spelled = valid(&run(&root, &["--dry-run"]));
    assert_eq!(
        correctly_spelled["detection"]["manifests"][0]["path"],
        "CaseAlias/package.json"
    );
    scope(&root, &["casealias"], &[]);
    let output = run(&root, &["--dry-run"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        value(&output)["reasonCodes"],
        json!(["structure.selection-alias"])
    );
}

#[test]
fn repository_path_adapter_probe_cannot_bypass_exclusions_or_gitignore() {
    let (_temp, root) = fixture();
    fs::remove_file(root.join("archives/.gitignore")).unwrap();
    let binary_name = if cfg!(windows) {
        "lekalo-target-scopeprobe.exe"
    } else {
        "lekalo-target-scopeprobe"
    };
    put(
        &root,
        &format!("archives/bin/{binary_name}"),
        "NOT-AN-EXECUTABLE-DO-NOT-RUN",
    );
    let path = std::env::join_paths(std::iter::once(root.join("archives/bin")).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    let probe = || {
        valid(
            &Command::new(env!("CARGO_BIN_EXE_lekalo"))
                .current_dir(&root)
                .env_remove("LEKALO_PROJECT")
                .env("PATH", &path)
                .args([
                    "--json",
                    "init",
                    "--adopt",
                    "--project",
                    ".",
                    "--project-id",
                    "mixed",
                    "--discovery-scope",
                    "discovery.json",
                    "--dry-run",
                ])
                .output()
                .unwrap(),
        )
    };
    let found = |receipt: &Value| {
        receipt["detection"]["adapters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["target"] == "scopeprobe" && v["installed"] == true)
    };
    put(&root, ".gitignore", "");
    scope(&root, &["."], &["archives"]);
    assert!(!found(&probe()));
    put(&root, ".gitignore", "archives/\n");
    scope(&root, &["."], &[]);
    assert!(!found(&probe()));
    put(&root, ".gitignore", "");
    assert!(
        found(&probe()),
        "positive control must see the admitted synthetic adapter without executing it"
    );
}
