use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn lekalo(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .output()
        .expect("run the real lekalo binary")
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
}

fn version_json() -> String {
    format!("{{\n  \"status\": \"valid\",\n  \"version\": \"{VERSION}\"\n}}\n")
}

/// The exact wire item of one registered data-less diagnostic, pretty at
/// the envelope's four-space depth.
fn diagnostic_item_json(
    id: &str,
    code: &str,
    severity: &str,
    category: &str,
    message: &str,
) -> String {
    format!(
        "{{\n      \"schema_version\": \"lekalo/diagnostic/v1.0.0\",\n      \"registry_version\": \"1.15.0\",\n      \"id\": \"{id}\",\n      \"code\": \"{code}\",\n      \"severity\": \"{severity}\",\n      \"category\": \"{category}\",\n      \"message_id\": \"{id}\",\n      \"message\": \"{message}\",\n      \"data\": {{}},\n      \"related_locations\": [],\n      \"causes\": [],\n      \"fixes\": [],\n      \"metadata\": {{}}\n    }}"
    )
}

fn usage_json() -> String {
    format!(
        "{{\n  \"status\": \"invalid\",\n  \"diagnostics\": [\n    {}\n  ],\n  \"reasonCodes\": [\n    \"cli.usage\"\n  ]\n}}\n",
        diagnostic_item_json(
            "cli.usage",
            "LEK-CLI-001",
            "error",
            "infrastructure",
            "Malformed command-line syntax."
        )
    )
}

fn usage_human() -> &'static str {
    "invalid error [LEK-CLI-001] cli.usage: Malformed command-line syntax.\n"
}

fn assert_json_document(bytes: &[u8]) {
    assert!(bytes.ends_with(b"\n"));
    assert!(!bytes.ends_with(b"\n\n"));
    assert!(!bytes.ends_with(b"\r\n"));
    assert!(!bytes.contains(&b'\r'));

    let documents = serde_json::Deserializer::from_slice(bytes)
        .into_iter::<Value>()
        .collect::<Result<Vec<_>, _>>()
        .expect("valid JSON output");
    assert_eq!(documents.len(), 1);
}

fn assert_no_escaped_argument_leak(output: &Output) {
    for stream in [&output.stdout, &output.stderr] {
        let rendered = String::from_utf8_lossy(stream);
        assert!(!rendered.contains("--json"));
        assert!(!rendered.contains("extra"));
    }
}

#[test]
fn plain_and_json_version_outputs_are_exact_in_both_flag_orders() {
    let plain = lekalo(&["--version"]);
    assert_exit(&plain, 0);
    assert_eq!(plain.stdout, format!("lekalo {VERSION}\n").as_bytes());
    assert!(plain.stderr.is_empty());

    for args in [["--json", "--version"], ["--version", "--json"]] {
        let output = lekalo(&args);
        assert_exit(&output, 0);
        assert_eq!(output.stdout, version_json().as_bytes());
        assert!(output.stderr.is_empty());
        assert_json_document(&output.stdout);
    }
}

#[test]
fn malformed_invocations_are_stable_usage_errors_and_never_exit_two() {
    let json_cases = [
        vec!["--json"],
        vec!["--json", "unknown"],
        vec!["--json", "--unknown"],
        vec!["inspect", "--json"],
        vec!["impact", "--json"],
        vec!["context", "--json"],
        vec!["context", "planner.task", "--json"],
        vec!["context", "planner.task", "--budget", "--json"],
        vec![
            "context",
            "planner.task",
            "--budget",
            "not-a-number",
            "--json",
        ],
        vec!["context", "planner.task", "--budget", "-1", "--json"],
    ];

    for args in json_cases {
        let output = lekalo(&args);
        assert_exit(&output, 1);
        assert_ne!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, usage_json().as_bytes());
        assert_json_document(&output.stderr);
    }

    let no_arguments = lekalo(&[]);
    assert_exit(&no_arguments, 1);
    assert!(no_arguments.stdout.is_empty());
    assert_eq!(no_arguments.stderr, usage_human().as_bytes());
}

#[test]
fn escaped_json_literals_do_not_select_json_output() {
    let inspect = lekalo(&["inspect", "--", "--json"]);
    // The escaped literal becomes the inspect selector; it violates the
    // selector grammar, so it is a stable usage failure with no echo.
    assert_exit(&inspect, 1);
    assert!(inspect.stdout.is_empty());
    assert_eq!(inspect.stderr, usage_human().as_bytes());
    assert_no_escaped_argument_leak(&inspect);

    for args in [
        &["inspect", "--", "--json", "extra"][..],
        &["--", "--json"][..],
    ] {
        let output = lekalo(args);
        assert_exit(&output, 1);
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, usage_human().as_bytes());
        assert_no_escaped_argument_leak(&output);
    }

    let version = lekalo(&["--version", "--", "--json"]);
    assert_exit(&version, 0);
    assert_eq!(version.stdout, format!("lekalo {VERSION}\n").as_bytes());
    assert!(version.stderr.is_empty());
    assert_no_escaped_argument_leak(&version);

    let real_json_flag = lekalo(&["--json", "--", "--json"]);
    assert_exit(&real_json_flag, 1);
    assert!(real_json_flag.stdout.is_empty());
    assert_eq!(real_json_flag.stderr, usage_json().as_bytes());
    assert_json_document(&real_json_flag.stderr);
    assert_no_escaped_argument_leak(&real_json_flag);
}

#[test]
fn root_and_each_command_help_succeed_without_a_failure_envelope() {
    let cases = [
        vec!["--help"],
        vec!["--json", "--help"],
        vec!["validate", "--help"],
        vec!["inspect", "--help"],
        vec!["impact", "--help"],
        vec!["context", "--help"],
    ];

    for args in cases {
        let output = lekalo(&args);
        assert_exit(&output, 0);
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).expect("UTF-8 help");
        assert!(stdout.contains("Usage:"));
        assert!(!stdout.contains("\"status\""));
        assert!(!stdout.contains("cli.usage"));
    }
}

#[test]
fn output_bytes_are_deterministic_and_do_not_leak_environment_or_raw_arguments() {
    let cases = [
        vec!["--json", "inspect", "private.symbol"],
        vec!["--json", "unknown-private-command"],
        vec!["--json", "context", "private.symbol", "--budget", "1024"],
    ];

    let workspace = workspace_root();
    let workspace_text = workspace.to_string_lossy();

    for args in cases {
        let first = lekalo(&args);
        let second = lekalo(&args);
        assert_eq!(first.status.code(), second.status.code());
        assert_eq!(first.stdout, second.stdout);
        assert_eq!(first.stderr, second.stderr);

        let bytes = if first.stdout.is_empty() {
            &first.stderr
        } else {
            &first.stdout
        };
        assert_json_document(bytes);
        let text = String::from_utf8_lossy(bytes);
        assert!(!text.contains('\u{1b}'));
        assert!(!text.contains(workspace_text.as_ref()));
        assert!(!text.contains(":\\"));
        assert!(!text.contains("\\\\"));
        assert!(!text.contains("private.symbol"));
        assert!(!text.contains("unknown-private-command"));
    }
}

#[test]
fn workspace_and_dependency_metadata_preserve_the_two_crate_boundary() {
    let workspace = workspace_root();
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .current_dir(&workspace)
        .output()
        .expect("run cargo metadata");
    assert!(output.status.success(), "{output:?}");

    let metadata: Value = serde_json::from_slice(&output.stdout).expect("cargo metadata JSON");
    let packages = metadata["packages"].as_array().expect("packages array");
    assert_eq!(packages.len(), 2);
    assert_eq!(
        metadata["workspace_members"]
            .as_array()
            .expect("workspace members")
            .len(),
        2
    );

    let mut package_names = packages
        .iter()
        .map(|package| package["name"].as_str().expect("package name"))
        .collect::<Vec<_>>();
    package_names.sort_unstable();
    assert_eq!(package_names, ["lekalo-cli", "lekalo-core"]);

    for package in packages {
        assert_eq!(package["version"], VERSION);
        assert_eq!(package["edition"], "2021");
        assert_eq!(package["rust_version"], "1.80.0");
    }

    // Normal (non-dev, non-build) dependencies of lekalo-core; the
    // cfg-gated native confinement entries carry a target and are excluded here.
    let core = packages
        .iter()
        .find(|package| package["name"] == "lekalo-core")
        .expect("core package");
    let mut core_normal_dependencies = core["dependencies"]
        .as_array()
        .expect("core dependencies")
        .iter()
        .filter(|dependency| dependency["kind"].is_null() && dependency["target"].is_null())
        .map(|dependency| dependency["name"].as_str().expect("dep name").to_owned())
        .collect::<Vec<_>>();
    core_normal_dependencies.sort();
    assert_eq!(
        core_normal_dependencies,
        [
            // Issue #20 adds the audited SQLite backend (bundled
            // amalgamation, exact-pinned, MIT, MSRV-compatible) behind the
            // cache storage seam.
            "rusqlite",
            "saphyr-parser",
            // Issue #9 adds the audited strict SemVer implementation and
            // the MSRV-proven SHA-256 digest for deterministic plan and
            // manifest identities; both are pinned and MSRV-compatible.
            "semver",
            "serde",
            "serde_json",
            "sha2",
            // Issue #27 uses exclusive owned temporary files and private
            // adapter views in production, not only in test fixtures.
            "tempfile",
            "unicode-normalization"
        ]
    );
    assert!(core["dependencies"]
        .as_array()
        .expect("core dependencies")
        .iter()
        .any(|dependency| dependency["name"] == "rustix"));
    assert!(!core["dependencies"]
        .as_array()
        .expect("core dependencies")
        .iter()
        .any(|dependency| dependency["name"] == "clap"));

    let cli = packages
        .iter()
        .find(|package| package["name"] == "lekalo-cli")
        .expect("CLI package");
    let mut cli_normal_dependencies = cli["dependencies"]
        .as_array()
        .expect("CLI dependencies")
        .iter()
        .filter(|dependency| dependency["kind"].is_null())
        .map(|dependency| dependency["name"].as_str().expect("dependency name"))
        .collect::<Vec<_>>();
    cli_normal_dependencies.sort_unstable();
    assert_eq!(
        cli_normal_dependencies,
        ["clap", "lekalo-core", "serde_json"]
    );
    assert!(cli["targets"]
        .as_array()
        .expect("CLI targets")
        .iter()
        .any(|target| target["name"] == "lekalo"
            && target["kind"]
                .as_array()
                .expect("target kind")
                .iter()
                .any(|kind| kind == "bin")));

    let root_manifest =
        std::fs::read_to_string(workspace.join("Cargo.toml")).expect("read workspace Cargo.toml");
    assert_eq!(root_manifest.matches("version = \"0.2.5\"").count(), 1);
    for member in [
        "crates/lekalo-core/Cargo.toml",
        "crates/lekalo-cli/Cargo.toml",
    ] {
        let manifest =
            std::fs::read_to_string(workspace.join(member)).expect("read member manifest");
        assert!(!manifest.contains("0.2.0"));
        assert!(!manifest.contains("0.2.1"));
        assert!(!manifest.contains("0.2.2"));
        assert!(!manifest.contains("0.2.3"));
        assert!(manifest.contains("version.workspace = true"));
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("CLI crate lives under workspace/crates")
        .to_path_buf()
}
