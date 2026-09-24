//! The adversarial containment suite (issue #89, plan S6).
//!
//! Every test drives the hostile fixture adapter through the real
//! confined `TargetClient` and proves one boundary: escape writes never
//! land, environment disclosure is bounded to the granted budget,
//! network is unreachable, output floods hit the cap, crashes clean up,
//! fork bombs are bounded, traversal plans refuse before publication,
//! and described scopes beyond the manifest ceiling refuse as
//! `adapter.permission-escalated`. Nothing here runs on the real tree:
//! every run is confined, classified, and its sandbox removed.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use lekalo_core::adapter_package::budget::SessionBudget;
use lekalo_core::adapter_package::ManifestDocument;
use lekalo_core::project_fs::Fs;
use lekalo_core::target_protocol::discovery::Discovery;
use lekalo_core::target_protocol::transport::AdapterCommand;
use lekalo_core::target_protocol::wire::Operation;
use lekalo_core::target_protocol::{CallRequest, TargetClient, TargetFailure};

const FIXTURE: &str = "tests/fixtures/adapter-security/adapter.mjs";

static NEXT_ROOT: AtomicU32 = AtomicU32::new(0);

/// A disposable project the fixture may (attempt to) touch.
struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let unique = NEXT_ROOT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!(
            "lekalo-security-{tag}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("project")).expect("project dir");
        Self { root }
    }

    fn project(&self) -> PathBuf {
        self.root.join("project")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The absolute fixture path, 8.3-alias-free on Windows runners.
fn fixture_path() -> String {
    let canonical = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(FIXTURE)
        .canonicalize()
        .expect("fixture exists");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => return rest.to_owned(),
        _ => {}
    }
    canonical.to_string_lossy().into_owned()
}

/// The fixture command vector with one hostile mode.
fn fixture_command(fault: &str) -> AdapterCommand {
    fixture_command_with(fault, &[])
}

fn fixture_command_with(fault: &str, extra_args: &[String]) -> AdapterCommand {
    AdapterCommand {
        program: "node".into(),
        args: vec![
            fixture_path(),
            "--lekalo-security-fault".into(),
            fault.into(),
        ]
        .into_iter()
        .chain(extra_args.iter().cloned())
        .collect(),
    }
}

/// The read-only project inputs the session needs for an IR-carrying
/// generate exchange.
fn stage_inputs(project: &Path) {
    std::fs::create_dir_all(project.join(".lekalo/ir")).expect("ir dir");
    std::fs::write(project.join(".lekalo/ir/input.json"), b"owned input").expect("ir input");
}

/// One full session: budget armed, describe, dry-run plan, apply.
/// Returns the apply outcome. `extra_args` extends the fixture argv
/// (the network test passes its loopback listener port this way).
fn run_session(
    tag: &str,
    fault: &str,
    budget: SessionBudget,
) -> (lekalo_core::target_protocol::CallOutcome, Sandbox) {
    run_session_with(tag, fault, budget, &[])
}

fn run_session_with(
    tag: &str,
    fault: &str,
    budget: SessionBudget,
    extra_args: &[String],
) -> (lekalo_core::target_protocol::CallOutcome, Sandbox) {
    let sandbox = Sandbox::new(tag);
    stage_inputs(&sandbox.project());
    let mut client = TargetClient::new(lekalo_core::target_protocol::transport::TransportLimits {
        timeout_ms: 30_000,
        ..Default::default()
    });
    client.set_budget(budget);
    let command = fixture_command_with(fault, extra_args);
    Discovery::run(&mut client, &command, &sandbox.project()).expect("describe");
    let fs = Fs::open(&sandbox.project()).expect("project fs");
    let planned = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Generate,
                target: Some("node-typescript"),
                profile: Some("default"),
                profile_resolution: None,
                ir_path: Some(".lekalo/ir/input.json"),
                dry_run: Some(true),
                plan_id: None,
                native_request: None,
            },
            &sandbox.project(),
            &fs,
            None,
        )
        .expect("dry-run");
    let plan_id = planned.plan_id.clone().expect("plan id");
    let outcome = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Generate,
                target: Some("node-typescript"),
                profile: Some("default"),
                profile_resolution: None,
                ir_path: Some(".lekalo/ir/input.json"),
                dry_run: Some(false),
                plan_id: Some(&plan_id),
                native_request: None,
            },
            &sandbox.project(),
            &fs,
            None,
        )
        .expect("apply");
    (outcome, sandbox)
}

/// The number of live lekalo sandbox staging directories.
fn live_sandboxes() -> usize {
    std::fs::read_dir(std::env::temp_dir())
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("lekalo-target-sandbox-")
                })
                .count()
        })
        .unwrap_or(0)
}

/// Wait until the sandbox staging directory count returns to the
/// baseline (other suites may hold their own sandboxes meanwhile).
fn staging_clean_again(baseline: usize) -> bool {
    for _ in 0..200 {
        if live_sandboxes() <= baseline {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    live_sandboxes() <= baseline
}

/// The fault outcome token the fixture reports as a progress detail.
fn fault_detail(outcome: &lekalo_core::target_protocol::CallOutcome) -> String {
    outcome
        .response
        .progress
        .as_ref()
        .and_then(|steps| steps.first())
        .map(|step| step.detail.clone().unwrap_or_default())
        .unwrap_or_default()
}

/// S6 #1: the escape attempt never lands on the real tree.
#[test]
fn an_escape_write_never_lands_outside_the_scopes() {
    let (outcome, sandbox) = run_session("escape", "escape", SessionBudget::strict_implicit());
    assert!(
        !sandbox.project().join("escape.txt").exists(),
        "the real project never receives the escape write"
    );
    assert_eq!(
        fault_detail(&outcome),
        "landed=false",
        "the sandbox denied the escape"
    );
    // The write audit shows only the declared, in-scope change.
    let audit = outcome.confinement.writes.as_ref().expect("write audit");
    assert_eq!(audit.declared, 1);
    assert_eq!(audit.outside_scopes, Vec::<String>::new());
}

/// S6 #2: the child sees exactly the granted variables — a host-only
/// variable never crosses and a secret handle resolves to its
/// controlled name.
#[test]
fn environment_disclosure_is_bounded_to_the_budget() {
    std::env::set_var("LEKALO_HOST_ONLY_VAR", "host-only-value");
    std::env::set_var("LEKALO_GRANTED_VAR", "granted-value");
    std::env::set_var("LEKALO_SECRET_PROBE", "probe-value");
    let budget = SessionBudget::from_declared(
        &lekalo_core::adapter_package::permissions::DeclaredPermissions {
            read_scopes: vec![".lekalo/ir/**".into(), ".lekalo/cache/ir/**".into()],
            write_scopes: vec!["out/**".into()],
            network_mode: "denied".into(),
            network_destinations: vec![],
            environment_allowlist: vec!["LEKALO_GRANTED_VAR".into()],
            child_processes: "denied".into(),
            secret_handles: vec!["probe".into()],
        },
    );
    let (outcome, sandbox) = run_session("envdump", "env-dump", budget);
    let published = std::fs::read(sandbox.project().join("out/probe.json")).expect("published");
    let record: serde_json::Value = serde_json::from_slice(&published).expect("json");
    let env = record["env"].as_object().expect("env object");
    assert_eq!(
        env.get("LEKALO_GRANTED_VAR").and_then(|v| v.as_str()),
        Some("granted-value"),
        "the granted variable crosses"
    );
    assert_eq!(
        env.get("LEKALO_HOST_ONLY_VAR"),
        None,
        "a host-only variable never crosses"
    );
    assert_eq!(
        env.get("LEKALO_SECRET_PROBE").and_then(|v| v.as_str()),
        Some("probe-value"),
        "the secret handle resolves to its controlled name"
    );
    if cfg!(not(windows)) {
        // Linux (clearenv + setenv) and macOS (env -i + grants) expose
        // strictly the granted set — nothing ambient.
        assert_eq!(env.len(), 2, "exactly the granted variables: {env:?}");
    } else {
        // Windows LPAC keeps a documented private minimum; everything
        // beyond it and the grants is still refused.
        let allowed: &[&str] = &[
            "APPDATA",
            "LOCALAPPDATA",
            "SystemDrive",
            "SystemRoot",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "windir",
        ];
        for name in env.keys() {
            assert!(
                allowed.contains(&name.as_str())
                    || name == "LEKALO_GRANTED_VAR"
                    || name == "LEKALO_SECRET_PROBE",
                "unexpected variable {name}"
            );
        }
    }
    // Values stay out of the confinement evidence: names only.
    let evidence = serde_json::to_string(&outcome.confinement).expect("evidence json");
    assert!(!evidence.contains("granted-value"));
    assert!(!evidence.contains("probe-value"));
    assert!(evidence.contains("LEKALO_GRANTED_VAR"));
    assert!(evidence.contains("LEKALO_SECRET_PROBE"));
}

/// S6 #3: the loopback dial is unreachable inside the sandbox while the
/// identical dial from this unsandboxed harness process connects. A real
/// listener removes the vacuous outcome: a refused dial against a closed
/// port proves nothing, a refused dial against a live listener does
/// (issue #89 fix round 2, C-F1).
#[test]
fn the_network_dial_is_unreachable() {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener");
    let port = listener.local_addr().expect("socket address").port();
    let target = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    // The control: without the sandbox this exact dial connects.
    std::net::TcpStream::connect_timeout(&target, std::time::Duration::from_secs(5))
        .expect("the unsandboxed control dial must connect to the live listener");
    // The fixture dials the same live listener inside the sandbox.
    let (outcome, _sandbox) = run_session_with(
        "network",
        "network",
        SessionBudget::strict_implicit(),
        &["--lekalo-dial-port".to_owned(), port.to_string()],
    );
    let detail = fault_detail(&outcome);
    assert!(
        detail == "dial=refused" || detail == "dial=timeout",
        "network denial is enforced inside the sandbox: {detail}"
    );
}

/// S6 #4: an unbounded stdout stream hits the output cap.
#[test]
fn a_stdout_flood_hits_the_output_cap() {
    let baseline = live_sandboxes();
    let sandbox = Sandbox::new("flood");
    stage_inputs(&sandbox.project());
    let mut client = TargetClient::new(lekalo_core::target_protocol::transport::TransportLimits {
        timeout_ms: 30_000,
        max_output_bytes: 1024,
        ..Default::default()
    });
    client.set_budget(SessionBudget::strict_implicit());
    let command = fixture_command("flood");
    let error = Discovery::run(&mut client, &command, &sandbox.project())
        .expect_err("the flood is refused at the cap");
    assert!(
        matches!(error, TargetFailure::OutputLimit { .. }),
        "output-limit refusal, got {error:?}"
    );
    assert!(
        staging_clean_again(baseline),
        "the staging sandbox is removed after the refusal"
    );
}

/// S6 #5: a hard crash classifies as a crash and cleans the sandbox.
#[test]
fn a_crash_classifies_and_cleans_the_sandbox() {
    let baseline = live_sandboxes();
    let sandbox = Sandbox::new("crash");
    stage_inputs(&sandbox.project());
    let mut client = TargetClient::default();
    client.set_budget(SessionBudget::strict_implicit());
    let command = fixture_command("crash");
    let error = Discovery::run(&mut client, &command, &sandbox.project())
        .expect_err("the crash is refused");
    assert!(
        matches!(error, TargetFailure::Crash { ref detail } if detail == "exit-7"),
        "crash classification, got {error:?}"
    );
    assert!(
        staging_clean_again(baseline),
        "the staging sandbox is removed after the crash"
    );
}

/// S6 #6: a concurrent fork bomb cannot escape the job/task bound
/// (Windows job cap 1; Linux task bound). macOS has no primitive and
/// is skipped honestly — the deadline kill still bounds it there.
#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_fork_bomb_stays_bounded() {
    let (outcome, _sandbox) =
        run_session("forkbomb", "fork-bomb", SessionBudget::strict_implicit());
    let detail = fault_detail(&outcome);
    let parts: Vec<&str> = detail.split(' ').collect();
    let spawned: u64 = parts
        .iter()
        .find(|part| part.starts_with("spawned="))
        .and_then(|part| part.strip_prefix("spawned="))
        .and_then(|value| value.parse().ok())
        .expect("spawn count");
    let attempts: u64 = parts
        .iter()
        .find(|part| part.starts_with("attempts="))
        .and_then(|part| part.strip_prefix("attempts="))
        .and_then(|value| value.parse().ok())
        .expect("attempt count");
    let enforcement = outcome.confinement.budget.children.enforcement;
    if cfg!(windows) {
        assert_eq!(spawned, 0, "the job admits exactly one process");
        assert_eq!(enforcement, "denied-enforced");
    } else {
        // The namespace contains the bomb either way; where the task
        // bound exists, children are refused before it is reached.
        if enforcement == "denied-bounded" {
            assert!(
                spawned < attempts,
                "the task bound refuses spawns before the budget: {detail}"
            );
        }
    }
}

/// S6 #7: traversal and absolute paths in writes[] refuse before any
/// publication.
#[test]
fn traversal_writes_refuse_before_publication() {
    let sandbox = Sandbox::new("traversal");
    stage_inputs(&sandbox.project());
    let mut client = TargetClient::default();
    client.set_budget(SessionBudget::strict_implicit());
    let command = fixture_command("traversal");
    Discovery::run(&mut client, &command, &sandbox.project()).expect("describe");
    let fs = Fs::open(&sandbox.project()).expect("project fs");
    let error = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Generate,
                target: Some("node-typescript"),
                profile: Some("default"),
                profile_resolution: None,
                ir_path: Some(".lekalo/ir/input.json"),
                dry_run: Some(true),
                plan_id: None,
                native_request: None,
            },
            &sandbox.project(),
            &fs,
            None,
        )
        .expect_err("the traversal plan is refused");
    assert!(
        matches!(
            error,
            TargetFailure::PlanMismatch { .. } | TargetFailure::ScopeViolation { .. }
        ),
        "plan-level refusal, got {error:?}"
    );
    assert!(!sandbox.project().join("escape.txt").exists());
}

/// The canonical package digest over the framed file parts (the same
/// framing `integrity` computes in-crate).
fn framed_package_digest(parts: &[Vec<u8>]) -> String {
    let mut framed = Vec::new();
    for part in parts {
        framed.extend_from_slice(&(part.len().to_be_bytes()));
        framed.extend_from_slice(part);
    }
    lekalo_core::digest::sha256_hex(&framed)
}

/// S6 #8: a manifested adapter whose describe claims scopes beyond its
/// manifest `permissions` ceiling refuses as
/// `adapter.permission-escalated` (denied), naming the exceeded
/// member.
#[test]
fn scope_exceeds_the_manifest_permission_ceiling() {
    let unique = NEXT_ROOT.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!(
        "lekalo-manifest-escalation-{}-{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let pkg = root.join("pkg");
    std::fs::create_dir_all(&pkg).expect("pkg");
    let entry_bytes = std::fs::read(fixture_path()).expect("fixture bytes");
    std::fs::write(pkg.join("probe.mjs"), &entry_bytes).expect("entry");
    let manifest_json = serde_json::json!({
        "schemaVersion": lekalo_core::adapter_package::version::MANIFEST_SCHEMA_VERSION,
        "identity": lekalo_core::adapter_package::version::MANIFEST_IDENTITY,
        "adapter": { "id": "security-probe", "name": "P", "version": "0.3.2" },
        "publisher": { "id": "p", "trustAnchor": "none" },
        "source": { "kind": "path", "coordinate": "path:pkg", "digest": format!("sha256:{}", "11".repeat(32)) },
        "license": { "spdx": "MIT", "file": "LICENSE", "fileDigest": format!("sha256:{}", "11".repeat(32)) },
        "compatibility": { "protocolVersions": [lekalo_core::target_protocol::version::VERSION], "irVersions": [lekalo_core::ir::version::VERSION], "extensions": [] },
        // The capability surface matches describe exactly.
        "capabilities": {
            "operations": ["describe", "generate", "scan", "verify"],
            "targets": ["node-typescript"], "profiles": ["default"], "named": {}, "constraints": {},
            "readScopes": [".lekalo/ir/**", ".lekalo/cache/ir/**"], "writeScopes": ["out/**"],
            "transports": ["stdin"]
        },
        "executable": { "runtime": { "kind": "node", "minVersion": "18.0.0" }, "entry": "probe.mjs", "argvPreview": ["node", "probe.mjs"], "assets": [] },
        "platforms": ["any"],
        "integrity": { "packageDigest": format!("sha256:{}", "0".repeat(64)), "files": [ { "path": "probe.mjs", "digest": format!("sha256:{}", lekalo_core::digest::sha256_hex(&entry_bytes)), "bytes": entry_bytes.len() } ], "signaturePolicy": "unsigned", "signature": null },
        // The runtime ceiling is NARROWER than the capability surface:
        // describe is an escalation against the budget.
        "permissions": { "filesystem": { "readScopes": [], "writeScopes": [] }, "network": { "mode": "denied", "destinations": [] }, "environment": { "allowlist": [] }, "processes": { "children": "denied" }, "secrets": { "handles": [] } },
        "hooks": [],
        "conformance": { "reportDigest": format!("sha256:{}", "33".repeat(32)), "badge": { "protocol": "0.3.2", "ir": "0.2.16", "profile": "default" }, "suiteRegistry": "dev.lekalo.diagnostic-registry@0.3.2" },
        "status": "active",
        "revocation": null
    });
    let provisional = serde_json::to_vec_pretty(&manifest_json).unwrap();
    std::fs::write(pkg.join("adapter.manifest.json"), &provisional).expect("manifest");
    let parsed = ManifestDocument::from_bytes(&provisional).expect("manifest parses");
    let entry_part = {
        let mut part = Vec::new();
        part.extend_from_slice(b"probe.mjs");
        part.push(0);
        part.extend_from_slice(&(entry_bytes.len() as u64).to_be_bytes());
        part.push(0);
        part.extend_from_slice(&entry_bytes);
        part
    };
    let manifest_part = {
        let mut part = Vec::new();
        part.extend_from_slice(lekalo_core::adapter_package::integrity::MANIFEST_FILE.as_bytes());
        part.push(0);
        part.extend_from_slice(&parsed.digest_domain_bytes().len().to_be_bytes());
        part.push(0);
        part.extend_from_slice(&parsed.digest_domain_bytes());
        part
    };
    let package_digest = framed_package_digest(&[entry_part, manifest_part]);
    let mut final_manifest = manifest_json;
    final_manifest["integrity"]["packageDigest"] =
        serde_json::Value::String(format!("sha256:{package_digest}"));
    std::fs::write(
        pkg.join("adapter.manifest.json"),
        serde_json::to_vec_pretty(&final_manifest).unwrap(),
    )
    .expect("final manifest");

    // Session over the manifested package: the budget comes from the
    // manifest ceiling, not describe.
    let mut client = TargetClient::default();
    let supply = lekalo_core::orchestration::AdapterSupply::new(
        &pkg,
        "node",
        vec![pkg.join("probe.mjs").to_string_lossy().into_owned()],
    )
    .expect("supply");
    let discovered = lekalo_core::orchestration::catalog::discover(
        &mut client,
        &supply,
        &pkg,
        Default::default(),
    )
    .expect("discovery");
    assert_eq!(discovered.adapter.id, "security-probe");
    // The budget is the manifest ceiling (empty scopes).
    assert_eq!(client.budget().read_caps(), Vec::<String>::new());
    let fs = Fs::open(&pkg).expect("project fs");
    let error = client
        .call(
            &supply.command,
            CallRequest {
                operation: Operation::Scan,
                target: Some("node-typescript"),
                profile: Some("default"),
                profile_resolution: None,
                ir_path: None,
                dry_run: None,
                plan_id: None,
                native_request: None,
            },
            &pkg,
            &fs,
            None,
        )
        .expect_err("described scopes exceed the ceiling");
    let (rule, status) = error.rule();
    assert_eq!(rule, "adapter.permission-escalated");
    assert_eq!(status.as_str(), "denied");
    let result = lekalo_core::result::DomainResult::from(&error);
    let diagnostic = result.diagnostics().first().expect("one diagnostic");
    assert_eq!(diagnostic.id(), "adapter.permission-escalated");
    let rendered = serde_json::to_value(diagnostic).expect("diagnostic json");
    let member = rendered["data"]["member"].as_str().unwrap_or_default();
    assert!(
        member.contains("capabilities.readScopes"),
        "the exceeded member is named: {member}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
