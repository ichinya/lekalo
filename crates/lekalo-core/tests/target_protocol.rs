//! Issue #27 integration tests: the full `lekalo.target/v1` handshake with
//! a real adapter written in another language (Node.js), transport bounds
//! and infrastructure classification, protocol-mismatch refusal before any
//! generation, dry-run plan binding, and write-plan verification against
//! the observed project state.
//!
//! The tests are hermetic: every exchange runs inside a temporary project
//! directory, the adapter is the committed `fake-adapter.mjs` fixture, and
//! the only coupling is the process protocol itself. Fault injection
//! travels through the adapter argv, never through process-global state.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use lekalo_core::project_fs::Fs;
use lekalo_core::target_protocol::wire::{
    ErrorClass, Operation, ProtocolMismatch, ResponseInvalidity,
};
use lekalo_core::target_protocol::{
    transport, CallOutcome, CallRequest, TargetClient, TargetFailure,
};

/// The committed language-neutral adapter fixture.
fn adapter_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/target-protocol/fake-adapter.mjs")
        .display()
        .to_string()
}

fn adapter_command() -> transport::AdapterCommand {
    transport::AdapterCommand {
        program: PathBuf::from("node"),
        args: vec![adapter_path()],
    }
}

fn faulted_command(fault: &str) -> transport::AdapterCommand {
    transport::AdapterCommand {
        program: PathBuf::from("node"),
        args: vec![
            adapter_path(),
            "--lekalo-fault".to_owned(),
            fault.to_owned(),
        ],
    }
}

/// One hermetic temporary project directory with its validated `Fs`.
struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("lekalo-target-{tag}-{}-{id}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("sandbox dir");
        std::fs::create_dir_all(dir.join(".lekalo/ir")).expect("IR directory");
        std::fs::write(dir.join(".lekalo/ir/planner.json"), b"{}").expect("IR input");
        Self { dir }
    }

    fn fs(&self) -> Fs {
        Fs::open(&self.dir).expect("sandbox fs")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Short, test-sized limits; the hang fault outlives every deadline here.
fn test_limits() -> transport::TransportLimits {
    transport::TransportLimits {
        timeout_ms: 20_000,
        ..transport::TransportLimits::default()
    }
}

fn call_request<'a>(
    operation: Operation,
    target: Option<&'a str>,
    profile: Option<&'a str>,
    ir_path: Option<&'a str>,
    dry_run: Option<bool>,
    plan_id: Option<&'a str>,
) -> CallRequest<'a> {
    CallRequest {
        operation,
        target,
        profile,
        profile_resolution: None,
        ir_path,
        dry_run,
        plan_id,
    }
}

const TARGET: &str = "node-typescript";
const PROFILE: &str = "default";
const IR: &str = ".lekalo/ir/planner.json";

#[test]
fn full_handshake_negotiates_the_published_protocol() {
    let sandbox = Sandbox::new("handshake");
    let command = adapter_command();
    let mut client = TargetClient::new(test_limits());
    let outcome = client.describe(&command, &sandbox.dir).expect("handshake");
    assert_eq!(outcome.capabilities.adapter.id, TARGET);
    assert!(outcome
        .capabilities
        .protocol_versions
        .iter()
        .any(|v| v == "1.0.0"));
    assert_eq!(outcome.capabilities.operations.len(), 8);
    assert!(outcome.capability_digest.starts_with("sha256:"));
}

#[test]
fn operations_without_a_handshake_are_refused() {
    let sandbox = Sandbox::new("no-handshake");
    let command = adapter_command();
    let mut client = TargetClient::new(test_limits());
    let error = client
        .call(
            &command,
            call_request(Operation::Scan, None, None, None, None, None),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("handshake required");
    assert_eq!(
        error,
        TargetFailure::HandshakeRequired {
            operation: Operation::Scan
        }
    );
    assert_eq!(
        error.rule(),
        ("target.handshake-required", lekalo_core::Status::Invalid)
    );
}

#[test]
fn read_only_operations_round_trip_the_full_envelope() {
    let sandbox = Sandbox::new("read-ops");
    let command = adapter_command();
    let mut client = TargetClient::new(test_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    let outcome = client
        .call(
            &command,
            call_request(Operation::Scan, Some(TARGET), None, None, None, None),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect("scan");
    let entries = outcome
        .response
        .result
        .as_ref()
        .expect("result")
        .entries
        .as_ref()
        .expect("entries");
    assert_eq!(entries.len(), 2);
    let outcome = client
        .call(
            &command,
            call_request(
                Operation::Bind,
                Some(TARGET),
                Some(PROFILE),
                None,
                None,
                None,
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect("bind");
    let bindings = outcome
        .response
        .result
        .as_ref()
        .expect("result")
        .bindings
        .as_ref()
        .expect("bindings");
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].module, "planner");
}

#[test]
fn capability_and_request_validation_refuse_before_transport() {
    let sandbox = Sandbox::new("capability");
    let command = adapter_command();
    let mut client = TargetClient::new(test_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    let error = client
        .call(
            &command,
            call_request(Operation::Describe, None, None, None, None, None),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("describe through call");
    assert!(matches!(error, TargetFailure::RequestInvalid { .. }));

    let error = client
        .call(
            &command,
            call_request(
                Operation::Scan,
                Some("cobol-mainframe"),
                None,
                None,
                None,
                None,
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("unsupported target");
    assert_eq!(
        error,
        TargetFailure::CapabilityUnsupported { detail: "target" }
    );
    let result = lekalo_core::DomainResult::from(&error);
    let envelope: serde_json::Value = serde_json::from_str(&result.to_json_string()).unwrap();
    assert_eq!(result.status(), lekalo_core::Status::Unsupported);
    assert_eq!(result.exit_code(), 4);
    assert!(!result.writes_stderr());
    assert_eq!(envelope["status"], "unsupported");
    assert_eq!(
        envelope["reasonCodes"],
        serde_json::json!(["target.capability-unsupported"])
    );
    assert_eq!(envelope["diagnostics"][0]["code"], "LEK-TGT-004");
    assert_eq!(envelope["diagnostics"][0]["data"]["detail"], "target");
}

#[test]
fn generate_requires_dry_run_target_and_the_bound_plan() {
    let sandbox = Sandbox::new("generate-validation");
    let command = adapter_command();
    let mut client = TargetClient::new(test_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    let error = client
        .call(
            &command,
            call_request(Operation::Generate, None, None, None, None, None),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("ir-path required");
    assert!(matches!(
        error,
        TargetFailure::RequestInvalid { detail: "ir-path" }
    ));
    let error = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                None,
                Some(PROFILE),
                Some(IR),
                None,
                None,
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("dry-run required");
    assert!(matches!(
        error,
        TargetFailure::RequestInvalid { detail: "dry-run" }
    ));
    let error = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                None,
                Some(PROFILE),
                Some(IR),
                Some(true),
                None,
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("target required");
    assert!(matches!(
        error,
        TargetFailure::RequestInvalid { detail: "target" }
    ));
    // Apply without a prior dry-run plan binding.
    let error = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(false),
                Some("plan-00"),
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("unplanned apply");
    assert!(matches!(
        error,
        TargetFailure::RequestInvalid { detail: "plan-id" }
    ));
}

#[test]
fn dry_run_then_apply_verifies_the_write_plan_end_to_end() {
    let sandbox = Sandbox::new("apply");
    let command = adapter_command();
    let mut client = TargetClient::new(test_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    let plan: CallOutcome = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(true),
                None,
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect("dry-run plan");
    let plan_id = plan.plan_id.expect("plan identity");
    assert!(plan_id.starts_with("plan-"));
    // The dry run itself must not have written anything.
    assert!(!sandbox.dir.join(".lekalo/generated").exists());
    let outcome = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(false),
                Some(&plan_id),
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect("apply");
    assert_eq!(outcome.plan_id.as_deref(), Some(plan_id.as_str()));
    let generated = sandbox
        .dir
        .join(".lekalo/generated/node-typescript/model.ts");
    assert!(generated.exists());
    // The binding is consumed: a second apply needs a fresh plan.
    let error = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(false),
                Some(&plan_id),
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("binding consumed");
    assert!(matches!(
        error,
        TargetFailure::RequestInvalid { detail: "plan-id" }
    ));
}

#[test]
fn plan_clean_then_clean_round_trips_the_deletions() {
    let sandbox = Sandbox::new("clean");
    let command = adapter_command();
    let mut client = TargetClient::new(test_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    // Produce the artifact first through a normal dry-run/apply cycle.
    let plan: CallOutcome = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(true),
                None,
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect("dry-run plan");
    let generate_plan_id = plan.plan_id.expect("plan identity");
    client
        .call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(false),
                Some(&generate_plan_id),
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect("apply");
    let generated = sandbox
        .dir
        .join(".lekalo/generated/node-typescript/model.ts");
    assert!(generated.exists());
    let clean_plan: CallOutcome = client
        .call(
            &command,
            call_request(Operation::PlanClean, None, None, None, None, None),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect("plan-clean");
    let clean_plan_id = clean_plan.plan_id.expect("clean plan identity");
    client
        .call(
            &command,
            call_request(
                Operation::Clean,
                None,
                None,
                None,
                None,
                Some(&clean_plan_id),
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect("clean");
    assert!(!generated.exists());
}

#[test]
fn dry_run_mutation_is_prevented_by_confinement() {
    let sandbox = Sandbox::new("mutate-dry");
    let command = faulted_command("mutate-dry");
    let mut client = TargetClient::new(test_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    let error = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(true),
                None,
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("dry run mutated");
    // The OS rejects the attempted write. Node exits instead of returning a
    // valid envelope; no after-the-fact mutation is claimed or normalized.
    assert!(matches!(error, TargetFailure::Crash { .. }), "{error:?}");
    assert!(!sandbox.dir.join(".lekalo/generated").exists());
}

#[test]
fn undeclared_writes_are_caught_against_the_plan() {
    let sandbox = Sandbox::new("extra-write");
    let command = faulted_command("extra-write");
    let mut client = TargetClient::new(test_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    let plan: CallOutcome = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(true),
                None,
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect("dry-run plan");
    let plan_id = plan.plan_id.expect("plan identity");
    let error = client
        .call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(false),
                Some(&plan_id),
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        )
        .expect_err("undeclared write");
    assert!(matches!(
        error,
        TargetFailure::PlanMismatch {
            detail: "undeclared",
            ..
        }
    ));
}

#[test]
fn protocol_mismatches_are_unsupported_before_any_generation() {
    for (fault, detail, golden) in [
        (
            "wrong-token",
            ProtocolMismatch::Token,
            include_str!("../../../tests/fixtures/diagnostics/target-protocol-token-envelope.json"),
        ),
        (
            "wrong-version",
            ProtocolMismatch::Version,
            include_str!(
                "../../../tests/fixtures/diagnostics/target-protocol-version-envelope.json"
            ),
        ),
    ] {
        let sandbox = Sandbox::new(fault);
        let command = faulted_command(fault);
        let mut client = TargetClient::new(test_limits());
        let error = client
            .describe(&command, &sandbox.dir)
            .expect_err("wire mismatch must refuse the handshake");
        assert_eq!(error, TargetFailure::ProtocolMismatch { detail });

        // Exercise the production envelope, including the registry's status
        // allowlist and the shared exit/stream contract used by CLI renderers.
        let result = lekalo_core::DomainResult::from(&error);
        let json = result.to_json_string();
        let envelope: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope["status"], "unsupported", "{fault}: {json}");
        assert_eq!(result.status(), lekalo_core::Status::Unsupported);
        assert_eq!(result.exit_code(), 4);
        assert!(!result.writes_stderr());
        assert_eq!(
            envelope["reasonCodes"],
            serde_json::json!(["target.protocol-mismatch"])
        );
        assert_eq!(envelope["diagnostics"][0]["code"], "LEK-TGT-003");
        assert_eq!(
            envelope["diagnostics"][0]["data"]["detail"],
            detail.detail()
        );
        assert!(envelope.get("capability").is_none());
        assert_eq!(json, golden.trim_end());

        assert!(client.describe_outcome().is_none());
        assert!(client.plan_binding().is_none());
        let generation = client.call(
            &command,
            call_request(
                Operation::Generate,
                Some(TARGET),
                Some(PROFILE),
                Some(IR),
                Some(false),
                None,
            ),
            &sandbox.dir,
            &sandbox.fs(),
            None,
        );
        assert_eq!(
            generation.unwrap_err(),
            TargetFailure::HandshakeRequired {
                operation: Operation::Generate
            }
        );
        assert!(!sandbox.dir.join(".lekalo/generated").exists());
        assert_eq!(std::fs::read(sandbox.dir.join(IR)).unwrap(), b"{}");
    }
}

#[test]
fn timeout_crash_and_garbage_classify_as_infrastructure() {
    // A deadline far below the adapter's hang window.
    let limits = transport::TransportLimits {
        timeout_ms: 1_500,
        ..transport::TransportLimits::default()
    };
    let sandbox = Sandbox::new("hang");
    let command = faulted_command("hang");
    let mut client = TargetClient::new(limits);
    let error = client
        .describe(&command, &sandbox.dir)
        .expect_err("hung adapter");
    assert_eq!(error, TargetFailure::Timeout);
    assert_eq!(
        error.rule(),
        ("target.timeout", lekalo_core::Status::Unavailable)
    );

    let sandbox = Sandbox::new("crash");
    let command = faulted_command("crash");
    // Crash/JSON classification does not impose the deliberate hang's short
    // startup deadline; use the same bounded budget as normal exchanges.
    let mut client = TargetClient::new(test_limits());
    let error = client
        .describe(&command, &sandbox.dir)
        .expect_err("crashed");
    assert!(matches!(error, TargetFailure::Crash { .. }), "{error:?}");
    assert_eq!(
        error.rule(),
        ("target.crash", lekalo_core::Status::Unavailable)
    );

    let sandbox = Sandbox::new("garbage");
    let command = faulted_command("garbage");
    let error = client
        .describe(&command, &sandbox.dir)
        .expect_err("garbage");
    assert_eq!(
        error,
        TargetFailure::ResponseInvalid {
            detail: ResponseInvalidity::NotJson
        }
    );
    assert_eq!(
        error.rule(),
        ("target.response-invalid", lekalo_core::Status::Unavailable)
    );
}

#[test]
fn output_limits_and_echo_violations_are_refused() {
    // The noise fault floods stderr past a small cap.
    let limits = transport::TransportLimits {
        timeout_ms: 20_000,
        max_stderr_bytes: 4 * 1024,
        ..transport::TransportLimits::default()
    };
    let sandbox = Sandbox::new("noise");
    let command = faulted_command("noise");
    let mut client = TargetClient::new(limits);
    let error = client
        .describe(&command, &sandbox.dir)
        .expect_err("stderr flood");
    assert_eq!(
        error,
        TargetFailure::OutputLimit {
            stream: transport::Stream::Stderr
        }
    );

    // A foreign request-id echo is a malformed response.
    let sandbox = Sandbox::new("bad-echo");
    let command = faulted_command("bad-echo");
    let error = client
        .describe(&command, &sandbox.dir)
        .expect_err("foreign echo");
    assert_eq!(
        error,
        TargetFailure::ResponseInvalid {
            detail: ResponseInvalidity::RequestId
        }
    );
}

#[test]
fn in_envelope_operation_errors_project_their_class() {
    let sandbox = Sandbox::new("boom");
    let command = faulted_command("boom");
    let mut client = TargetClient::new(test_limits());
    let error = client.describe(&command, &sandbox.dir).expect_err("boom");
    assert_eq!(
        error,
        TargetFailure::OperationFailed {
            class: ErrorClass::Invalid,
            code: "fake-invalid".to_owned(),
            partial: false
        }
    );
    assert_eq!(
        error.rule(),
        ("target.operation-failed", lekalo_core::Status::Invalid)
    );
}

#[test]
fn cancellation_kills_the_child_and_classifies() {
    let sandbox = Sandbox::new("cancel");
    let limits = transport::TransportLimits {
        timeout_ms: 30_000,
        ..transport::TransportLimits::default()
    };
    let command = faulted_command("hang");
    let mut client = TargetClient::new(limits);
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let error = client
        .describe_with_cancel(&command, &sandbox.dir, Some(&cancel))
        .expect_err("cancelled");
    assert_eq!(error, TargetFailure::Cancelled);
    assert_eq!(
        error.rule(),
        ("target.cancelled", lekalo_core::Status::Unavailable)
    );
}

#[test]
fn protected_homes_and_scope_rules_never_yield_to_adapters() {
    use lekalo_core::target_protocol::wire::{validate_writes, WriteAction, WriteEntry};

    let scopes = vec![".lekalo/generated/node-typescript/**".to_owned()];
    let entry = |path: &str, action: WriteAction, digest: Option<&str>| WriteEntry {
        path: path.to_owned(),
        action,
        sha256: digest.map(str::to_owned),
    };
    let digest = format!("sha256:{}", "a".repeat(64));
    // Canonical plan: accepted.
    assert!(validate_writes(
        &[entry(
            ".lekalo/generated/node-typescript/model.ts",
            WriteAction::Create,
            Some(&digest)
        )],
        &scopes,
    )
    .is_ok());
    // Canonical order is mandatory.
    assert!(matches!(
        validate_writes(
            &[
                entry(
                    ".lekalo/generated/node-typescript/z.ts",
                    WriteAction::Create,
                    Some(&digest)
                ),
                entry(
                    ".lekalo/generated/node-typescript/a.ts",
                    WriteAction::Create,
                    Some(&digest)
                ),
            ],
            &scopes,
        )
        .expect_err("unsorted"),
        TargetFailure::PlanMismatch {
            detail: "unsorted",
            ..
        }
    ));
    // Delete entries never carry digests.
    assert!(matches!(
        validate_writes(
            &[entry(
                ".lekalo/generated/node-typescript/model.ts",
                WriteAction::Delete,
                Some(&digest)
            )],
            &scopes,
        )
        .expect_err("delete digest"),
        TargetFailure::PlanMismatch {
            detail: "digest",
            ..
        }
    ));
    // Writes outside the declared scopes are refused.
    assert!(matches!(
        validate_writes(
            &[entry(
                "vendor/output.ts",
                WriteAction::Create,
                Some(&digest)
            )],
            &scopes,
        )
        .expect_err("outside scope"),
        TargetFailure::ScopeViolation { .. }
    ));
    // Canonical homes are refused even if an adapter declares them.
    assert!(matches!(
        validate_writes(
            &[entry(
                "lekalo/project.yaml",
                WriteAction::Replace,
                Some(&digest)
            )],
            &["lekalo/**".to_owned()],
        )
        .expect_err("canonical model home"),
        TargetFailure::ProtectedPath {
            home: "lekalo-model",
            ..
        }
    ));
    assert!(matches!(
        validate_writes(
            &[entry(
                "openspec/changes/x.md",
                WriteAction::Create,
                Some(&digest)
            )],
            &["openspec/**".to_owned()],
        )
        .expect_err("openspec home"),
        TargetFailure::ProtectedPath {
            home: "openspec",
            ..
        }
    ));
}
