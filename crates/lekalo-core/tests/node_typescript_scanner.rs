//! Issue #44 integration tests: the production `TargetClient` drives the
//! actual single-file scanner artifact through the public process
//! protocol, proving the 0.3.1 negotiation, the launch-profile seam, the
//! pre-dispatch profile binding (review finding F1), the typed
//! scan-entry evidence member on the wire, and the end-to-end consumer
//! round trip into the observed binding registry (AC5).
//!
//! The suites never launch anything but the committed `adapter.mjs`
//! under Node (the same allowlist as CI), never execute a package
//! manager or project script, and never modify the adapter's source
//! tree: every scan runs inside a fresh private sandbox copied from the
//! committed synthetic fixtures.

use std::path::{Path, PathBuf};

use lekalo_core::target_protocol::transport::{AdapterCommand, TransportLimits};
use lekalo_core::target_protocol::wire::{Operation, SupportState};
use lekalo_core::target_protocol::{CallRequest, TargetClient, TargetFailure};

fn adapter_path() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../adapters/node-typescript/adapter.mjs")
        .display()
        .to_string()
}

fn scanner_command() -> AdapterCommand {
    AdapterCommand {
        program: PathBuf::from("node"),
        args: vec![adapter_path()],
    }
}

fn fixture_profile() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/node-typescript-scanner/protocol/conformance.profile.json");
    let text = std::fs::read_to_string(&path).expect("conformance profile fixture");
    text.split_whitespace().collect::<String>()
}

/// One hermetic temporary project directory with a tiny TypeScript
/// source tree, mirroring the committed `esm` fixture's shape.
struct ScanSandbox {
    dir: PathBuf,
}

impl ScanSandbox {
    fn new(tag: &str) -> Self {
        static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "lekalo-scanner-rs-{tag}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("src")).expect("sandbox src");
        std::fs::create_dir_all(dir.join("lekalo")).expect("sandbox lekalo home");
        std::fs::create_dir_all(dir.join("lekalo").join("modules").join("project"))
            .expect("module dir");
        std::fs::write(
            dir.join("lekalo").join("project.yaml"),
            b"schema_version: \"0.2.16\"\ndefinitions:\n  - id: scan_fixture\n    kind: project\n    version: 1\n",
        )
        .expect("project marker");
        std::fs::write(
            dir.join("lekalo")
                .join("modules")
                .join("project")
                .join("module.yaml"),
            b"schema_version: \"0.2.16\"\ndefinitions:\n  - id: project\n    kind: module\n    version: 1\n",
        )
        .expect("module marker");
        std::fs::write(
            dir.join("package.json"),
            br#"{ "name": "@scan/rs-fixture", "type": "module", "version": "1.0.0" }"#,
        )
        .expect("package manifest");
        std::fs::write(
            dir.join("src").join("main.ts"),
            b"import { tag } from './helper';\n\
              export interface Point { x: number; y: number }\n\
              export function dist(p: Point): number { return p.x + p.y + tag.length; }\n",
        )
        .expect("source");
        std::fs::write(
            dir.join("src").join("helper.ts"),
            b"export const tag = \"helper\";\n",
        )
        .expect("helper source");
        Self { dir }
    }
}

impl Drop for ScanSandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn brief_limits() -> TransportLimits {
    TransportLimits {
        timeout_ms: 60_000,
        ..TransportLimits::default()
    }
}

fn scan_request(profile: &'static str) -> CallRequest<'static> {
    CallRequest {
        operation: Operation::Scan,
        target: None,
        profile: Some(profile),
        profile_resolution: None,
        ir_path: None,
        dry_run: None,
        plan_id: None,
    }
}

#[test]
fn the_scanner_negotiates_the_current_protocol_and_declares_the_capability() {
    let sandbox = ScanSandbox::new("describe");
    let command = AdapterCommand {
        program: PathBuf::from("node"),
        args: vec![
            adapter_path(),
            "--lekalo-project-profile-json".to_owned(),
            fixture_profile(),
        ],
    };
    let mut client = TargetClient::new(brief_limits());
    let described = client
        .describe(&command, &sandbox.dir)
        .expect("the scanner handshake must succeed");
    assert_eq!(described.negotiated_version, "0.3.1");
    assert_eq!(
        described.capabilities.adapter.id,
        "lekalo-target-node-typescript"
    );
    assert_eq!(described.capabilities.adapter.version, "0.3.2");
    assert_eq!(described.capabilities.operations.len(), 2);
    assert!(described.capabilities.operations.contains(&Operation::Scan));
    assert_eq!(
        described.capabilities.read_scopes,
        vec!["src/**".to_owned()]
    );
    assert_eq!(
        described
            .capabilities
            .capabilities
            .get("scan.symbols")
            .copied(),
        Some(SupportState::Full)
    );
}

#[test]
fn scan_without_the_launch_profile_is_honestly_unsupported() {
    let sandbox = ScanSandbox::new("noprofile");
    let command = scanner_command();
    let mut client = TargetClient::new(brief_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    let outcome = client.call(
        &command,
        scan_request("standalone"),
        &sandbox.dir,
        &lekalo_core::project_fs::Fs::open(&sandbox.dir).expect("fs"),
        None,
    );
    match outcome {
        Err(TargetFailure::CapabilityUnsupported { .. }) => {}
        other => panic!("expected an unsupported refusal, got {other:?}"),
    }
}

#[test]
fn the_launched_scanner_indexes_symbols_and_reports_typed_evidence() {
    let sandbox = ScanSandbox::new("scan");
    let command = AdapterCommand {
        program: PathBuf::from("node"),
        args: vec![
            adapter_path(),
            "--lekalo-project-profile-json".to_owned(),
            fixture_profile(),
        ],
    };
    let mut client = TargetClient::new(brief_limits());
    client.describe(&command, &sandbox.dir).expect("handshake");
    let fs = lekalo_core::project_fs::Fs::open(&sandbox.dir).expect("fs");
    let outcome = client
        .call(
            &command,
            scan_request("standalone"),
            &sandbox.dir,
            &fs,
            None,
        )
        .unwrap_or_else(|error| panic!("the scan completes in-envelope: {error:?}"));
    let result = outcome.response.result.as_ref().expect("scan result");
    let entries = result.entries.as_deref().expect("entries");
    assert!(!entries.is_empty(), "the fixture symbols are indexed");
    // Every entry carries the closed typed evidence member with a
    // structural signature digest (the demo sources export callables).
    let with_signature = entries
        .iter()
        .filter(|entry| {
            entry
                .evidence
                .as_ref()
                .is_some_and(|evidence| evidence.signature.is_some())
        })
        .count();
    assert!(with_signature > 0, "signature evidence reaches the wire");
    // Determinism: the same request bytes repeat byte for byte.
    let second = client
        .call(
            &command,
            scan_request("standalone"),
            &sandbox.dir,
            &fs,
            None,
        )
        .expect("second scan");
    assert_eq!(
        serde_json::to_vec(&outcome.response).expect("serialize first"),
        serde_json::to_vec(&second.response).expect("serialize second"),
        "scan responses are deterministic"
    );
}

#[test]
fn scan_entries_with_signature_and_references_merge_into_the_registry() {
    // The end-to-end consumer round trip (AC5): the production scan
    // service drives the real scanner artifact and the merged registry
    // records symbols whose observed evidence carries the adapter's
    // typed references — the dependency graph edge material.
    let sandbox = ScanSandbox::new("merge");
    let command = AdapterCommand {
        program: PathBuf::from("node"),
        args: vec![
            adapter_path(),
            "--lekalo-project-profile-json".to_owned(),
            fixture_profile(),
        ],
    };
    // The observed context loads relative to the process cwd; the
    // sandbox itself carries the project marker files.
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(&sandbox.dir).expect("enter sandbox");
    let loaded = std::panic::catch_unwind(|| {
        let selection = lekalo_core::loader::LoadSelection { project: None };
        lekalo_core::observed::context(&selection).expect("observed context")
    });
    std::env::set_current_dir(original).expect("restore cwd");
    let context = loaded.expect("observed context");
    let request = lekalo_core::observed::scan_service::ScanRequest {
        target: "node-typescript",
        profile: Some("standalone"),
        command,
        limits: brief_limits(),
    };
    let receipt = lekalo_core::observed::scan_service::run(&context, &request)
        .expect("the production scan completes");
    assert!(receipt.symbols > 0, "symbols merged");
    let index = lekalo_core::observed::load_index(&context)
        .expect("index loads")
        .expect("index exists after a scan");
    let with_references = index
        .symbols
        .iter()
        .filter(|record| !record.evidence.references.is_empty())
        .count();
    assert!(
        with_references > 0,
        "typed references merge into the observed evidence (the dependency graph input)"
    );
    // The view projects the evidence into analyzer edges.
    let view = lekalo_core::observed::view::ObservedView::of(&index);
    assert!(
        !view.edges.is_empty() || with_references == 0,
        "evidence references become analyzer edges"
    );
}
