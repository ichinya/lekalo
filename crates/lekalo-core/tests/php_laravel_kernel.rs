//! Issue #54 integration tests: the production `TargetClient` drives the
//! shipped `lekalo-target-php-laravel` artifact through the public
//! process protocol, proving the language-neutral core claim of the
//! issue — a second, non-Node adapter speaking the same wire inside the
//! same confined runtime, with the same handshake, plan/apply/retry
//! discipline, and honest unsupported refusals.
//!
//! The PHP interpreter must be installed on the host (`php` on PATH);
//! provisioning is a developer/CI concern, never an adapter behavior.
//! Tests skip cleanly when no interpreter is present so the gate stays
//! honest about what it proved instead of failing on missing tooling.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use lekalo_core::project_fs::Fs;
use lekalo_core::target_protocol::transport::{AdapterCommand, TransportLimits};
use lekalo_core::target_protocol::wire::{Operation, WriteAction};
use lekalo_core::target_protocol::{CallRequest, TargetClient, TargetFailure};

/// The shipped single-file artifact.
fn kernel_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../adapters/php-laravel/adapter.php")
        .display()
        .to_string()
}

fn kernel_command() -> AdapterCommand {
    AdapterCommand {
        program: PathBuf::from("php"),
        args: vec![kernel_path()],
    }
}

/// Whether a PHP interpreter is available without any provisioning.
fn php_available() -> bool {
    static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let output = std::process::Command::new("php")
            .arg("-v")
            .output()
            .map(|output| output.status.success());
        output.unwrap_or(false)
    })
}

/// One hermetic temporary project directory for the child cwd.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "lekalo-php-kernel-rs-{tag}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("sandbox dir");
        Self { dir }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn brief_limits() -> TransportLimits {
    TransportLimits {
        timeout_ms: 30_000,
        ..TransportLimits::default()
    }
}

fn require_valid_ir(sandbox: &Sandbox) -> String {
    let ir_path = ".lekalo/ir/minimal.json";
    let target = sandbox.dir.join(".lekalo").join("ir");
    std::fs::create_dir_all(&target).expect("ir home");
    std::fs::write(
        target.join("minimal.json"),
        std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/adapter-conformance/inputs/ir-minimal.json"),
        )
        .expect("fixture readable"),
    )
    .expect("ir written");
    ir_path.to_owned()
}

#[test]
fn the_php_kernel_describes_itself_through_the_production_client() {
    if !php_available() {
        eprintln!("skip: no PHP interpreter on PATH");
        return;
    }
    let sandbox = Sandbox::new("describe");
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    let described = client
        .describe(&command, &sandbox.dir)
        .expect("the PHP kernel handshake must succeed");
    assert_eq!(described.negotiated_version, "0.3.2");
    assert_eq!(described.capabilities.adapter.id, "lekalo-target-php-laravel");
    assert_eq!(described.capabilities.adapter.version, "0.1.0");
    assert!(described
        .capabilities
        .adapter
        .digest
        .starts_with("sha256:"));
    assert_eq!(
        described.capabilities.protocol_versions,
        vec!["0.3.2".to_owned()]
    );
    assert!(described
        .capabilities
        .operations
        .contains(&Operation::Describe));
    // The full v1 surface is declared: the strict conformance battery
    // requires exactly that (issue #31), and the adapter passes it.
    assert_eq!(described.capabilities.operations.len(), 9);
    assert_eq!(described.capabilities.ir_versions, vec!["0.2.16".to_owned()]);
    assert_eq!(described.capabilities.targets, vec!["php-laravel".to_owned()]);
}

#[test]
fn repeated_handshakes_bind_the_same_capability_digest() {
    if !php_available() {
        eprintln!("skip: no PHP interpreter on PATH");
        return;
    }
    let sandbox = Sandbox::new("digest");
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    let first = client.describe(&command, &sandbox.dir).unwrap();
    let digest = first.capability_digest.clone();
    let second = client.describe(&command, &sandbox.dir).unwrap();
    assert_eq!(digest, second.capability_digest);
}

#[test]
fn the_php_generation_seam_plans_applies_and_verifies() {
    if !php_available() {
        eprintln!("skip: no PHP interpreter on PATH");
        return;
    }
    let sandbox = Sandbox::new("generate");
    let ir_path = require_valid_ir(&sandbox);
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    client.describe(&command, &sandbox.dir).unwrap();
    let fs = Fs::open(&sandbox.dir).unwrap();
    let target = "php-laravel";

    // The dry run plans exactly one artifact with its digest.
    let dry = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Generate,
                target: Some(target),
                profile: Some("default"),
                profile_resolution: None,
                ir_path: Some(&ir_path),
                dry_run: Some(true),
                plan_id: None,
                native_request: None,
            },
            &sandbox.dir,
            &fs,
            None,
        )
        .expect("dry run must succeed");
    let planned = dry.response.writes.clone().expect("dry run plans writes");
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].action, WriteAction::Create);
    assert!(planned[0].sha256.as_deref().is_some_and(
        lekalo_core::target_protocol::wire::is_sha256_digest,
    ));
    let plan_id = dry.plan_id.expect("planning binds a plan id");

    // The apply publishes exactly the planned bytes.
    let apply = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Generate,
                target: Some(target),
                profile: Some("default"),
                profile_resolution: None,
                ir_path: Some(&ir_path),
                dry_run: Some(false),
                plan_id: Some(&plan_id),
                native_request: None,
            },
            &sandbox.dir,
            &fs,
            None,
        )
        .expect("apply must succeed");
    let applied = apply.response.writes.clone().expect("apply declares writes");
    assert_eq!(planned, applied);
    let file = sandbox.dir.join(".lekalo").join("generated").join("php-laravel").join("php-laravel").join("kernel.php");
    let bytes = std::fs::read(&file).expect("applied artifact exists");
    assert_eq!(
        format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes)),
        planned[0].sha256.as_deref().expect("digest").to_owned(),
        "applied bytes match the declared digest"
    );

    // The apply consumed its authority: a replay without a fresh dry
    // run is refused before the child is launched.
    let replay = client.call(
        &command,
        CallRequest {
            operation: Operation::Generate,
            target: Some(target),
            profile: Some("default"),
            profile_resolution: None,
            ir_path: Some(&ir_path),
            dry_run: Some(false),
            plan_id: Some(&plan_id),
            native_request: None,
        },
        &sandbox.dir,
        &fs,
        None,
    );
    assert!(
        matches!(replay, Err(TargetFailure::RequestInvalid { .. })),
        "replay must refuse: {replay:?}"
    );
}

#[test]
fn the_php_kernel_refuses_undeclared_input_without_a_fake_envelope() {
    if !php_available() {
        eprintln!("skip: no PHP interpreter on PATH");
        return;
    }
    let sandbox = Sandbox::new("refuse");
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    client.describe(&command, &sandbox.dir).unwrap();
    // The plan-native exchange is wire-legal but the kernel honestly
    // reports it unsupported in-envelope (the #54 MVP owns no native
    // planning); the client must see a classified operation failure.
    let native = lekalo_core::target_protocol::wire::NativeRequest {
        changes: Default::default(),
        scan_ref: lekalo_core::target_protocol::wire::NativeContentRef {
            digest: format!("sha256:{}", "1".repeat(64)),
            revision: None,
            adapter: None,
        },
        observed_ref: None,
        execution_policy_ref: lekalo_core::target_protocol::wire::NativeContentRef {
            digest: format!("sha256:{}", "2".repeat(64)),
            revision: None,
            adapter: None,
        },
        input_manifest_digest: format!("sha256:{}", "3".repeat(64)),
        tool_catalog_digest: format!("sha256:{}", "4".repeat(64)),
        capability_snapshot_digest: format!("sha256:{}", "5".repeat(64)),
    };
    let outcome = client.call(
        &command,
        CallRequest {
            operation: Operation::PlanNative,
            target: None,
            profile: None,
            profile_resolution: None,
            ir_path: None,
            dry_run: None,
            plan_id: None,
            native_request: Some(&native),
        },
        &sandbox.dir,
        &Fs::open(&sandbox.dir).unwrap(),
        None,
    );
    match outcome {
        Err(TargetFailure::OperationFailed { class, .. }) => {
            assert_eq!(class, lekalo_core::target_protocol::wire::ErrorClass::Unsupported);
        }
        other => panic!("plan-native must refuse unsupported in-envelope, got {other:?}"),
    }
}

#[test]
fn a_cancelled_php_exchange_is_classified_and_recoverable() {
    if !php_available() {
        eprintln!("skip: no PHP interpreter on PATH");
        return;
    }
    let sandbox = Sandbox::new("cancel");
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    client.describe(&command, &sandbox.dir).unwrap();
    let cancel = AtomicBool::new(true);
    let cancelled = client.call(
        &command,
        CallRequest {
            operation: Operation::Scan,
            target: None,
            profile: None,
            profile_resolution: None,
            ir_path: None,
            dry_run: None,
            plan_id: None,
            native_request: None,
        },
        &sandbox.dir,
        &Fs::open(&sandbox.dir).unwrap(),
        Some(&cancel),
    );
    assert!(matches!(cancelled, Ok(_) | Err(TargetFailure::Cancelled)));
    // A fresh handshake must recover and bind the same identity.
    let described = client.describe(&command, &sandbox.dir).unwrap();
    assert_eq!(described.capabilities.adapter.id, "lekalo-target-php-laravel");
}

#[test]
fn debug_dump_plan_native_request() {
    use lekalo_core::target_protocol::wire;
    let limits = lekalo_core::target_protocol::wire::Limits { timeout_ms: Some(30000), max_output_bytes: Some(8388608) };
    let mut envelope = lekalo_core::target_protocol::wire::RequestEnvelope {
        protocol: "lekalo.target/v1".into(),
        protocol_version: "0.3.2".into(),
        operation: Operation::PlanNative,
        request_id: "req-x".into(),
        project_root: ".".into(),
        ir_path: None, target: None, profile: None, profile_digest: None,
        profile_capabilities: None, dry_run: None, limits: Some(limits),
        plan_id: None,
        native_request: Some(wire::NativeRequest {
            changes: Default::default(),
            scan_ref: wire::NativeContentRef { digest: format!("sha256:{}", "1".repeat(64)), revision: None, adapter: None },
            observed_ref: None,
            execution_policy_ref: wire::NativeContentRef { digest: format!("sha256:{}", "2".repeat(64)), revision: None, adapter: None },
            input_manifest_digest: format!("sha256:{}", "3".repeat(64)),
            tool_catalog_digest: format!("sha256:{}", "4".repeat(64)),
            capability_snapshot_digest: format!("sha256:{}", "5".repeat(64)),
        }),
    };
    envelope.request_id = wire::request_id(&envelope);
    println!("{}", serde_json::to_string(&envelope).unwrap());
}
