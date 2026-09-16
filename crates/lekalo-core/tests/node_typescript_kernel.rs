//! Issue #43 integration tests: the production `TargetClient` drives the
//! actual `lekalo-target-node-typescript` kernel artifact through the
//! public process protocol, proving the handshake, honest unsupported
//! refusals for absent extensions, response determinism, and the
//! classification of every public failure path.
//!
//! The kernel owns the protocol lifecycle; these tests own the consumer
//! side. They never launch anything but the committed `adapter.mjs` under
//! Node (the same allowlist as CI), and they never modify the kernel's
//! source tree.

use std::path::{Path, PathBuf};

use lekalo_core::target_protocol::transport::{AdapterCommand, TransportLimits};
use lekalo_core::target_protocol::wire::Operation;
use lekalo_core::target_protocol::{CallRequest, TargetClient, TargetFailure};

/// The committed single-file kernel artifact.
fn kernel_command() -> AdapterCommand {
    AdapterCommand {
        program: PathBuf::from("node"),
        args: vec![kernel_path()],
    }
}

fn kernel_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../adapters/node-typescript/adapter.mjs")
        .display()
        .to_string()
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
            "lekalo-kernel-rs-{tag}-{}-{id}",
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

#[test]
fn the_kernel_describes_itself_through_the_production_client() {
    let sandbox = Sandbox::new("describe");
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    let described = client
        .describe(&command, &sandbox.dir)
        .expect("the kernel handshake must succeed");
    assert_eq!(described.negotiated_version, "0.3.1");
    assert_eq!(
        described.capabilities.adapter.id,
        "lekalo-target-node-typescript"
    );
    assert_eq!(described.capabilities.adapter.version, "0.3.2");
    assert!(described.capabilities.adapter.digest.starts_with("sha256:"));
    // The kernel advertises describe only, with truthful emptiness.
    assert_eq!(described.capabilities.operations, vec![Operation::Describe]);
    assert!(described.capabilities.ir_versions.is_empty());
    assert!(described.capabilities.read_scopes.is_empty());
    assert!(described.capabilities.write_scopes.is_empty());
    assert!(!described.capabilities.progress);
    // Every registered capability id is declared unsupported — an honest
    // gap, never an optimistic or invented state.
    for state in described.capabilities.capabilities.values() {
        assert_eq!(
            *state,
            lekalo_core::target_protocol::wire::SupportState::Unsupported
        );
    }
}

#[test]
fn repeated_handshakes_bind_the_same_capability_digest() {
    let sandbox = Sandbox::new("digest");
    let command = kernel_command();
    let mut first = TargetClient::new(brief_limits());
    let a = first
        .describe(&command, &sandbox.dir)
        .expect("first")
        .to_owned();
    let mut second = TargetClient::new(brief_limits());
    let b = second
        .describe(&command, &sandbox.dir)
        .expect("second")
        .to_owned();
    assert_eq!(a.capability_digest, b.capability_digest);
    assert_eq!(a.capabilities, b.capabilities);
}

#[test]
fn undeclared_operations_are_refused_before_launch() {
    let sandbox = Sandbox::new("undeclared");
    let command = kernel_command();
    let mut client = TargetClient::new(brief_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    let outcome = client.call(
        &command,
        CallRequest {
            operation: Operation::Scan,
            target: None,
            profile: None,
            profile_resolution: None,
            ir_path: None,
            dry_run: None,
            plan_id: None,
        },
        &sandbox.dir,
        &lekalo_core::project_fs::Fs::open(&sandbox.dir).expect("fs"),
        None,
    );
    match outcome {
        Err(TargetFailure::CapabilityUnsupported { detail }) => {
            assert_eq!(detail, "operation");
        }
        other => panic!("expected capability-unsupported, got {other:?}"),
    }
}

#[test]
fn the_kernel_refuses_invalid_protocol_input_without_a_fake_envelope() {
    // A garbage child would crash; the kernel must not be garbage. This
    // probe drives the adapter at the transport layer with a malformed
    // request and expects a nonzero exit with no stdout envelope.
    use std::process::{Command, Stdio};
    let sandbox = Sandbox::new("invalid");
    let mut child = Command::new("node")
        .arg(kernel_path())
        .current_dir(&sandbox.dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn node");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait");
    assert_ne!(
        output.status.code(),
        Some(0),
        "a missing request is refused"
    );
    assert!(output.stdout.is_empty(), "no fabricated envelope on stdout");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("lekalo-target-node-typescript"),
        "bounded stderr"
    );
    assert!(stderr.len() < 512, "stderr stays bounded");
}
