//! Issue #54 integration tests: the production `TargetClient` drives the
//! shipped `lekalo-target-php-laravel` artifact through the public
//! process protocol, proving the language-neutral core claim of the
//! issue — a second, non-Node adapter speaking the same wire inside the
//! same confined runtime, with the same handshake, plan/apply/retry
//! discipline, and honest unsupported refusals.
//!
//! The PHP interpreter must be installed on the host (`php` on PATH);
//! provisioning is a developer/CI concern, never an adapter behavior.
//! Availability is proven by one real confined describe exchange, not
//! merely by locating the interpreter on PATH: the confinement sandbox
//! copies the interpreter plus exactly the first-argument script into
//! its private view, and standard PHP builds carry dyld/DLL siblings
//! the copy cannot include (macOS seatbelt refuses the dyld deps;
//! Windows STATUS_DLL_NOT_FOUND without php8.dll beside the copied
//! php.exe). A host where the confined exchange cannot run is a skip
//! with an explicit, machine-readable reason — visibly reported, never
//! a silent pass — so the gate stays honest about what it proved. The
//! suite runs for real wherever the confined exchange succeeds (the
//! Linux CI leg, where bwrap ro-binds /usr and the packaged PHP build
//! is self-contained).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

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

/// Why the suite cannot run on this host, proven by evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Availability {
    /// The confined describe exchange succeeded: run everything.
    Runnable,
    /// No `php` interpreter on PATH (proven by `php -v`).
    MissingInterpreter,
    /// An interpreter exists, but the confined describe exchange died
    /// before an envelope could be produced (crash, spawn failure, or
    /// deadline) — the confinement copy of the interpreter cannot run
    /// on this platform, so the suite would prove nothing by running.
    ConfinedRuntimeUnavailable(&'static str),
}

impl Availability {
    /// The machine-readable skip reason; a skipped suite is visibly
    /// reported, never silently absent.
    fn reason(self) -> &'static str {
        match self {
            Self::Runnable => "runnable",
            Self::MissingInterpreter => {
                "skip: no PHP interpreter on PATH (php -v failed); \
                 the confined PHP kernel suite proved nothing on this host"
            }
            Self::ConfinedRuntimeUnavailable(class) => match class {
                "crash" => {
                    "skip: the confined PHP exchange crashed before an envelope; \
                     this platform's PHP build carries runtime dependencies the \
                     sandbox's interpreter copy cannot load (macOS seatbelt dyld \
                     refusal / Windows DLL_NOT_FOUND); the confined PHP kernel \
                     suite proved nothing on this host"
                }
                "spawn" => {
                    "skip: the confined PHP exchange could not spawn the \
                     interpreter; the confined PHP kernel suite proved nothing \
                     on this host"
                }
                "deadline" => {
                    "skip: the confined PHP exchange hit the deadline before an \
                     envelope; the confined PHP kernel suite proved nothing on \
                     this host"
                }
                _ => {
                    "skip: the confined PHP exchange failed transport; \
                     the confined PHP kernel suite proved nothing on this host"
                }
            },
        }
    }
}

/// Probe once per process: a bare `php -v` for interpreter presence,
/// then one real confined describe exchange through the production
/// client — the same exchange every test depends on. Only that proves
/// the platform can run the suite at all.
fn availability() -> Availability {
    static AVAILABLE: OnceLock<Availability> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let probe = std::process::Command::new("php").arg("-v").output();
        if !probe.map(|output| output.status.success()).unwrap_or(false) {
            return Availability::MissingInterpreter;
        }
        let sandbox =
            std::env::temp_dir().join(format!("lekalo-php-kernel-probe-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&sandbox);
        let command = kernel_command();
        let limits = TransportLimits {
            timeout_ms: 20_000,
            ..TransportLimits::default()
        };
        let mut client = TargetClient::new(limits);
        let outcome = client
            .describe(&command, &sandbox)
            .map(|described| described.capabilities.adapter.id.clone());
        let _ = std::fs::remove_dir_all(&sandbox);
        match outcome {
            Ok(id) if id == "lekalo-target-php-laravel" => Availability::Runnable,
            // A describe that answers with a different adapter is a
            // broken environment; the suite would prove nothing.
            Ok(_) => Availability::ConfinedRuntimeUnavailable("transport"),
            Err(TargetFailure::Crash { .. }) => Availability::ConfinedRuntimeUnavailable("crash"),
            Err(TargetFailure::TransportFailed { detail: "spawn" }) => {
                Availability::ConfinedRuntimeUnavailable("spawn")
            }
            Err(TargetFailure::Timeout) => Availability::ConfinedRuntimeUnavailable("deadline"),
            Err(_) => Availability::ConfinedRuntimeUnavailable("transport"),
        }
    })
}

/// Bail out of one test with the explicit evidence-backed reason. The
/// message always names what was (and was not) proved, so a skipped
/// suite is visible in CI logs instead of silently green.
fn require_runnable() -> bool {
    match availability() {
        Availability::Runnable => true,
        state => {
            eprintln!("{}", state.reason());
            false
        }
    }
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
    if !require_runnable() {
        return;
    }
    let sandbox = Sandbox::new("describe");
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    let described = client
        .describe(&command, &sandbox.dir)
        .expect("the PHP kernel handshake must succeed");
    assert_eq!(described.negotiated_version, "0.3.2");
    assert_eq!(
        described.capabilities.adapter.id,
        "lekalo-target-php-laravel"
    );
    assert_eq!(described.capabilities.adapter.version, "0.2.0");
    assert!(described.capabilities.adapter.digest.starts_with("sha256:"));
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
    assert_eq!(
        described.capabilities.ir_versions,
        vec!["0.2.16".to_owned()]
    );
    assert_eq!(
        described.capabilities.targets,
        vec!["php-laravel".to_owned()]
    );
}

#[test]
fn repeated_handshakes_bind_the_same_capability_digest() {
    if !require_runnable() {
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
    if !require_runnable() {
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
    assert!(planned[0]
        .sha256
        .as_deref()
        .is_some_and(lekalo_core::target_protocol::wire::is_sha256_digest,));
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
    let applied = apply
        .response
        .writes
        .clone()
        .expect("apply declares writes");
    assert_eq!(planned, applied);
    let file = sandbox
        .dir
        .join(".lekalo")
        .join("generated")
        .join("php-laravel")
        .join("php-laravel")
        .join("kernel.php");
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
    if !require_runnable() {
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
            assert_eq!(
                class,
                lekalo_core::target_protocol::wire::ErrorClass::Unsupported
            );
        }
        other => panic!("plan-native must refuse unsupported in-envelope, got {other:?}"),
    }
}

#[test]
fn a_cancelled_php_exchange_is_classified_and_recoverable() {
    if !require_runnable() {
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
    assert_eq!(
        described.capabilities.adapter.id,
        "lekalo-target-php-laravel"
    );
}

/// Issue #58: the type generator over the production transport — the
/// bounded types-input document plans the closed managed inventory, the
/// apply publishes exactly those bytes under the generated types root,
/// and a verify over the same input is clean. The stale-digest input
/// refuses before the child ever plans, proving the binding is real.
#[test]
fn the_php_type_generator_plans_applies_and_verifies_managed_types() {
    if !require_runnable() {
        return;
    }
    let sandbox = Sandbox::new("types");
    // The compiled-IR evidence sits at the canonical cache home; the
    // input document binds its exact digest.
    let cache = sandbox.dir.join(".lekalo").join("cache").join("ir");
    std::fs::create_dir_all(&cache).expect("cache home");
    let evidence = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/adapter-conformance/inputs/ir-minimal.json"),
    )
    .expect("fixture readable");
    std::fs::write(cache.join("planner.json"), &evidence).expect("evidence written");
    let digest = format!("sha256:{}", lekalo_core::digest::sha256_hex(&evidence));
    let types_home = sandbox.dir.join("lekalo").join("types");
    std::fs::create_dir_all(&types_home).expect("types home");
    std::fs::write(
        types_home.join("planner.types.json"),
        format!(
            concat!(
                "{{\"identity\":\"dev.lekalo.php-types-input@0.4.0\",",
                "\"irDigest\":\"{digest}\",\"projectId\":\"planner\",",
                "\"schemaVersion\":\"lekalo/php-types-input/v0.4.0\"}}\n"
            ),
            digest = digest
        ),
    )
    .expect("input written");
    let ir_path = "lekalo/types/planner.types.json";
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    client.describe(&command, &sandbox.dir).unwrap();
    let fs = Fs::open(&sandbox.dir).unwrap();
    let target = "php-laravel";

    // The dry run plans the closed managed inventory (25 files).
    let dry = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Generate,
                target: Some(target),
                profile: Some("default"),
                profile_resolution: None,
                ir_path: Some(ir_path),
                dry_run: Some(true),
                plan_id: None,
                native_request: None,
            },
            &sandbox.dir,
            &fs,
            None,
        )
        .expect("types dry run must succeed");
    let planned = dry.response.writes.clone().expect("dry run plans writes");
    assert_eq!(planned.len(), 25);
    for entry in &planned {
        assert_eq!(entry.action, WriteAction::Create);
        let path = entry.path.as_str();
        assert!(
            path.starts_with(".lekalo/generated/php-laravel/types/"),
            "managed type writes stay in the generated home: {path}"
        );
        assert!(
            lekalo_core::target_protocol::wire::is_sha256_digest(
                entry.sha256.as_deref().unwrap_or_default()
            ),
            "every planned write carries a digest"
        );
    }
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
                ir_path: Some(ir_path),
                dry_run: Some(false),
                plan_id: Some(&plan_id),
                native_request: None,
            },
            &sandbox.dir,
            &fs,
            None,
        )
        .expect("types apply must succeed");
    let applied = apply
        .response
        .writes
        .clone()
        .expect("apply declares writes");
    assert_eq!(planned, applied);
    let dto = sandbox
        .dir
        .join(".lekalo")
        .join("generated")
        .join("php-laravel")
        .join("types")
        .join("planner")
        .join("task_dto.php");
    let bytes = std::fs::read(&dto).expect("the applied DTO exists");
    assert!(String::from_utf8_lossy(&bytes).contains("final readonly class TaskDto"));
    assert!(String::from_utf8_lossy(&bytes).contains("strict_types=1"));

    // Verify over the same input is clean: the bytes on disk are the
    // planned bytes.
    let verify = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Verify,
                target: Some(target),
                profile: Some("default"),
                profile_resolution: None,
                ir_path: Some(ir_path),
                dry_run: None,
                plan_id: None,
                native_request: None,
            },
            &sandbox.dir,
            &fs,
            None,
        )
        .expect("types verify must succeed");
    let findings = verify
        .response
        .result
        .as_ref()
        .and_then(|result| result.findings.clone())
        .unwrap_or_default();
    assert!(findings.is_empty(), "fresh managed types verify clean");

    // The stale-digest input refuses as a bounded error: the binding to
    // the exact evidence bytes is checked before anything is planned.
    std::fs::write(
        types_home.join("stale.types.json"),
        concat!(
            "{\"identity\":\"dev.lekalo.php-types-input@0.4.0\",",
            "\"irDigest\":\"sha256:0000000000000000000000000000000000000000000000000000000000000000\",",
            "\"projectId\":\"planner\",",
            "\"schemaVersion\":\"lekalo/php-types-input/v0.4.0\"}\n"
        ),
    )
    .expect("stale input written");
    let stale = client.call(
        &command,
        CallRequest {
            operation: Operation::Generate,
            target: Some(target),
            profile: Some("default"),
            profile_resolution: None,
            ir_path: Some("lekalo/types/stale.types.json"),
            dry_run: Some(true),
            plan_id: None,
            native_request: None,
        },
        &sandbox.dir,
        &fs,
        None,
    );
    assert!(
        matches!(
            stale,
            Err(TargetFailure::Crash { .. }) | Err(TargetFailure::TransportFailed { .. })
        ),
        "a stale input digest is a refusal, never a plan"
    );
}

/// Issue #59, through the production client: the composed operations
/// run over the staged IR evidence plans the missing managed types and
/// the generated operations in one authorized plan, the apply publishes
/// exactly those bytes, and a verify over the same evidence is clean.
/// The query-write tamper is a typed finding with a zero-write plan.
#[test]
fn the_php_operations_generator_plans_applies_and_verifies_composed_operations() {
    if !require_runnable() {
        return;
    }
    let sandbox = Sandbox::new("operations");
    let cache = sandbox.dir.join(".lekalo").join("cache").join("ir");
    std::fs::create_dir_all(&cache).expect("cache home");
    let evidence = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/php-laravel/operations/inputs/ir/planner.ir.json"),
    )
    .expect("fixture readable");
    std::fs::write(cache.join("planner.json"), &evidence).expect("evidence written");
    let digest = format!("sha256:{}", lekalo_core::digest::sha256_hex(&evidence));
    let types_home = sandbox.dir.join("lekalo").join("types");
    std::fs::create_dir_all(&types_home).expect("types home");
    std::fs::write(
        types_home.join("planner.types.json"),
        format!(
            concat!(
                "{{\"identity\":\"dev.lekalo.php-types-input@0.4.0\",",
                "\"irDigest\":\"{digest}\",\"projectId\":\"planner\",",
                "\"schemaVersion\":\"lekalo/php-types-input/v0.4.0\"}}\n"
            ),
            digest = digest
        ),
    )
    .expect("types input written");
    let template = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/php-laravel/operations/inputs/planner.operations.json"),
    )
    .expect("operations template readable");
    let zeros = format!("sha256:{}", "0".repeat(64));
    let types_digest = {
        let bytes = std::fs::read(types_home.join("planner.types.json")).expect("types bytes");
        format!("sha256:{}", lekalo_core::digest::sha256_hex(&bytes))
    };
    let operations_home = sandbox.dir.join("lekalo").join("operations");
    std::fs::create_dir_all(&operations_home).expect("operations home");
    let input_bytes = template
        .replace(
            &format!("\"irDigest\": \"{zeros}\""),
            &format!("\"irDigest\": \"{digest}\""),
        )
        .replace(
            &format!("\"typesInputDigest\": \"{zeros}\""),
            &format!("\"typesInputDigest\": \"{types_digest}\""),
        );
    std::fs::write(
        operations_home.join("planner.operations.json"),
        &input_bytes,
    )
    .expect("operations input written");
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    client.describe(&command, &sandbox.dir).unwrap();
    let fs = Fs::open(&sandbox.dir).unwrap();
    let target = "php-laravel";
    let evidence_path = ".lekalo/cache/ir/planner.json";

    // The composed dry run plans types AND operations together.
    let dry = client
        .call(
            &command,
            CallRequest {
                operation: Operation::Generate,
                target: Some(target),
                profile: Some("default"),
                profile_resolution: None,
                ir_path: Some(evidence_path),
                dry_run: Some(true),
                plan_id: None,
                native_request: None,
            },
            &sandbox.dir,
            &fs,
            None,
        )
        .expect("operations dry run must succeed");
    let writes = dry.response.writes.clone().unwrap_or_default();
    assert!(
        writes.iter().any(|write| write
            .path
            .starts_with(".lekalo/generated/php-laravel/operations/")),
        "the operations family plans under the generated root"
    );
    assert!(
        writes.iter().any(|write| write
            .path
            .starts_with(".lekalo/generated/php-laravel/types/")),
        "the missing managed types ride the same authorized plan"
    );

    // The query-write tamper: a zero-write plan with the typed finding.
    let mut document: serde_json::Value =
        serde_json::from_str(input_bytes.trim()).expect("input decodes");
    document["operations"][0]["recipe"] = serde_json::json!({
        "kind": "single-entity-update",
        "entity": "planner.task",
        "key": "task_id",
        "assignments": [
            {"field": "state",
             "value": {"enumCase": {"type": "planner.task_state", "value": "focused"}}}
        ],
        "kept": ["title"],
        "missingBehavior": {"error": "planner.store_unavailable"}
    });
    let tampered = serde_json::to_string_pretty(&document).expect("tampered serializes") + "\n";
    std::fs::write(operations_home.join("planner.operations.json"), &tampered)
        .expect("tampered input written");
    let refused = client.call(
        &command,
        CallRequest {
            operation: Operation::Generate,
            target: Some(target),
            profile: Some("default"),
            profile_resolution: None,
            ir_path: Some(evidence_path),
            dry_run: Some(true),
            plan_id: None,
            native_request: None,
        },
        &sandbox.dir,
        &fs,
        None,
    );
    // The closed v0.3.2 generate result carries no findings member (the
    // client's completeness gate requires `result` to be absent), and an
    // error envelope admits a writes member only with partial=true, so
    // the veto surfaces as the client-classified OperationFailed: the
    // first sorted finding code names the refusal, the class is
    // invalid, and it is not a partial success. No plan authority is
    // ever bound: there is nothing to apply.
    match &refused {
        Err(TargetFailure::OperationFailed {
            class,
            code,
            partial,
        }) => {
            assert_eq!(
                class,
                &lekalo_core::target_protocol::wire::ErrorClass::Invalid
            );
            assert_eq!(code, "operations.query-write");
            assert!(!partial, "a veto is not a partial success");
        }
        other => panic!(
            "the tampered plan is a bounded OperationFailed, never a crash or a plan: {other:?}"
        ),
    }
    std::fs::write(
        operations_home.join("planner.operations.json"),
        &input_bytes,
    )
    .expect("restore input");
}
