//! Engine-level tests for the contracted seam (issue #40): the wire
//! grammar of adapter declarations, the canonical registry bytes, the
//! signature/effect drift classification against the planner slice
//! fixture, and the support-artifact staleness gate. Every test builds
//! a sandbox copy under target/ addressed with relative selectors.

use lekalo_core::contracted;
use lekalo_core::loader::LoadSelection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

const SLICE: &str = "tests/fixtures/contracted/planner-slice";

static NEXT: AtomicU32 = AtomicU32::new(0);

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create sandbox dir");
    for entry in std::fs::read_dir(from).expect("read reference") {
        let entry = entry.expect("reference entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("entry type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy reference file");
        }
    }
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let unique = NEXT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::current_dir()
            .expect("cwd")
            .join("target/contracted-core")
            .join(format!("{tag}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../")
                .join(SLICE),
            &root,
        );
        Self { root }
    }

    fn selection(&self) -> LoadSelection {
        LoadSelection {
            project: Some(
                self.root
                    .strip_prefix(std::env::current_dir().expect("cwd"))
                    .expect("relative")
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            ),
        }
    }

    fn context(&self) -> contracted::Context {
        contracted::context(&self.selection()).expect("context")
    }

    fn declaration(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.root.join("declarations").join(name)).expect("declaration")
    }

    fn update(&self, name: &str) {
        contracted::update_registry(&self.context(), &self.declaration(name))
            .expect("update registry");
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn rewrite_declaration(sandbox: &Sandbox, name: &str, mutate: impl FnOnce(&mut serde_json::Value)) {
    let path = sandbox.root.join("declarations").join(name);
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("parse");
    mutate(&mut value);
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&value).expect("render") + "\n",
    )
    .expect("write");
}

#[test]
fn declaration_wire_fails_closed_on_unknown_keys_and_bad_grammar() {
    let base = br#"{
        "schemaVersion": "lekalo/contracted-declaration/v1.0.0",
        "adapter": {"id": "lekalo-target-node-typescript", "version": "1.0.0", "digest": null},
        "project": "planner",
        "revision": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "symbols": [], "artifacts": []
    }"#;
    let document = contracted::parse_declaration_bytes(base).expect("base parses");
    assert_eq!(document.symbols.len(), 0);

    let mut unknown: serde_json::Value = serde_json::from_slice(base).expect("parse");
    unknown["extra"] = serde_json::json!(1);
    let bytes = serde_json::to_vec(&unknown).expect("render");
    let set = contracted::parse_declaration_bytes(&bytes).expect_err("unknown key refuses");
    assert_eq!(set.reason_ids()[0], "contracted.declaration-invalid");

    let mut bad_revision: serde_json::Value = serde_json::from_slice(base).expect("parse");
    bad_revision["revision"] = serde_json::json!("sha256:UPPER");
    let bytes = serde_json::to_vec(&bad_revision).expect("render");
    let set = contracted::parse_declaration_bytes(&bytes).expect_err("bad digest refuses");
    assert_eq!(set.reason_ids()[0], "contracted.declaration-invalid");

    let mut bad_path: serde_json::Value = serde_json::from_slice(base).expect("parse");
    bad_path["symbols"] = serde_json::json!([{
        "id": "planner.focus_task", "kind": "command",
        "source": {"path": "lekalo/modules/planner/commands.yaml"},
        "signature": {"inputs": [], "output": null, "reads": []},
        "effects": []
    }]);
    let bytes = serde_json::to_vec(&bad_path).expect("render");
    let set = contracted::parse_declaration_bytes(&bytes).expect_err("canonical path refuses");
    assert_eq!(set.reason_ids()[0], "contracted.declaration-invalid");
}

#[test]
fn update_refuses_unknown_modules_symbols_and_kind_mismatch() {
    let sandbox = Sandbox::new("refuse");
    let ctx = sandbox.context();
    let mut unknown_module: serde_json::Value =
        serde_json::from_slice(&sandbox.declaration("initial.json")).expect("parse");
    unknown_module["symbols"][0]["id"] = serde_json::json!("ghost.focus_task");
    unknown_module["artifacts"] = serde_json::json!([]);
    let bytes = serde_json::to_vec(&unknown_module).expect("render");
    let set = contracted::update_registry(&ctx, &bytes).expect_err("unknown module");
    assert_eq!(set.reason_ids()[0], "contracted.unknown-module");

    let mut unknown_symbol: serde_json::Value =
        serde_json::from_slice(&sandbox.declaration("initial.json")).expect("parse");
    unknown_symbol["symbols"][0]["id"] = serde_json::json!("planner.ghost");
    unknown_symbol["artifacts"] = serde_json::json!([]);
    let bytes = serde_json::to_vec(&unknown_symbol).expect("render");
    let set = contracted::update_registry(&ctx, &bytes).expect_err("unknown symbol");
    assert_eq!(set.reason_ids()[0], "contracted.unknown-symbol");

    let mut kind_mismatch: serde_json::Value =
        serde_json::from_slice(&sandbox.declaration("initial.json")).expect("parse");
    kind_mismatch["symbols"][2]["kind"] = serde_json::json!("command");
    let bytes = serde_json::to_vec(&kind_mismatch).expect("render");
    let set = contracted::update_registry(&ctx, &bytes).expect_err("kind mismatch");
    assert_eq!(set.reason_ids()[0], "contracted.declaration-invalid");
}

#[test]
fn registry_persists_canonical_bytes_and_rereads_exactly() {
    let sandbox = Sandbox::new("canonical");
    sandbox.update("initial.json");
    let ctx = sandbox.context();
    let bytes = std::fs::read(
        ctx.root
            .join(contracted::REGISTRY_DIR)
            .join(contracted::REGISTRY_NAME),
    )
    .expect("registry bytes");
    let registry = contracted::parse_registry(&bytes).expect("canonical registry");
    assert_eq!(registry.mode, "contracted");
    assert_eq!(registry.project, "planner");
    assert_eq!(
        contracted::canonical_bytes(&registry)
            .expect("canonical bytes")
            .as_bytes(),
        bytes.as_slice()
    );
    // Any rewrite is noncanonical and refuses.
    let mut tampered = bytes.clone();
    tampered.extend_from_slice(b"\n");
    assert!(contracted::parse_registry(&tampered).is_err());
}

#[test]
fn signature_and_effect_drift_are_classified() {
    let sandbox = Sandbox::new("drift");
    sandbox.update("initial.json");
    // Drift: input type changed in the declaration.
    sandbox.update("drift-signature.json");
    let receipt = contracted::check(&sandbox.context(), None).expect_err("signature drift");
    assert!(receipt
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "contracted.binding-drift"));

    // Re-declare the conforming document, then drift the effects.
    sandbox.update("initial.json");
    sandbox.update("drift-effects.json");
    let receipt = contracted::check(&sandbox.context(), None).expect_err("effect drift");
    assert!(receipt
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "contracted.binding-drift"));

    // The conforming declaration passes once coverage is attached.
    sandbox.update("initial.json");
    let ctx = sandbox.context();
    contracted::attach(
        &ctx,
        "planner.focus_task",
        &["npm test -- focusTask".to_owned()],
        &[],
    )
    .expect("attach");
    contracted::attach(
        &ctx,
        "planner.list_tasks",
        &["npm test -- listTasks".to_owned()],
        &[],
    )
    .expect("attach");
    // The artifact claim needs its exact bytes inside the generated home.
    std::fs::create_dir_all(sandbox.root.join(".lekalo/generated/openapi")).expect("home");
    std::fs::copy(
        sandbox.root.join("openapi-planner.json"),
        sandbox.root.join(".lekalo/generated/openapi/planner.json"),
    )
    .expect("materialize");
    let receipt = contracted::check(&ctx, None).expect("clean check");
    assert_eq!(receipt.symbols, 3);
    assert_eq!(receipt.conformant, 3);
    assert!(receipt.drifts.is_empty());
}

#[test]
fn module_scope_flags_unimplemented_operations() {
    let sandbox = Sandbox::new("scope");
    // Declare only the command: the query is unimplemented.
    rewrite_declaration(&sandbox, "initial.json", |value| {
        let symbols = value["symbols"].as_array_mut().expect("symbols");
        symbols.retain(|symbol| symbol["id"] == "planner.focus_task");
    });
    sandbox.update("initial.json");
    let receipt =
        contracted::check(&sandbox.context(), Some("planner")).expect_err("unimplemented");
    assert!(receipt
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "contracted.binding-drift"));
}

#[test]
fn source_edits_are_detected_as_stale_bindings() {
    let sandbox = Sandbox::new("stale");
    sandbox.update("initial.json");
    let focus = sandbox.root.join("src/focus.ts");
    let mut source = std::fs::read_to_string(&focus).expect("read");
    source.push_str("\n// drift");
    std::fs::write(&focus, source).expect("write");
    let receipt = contracted::check(&sandbox.context(), None).expect_err("stale");
    assert!(receipt
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "contracted.binding-drift"));
}

#[test]
fn support_paths_confine_to_the_generated_home_and_digests_gate() {
    let sandbox = Sandbox::new("support");
    sandbox.update("initial.json");
    let ctx = sandbox.context();
    // A maintained path refuses.
    let set = contracted::support(
        &ctx,
        "planner.focus_task",
        contracted::SupportKind::Types,
        "src/focus.ts",
        contracted::SupportLifecycle::Generated,
        None,
    )
    .expect_err("maintained path refuses");
    assert_eq!(set.reason_ids()[0], "contracted.declaration-invalid");

    // A registered artifact without bytes is stale.
    contracted::support(
        &ctx,
        "planner.focus_task",
        contracted::SupportKind::Types,
        ".lekalo/generated/types/focus.json",
        contracted::SupportLifecycle::Generated,
        Some(&format!("sha256:{}", "a".repeat(64))),
    )
    .expect("register");
    let receipt = contracted::check(&ctx, None).expect_err("artifact missing");
    assert!(receipt
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "contracted.stale-artifact"));

    // Exact bytes satisfy the digest gate.
    std::fs::create_dir_all(sandbox.root.join(".lekalo/generated/types")).expect("dir");
    let payload = b"{\"planner\":true}";
    let digest = format!(
        "sha256:{}",
        lekalo_core::versioning::plan::sha256_hex(payload)
    );
    std::fs::write(
        sandbox.root.join(".lekalo/generated/types/focus.json"),
        payload,
    )
    .expect("write");
    contracted::support(
        &ctx,
        "planner.focus_task",
        contracted::SupportKind::Types,
        ".lekalo/generated/types/focus.json",
        contracted::SupportLifecycle::Generated,
        Some(&digest),
    )
    .expect("refresh");
    // Materialize the fixture's other declared artifact too, so the
    // only remaining findings are the coverage gaps.
    std::fs::create_dir_all(sandbox.root.join(".lekalo/generated/openapi")).expect("dir");
    std::fs::copy(
        sandbox.root.join("openapi-planner.json"),
        sandbox.root.join(".lekalo/generated/openapi/planner.json"),
    )
    .expect("materialize");
    let receipt = contracted::check(&ctx, None).expect_err("coverage gap remains");
    assert!(!receipt
        .as_slice()
        .iter()
        .any(|diagnostic| diagnostic.id() == "contracted.stale-artifact"));
}
