//! Issue #34 provider-boundary conformance: `lekalo provider describe`
//! is the published AIFHub discovery handshake. Child-process tests
//! against the real binary prove determinism, stream/exit discipline,
//! side-effect-free discovery, and the closed operation vocabulary.
//! These tests are the boundary the extension consumes; they never link
//! `lekalo-core` internals into a consumer.

use std::fs;
use std::process::{Command, Output};

fn lekalo_in(dir: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(dir)
        .env_clear()
        .env(
            "SystemRoot",
            std::env::var("SystemRoot").unwrap_or_default(),
        )
        .output()
        .expect("run the real lekalo binary")
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lekalo-provider-test-{}-{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn describe_emits_a_valid_manifest_receipt_on_stdout() {
    let dir = temp_dir("describe");
    let output = lekalo_in(&dir, &["provider", "describe", "--json"]);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(stderr_text(&output).is_empty(), "stdout owns the receipt");
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope parses");
    assert_eq!(document["status"], "valid");
    assert_eq!(
        document["manifest"]["schemaVersion"],
        "lekalo/workflow-provider/v0.6.4"
    );
    assert_eq!(
        document["manifest"]["identity"],
        "dev.lekalo.workflow-provider@0.6.4"
    );
    assert_eq!(document["manifest"]["productVersion"], "0.6.4");
    assert_eq!(
        document["manifest"]["targetProtocolIdentity"], "dev.lekalo.target-protocol@0.3.2",
        "the target protocol stays a separate negotiated family"
    );
    let digest = document["manifest"]["manifestDigest"]
        .as_str()
        .expect("digest present");
    assert!(digest.starts_with("sha256:"));
    assert_eq!(digest.len(), "sha256:".len() + 64);
}

#[test]
fn json_projection_is_byte_identical_to_the_committed_golden() {
    let dir = temp_dir("golden");
    let output = lekalo_in(&dir, &["provider", "describe", "--json"]);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(0));
    let golden = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/provider/describe.golden.json"
    ))
    .expect("golden fixture");
    assert_eq!(
        stdout_text(&output),
        golden,
        "manifest projection is golden"
    );
}

#[test]
fn describe_is_deterministic_across_runs_and_directories() {
    let first_dir = temp_dir("determinism-a");
    let second_dir = temp_dir("determinism-b");
    let first = lekalo_in(&first_dir, &["--json", "provider", "describe"]);
    let second = lekalo_in(&second_dir, &["provider", "describe", "--json"]);
    let _ = fs::remove_dir_all(&first_dir);
    let _ = fs::remove_dir_all(&second_dir);
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(stdout_text(&first), stdout_text(&second), "byte-identical");
    assert!(!stdout_text(&first).contains('\r'));
}

#[test]
fn describe_works_outside_any_repository_and_writes_nothing() {
    let dir = temp_dir("no-side-effects");
    let before = fs::read_dir(&dir).expect("list").count();
    let output = lekalo_in(&dir, &["provider", "describe", "--json"]);
    let after_entries: Vec<String> = fs::read_dir(&dir)
        .expect("list")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    let after = after_entries.len();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(before, 0, "the sandbox starts empty");
    assert_eq!(after, 0, "discovery writes nothing: {after_entries:?}");
    assert!(stderr_text(&output).is_empty());
}

#[test]
fn describe_never_declares_hidden_lifecycle_operations() {
    let dir = temp_dir("closed-vocabulary");
    let output = lekalo_in(&dir, &["provider", "describe", "--json"]);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(0));
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
    let operations = document["manifest"]["operations"]
        .as_array()
        .expect("operations array");
    let ids: Vec<&str> = operations
        .iter()
        .map(|operation| operation["id"].as_str().expect("id"))
        .collect();
    assert_eq!(
        ids,
        vec![
            "context",
            "doctor",
            "drift",
            "generate",
            "impact",
            "readiness",
            "status",
            "trace.export",
            "validate",
            "verify"
        ]
    );
    for forbidden in [
        "init", "install", "update", "sync", "cleanup", "migrate", "detect",
    ] {
        assert!(!ids.contains(&forbidden), "{forbidden} is never advertised");
    }
    // Every non-generate operation is read-only; only generate mutates
    // and only generate requires an adapter program.
    for operation in operations {
        let id = operation["id"].as_str().expect("id");
        let effect = operation["effect"].as_str().expect("effect");
        if id == "generate" {
            assert_eq!(effect, "generated-artifacts");
            assert_eq!(operation["requiresAdapter"], true);
        } else {
            assert_eq!(effect, "read-only", "{id}");
            assert_eq!(operation["requiresAdapter"], false, "{id}");
        }
    }
}

#[test]
fn describe_pins_one_output_schema_per_operation_and_the_shared_families() {
    let dir = temp_dir("schema-pins");
    let output = lekalo_in(&dir, &["provider", "describe", "--json"]);
    let _ = fs::remove_dir_all(&dir);
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
    let schema_of = |id: &str| -> String {
        document["manifest"]["operations"]
            .as_array()
            .expect("operations")
            .iter()
            .find(|operation| operation["id"] == id)
            .expect("operation")["outputSchema"]
            .as_str()
            .expect("schema")
            .to_owned()
    };
    assert_eq!(schema_of("status"), "lekalo/doctor/v0.3.2");
    assert_eq!(schema_of("doctor"), "lekalo/doctor/v0.3.2");
    assert_eq!(schema_of("readiness"), "lekalo/doctor/v0.3.2");
    assert_eq!(schema_of("impact"), "lekalo/impact/v0.2.16");
    assert_eq!(schema_of("context"), "lekalo/context/v0.2.16");
    assert_eq!(schema_of("validate"), "lekalo/validation-report/v0.6.4");
    assert_eq!(schema_of("drift"), "lekalo/generate-check/v0.6.3");
    assert_eq!(schema_of("verify"), "lekalo/orchestration/v0.2.16");
    assert_eq!(schema_of("generate"), "lekalo/orchestration/v0.2.16");
    assert_eq!(schema_of("trace.export"), "lekalo/trace-manifest/v0.2.16");
    // The pins cover the describing schemas for the receipt-shaped
    // payloads without embedded discriminators, the wire-discriminated
    // families, and the diagnostic envelope.
    let pins: Vec<&str> = document["manifest"]["schemaPins"]
        .as_array()
        .expect("pins")
        .iter()
        .map(|pin| pin["schemaVersion"].as_str().expect("pin"))
        .collect();
    assert_eq!(pins.len(), 9);
    assert!(
        pins.contains(&"lekalo/diagnostic/v0.2.16"),
        "diagnostics are pinned"
    );
    assert!(
        pins.contains(&"lekalo/validation-report/v0.6.4"),
        "the validate result contract is pinned"
    );
    assert!(
        pins.contains(&"lekalo/generate-check/v0.6.3"),
        "the drift receipt contract is pinned"
    );
}

#[test]
fn manifest_digest_binds_the_exact_payload() {
    let dir = temp_dir("digest");
    let output = lekalo_in(&dir, &["provider", "describe", "--json"]);
    let _ = fs::remove_dir_all(&dir);
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
    // Recompute the canonical digest domain: sorted keys, no
    // whitespace, manifestDigest removed.
    fn canonical(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let body: Vec<String> = keys
                    .into_iter()
                    .map(|key| {
                        format!(
                            "{}:{}",
                            serde_json::to_string(key).unwrap(),
                            canonical(&map[key])
                        )
                    })
                    .collect();
                format!("{{{}}}", body.join(","))
            }
            serde_json::Value::Array(items) => {
                let body: Vec<String> = items.iter().map(canonical).collect();
                format!("[{}]", body.join(","))
            }
            serde_json::Value::String(text) => serde_json::to_string(text).unwrap(),
            other => other.to_string(),
        }
    }
    let mut manifest = document["manifest"].clone();
    let recorded = manifest
        .as_object_mut()
        .expect("object")
        .remove("manifestDigest")
        .expect("digest");
    let digest = sha256_hex(canonical(&manifest).as_bytes());
    assert_eq!(
        recorded.as_str().expect("digest string"),
        format!("sha256:{digest}"),
        "the consumer-side recomputation must agree"
    );
}

#[test]
fn unknown_provider_subcommands_stay_usage_errors() {
    let dir = temp_dir("usage");
    let output = lekalo_in(&dir, &["provider", "install"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout_text(&output).is_empty(), "nothing on stdout");
    let stderr = stderr_text(&output);
    assert!(
        stderr.contains("LEK-CLI-001"),
        "stable usage code: {stderr}"
    );
    assert!(stderr.contains("cli.usage"));
    // `--json` changes the rendering, not the classification.
    let json_output = lekalo_in(&dir, &["provider", "init", "--json"]);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(json_output.status.code(), Some(1));
    let envelope: serde_json::Value =
        serde_json::from_str(stderr_text(&json_output).trim()).expect("envelope");
    assert_eq!(envelope["status"], "invalid");
    assert_eq!(envelope["diagnostics"][0]["code"], "LEK-CLI-001");
}

#[test]
fn human_projection_is_one_stable_summary_line() {
    let dir = temp_dir("human");
    let output = lekalo_in(&dir, &["provider", "describe"]);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout_text(&output),
        "provider dev.lekalo.workflow-provider@0.6.4 product 0.6.4 operations \
         context,doctor,drift,generate,impact,readiness,status,trace.export,validate,verify\n"
    );
}

/// SHA-256 over exact bytes (test-local; the consumer contract is the
/// `sha256:<hex>` spelling, not a linked hasher).
fn sha256_hex(bytes: &[u8]) -> String {
    // Minimal pure-Rust SHA-256 to keep this test dependency-free.
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut message = bytes.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.chunks(64) {
        let mut w = [0u32; 64];
        for (index, word) in chunk.chunks(4).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let mut v = state;
        for index in 0..64 {
            const K: [u32; 64] = [
                0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
                0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
                0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
                0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
                0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
                0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
                0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
                0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
                0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
                0xc67178f2,
            ];
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ ((!v[4]) & v[6]);
            let temp1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let temp2 = s0.wrapping_add(maj);
            v[7] = v[6];
            v[6] = v[5];
            v[5] = v[4];
            v[4] = v[3].wrapping_add(temp1);
            v[3] = v[2];
            v[2] = v[1];
            v[1] = v[0];
            v[0] = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip(v) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = String::with_capacity(64);
    for word in state {
        out.push_str(&format!("{word:08x}"));
    }
    out
}

#[test]
fn sha256_helper_matches_the_known_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// The prescribed provider argv for `validate` carries `--no-cache`: the
/// default cached pipeline materializes `.lekalo/cache/cache.sqlite`, so
/// a read-only provider phase must use the bypass (fix-round finding:
/// undeclared mutation under a read-only effect).
#[test]
fn prescribed_validate_argv_with_no_cache_writes_nothing() {
    let fixtures = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/validation"
    );
    let work = temp_dir("validate-no-cache");
    let project = work.join("base");
    fs_extra_copy_dir(
        &std::path::PathBuf::from(fixtures).join("valid/base"),
        &project,
    );
    let before = count_entries(&project);
    let output = lekalo_in(
        &work,
        &["validate", "--no-cache", "--json", "--project", "base"],
    );
    let after = count_entries(&project);
    let _ = fs::remove_dir_all(&work);
    assert_eq!(output.status.code(), Some(0), "{:?}", output.status);
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["validation"]["profile"], "default");
    assert!(
        !project.join(".lekalo").exists(),
        "the prescribed argv materializes no cache home"
    );
    assert_eq!(before, after, "no filesystem entries added");
}

/// The `drift` operation (generate --check) is negotiated through its own
/// published receipt contract, never through the orchestration schema,
/// and a clean check writes nothing.
#[test]
fn drift_check_clean_receipt_writes_nothing() {
    let fixtures = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/orchestration"
    );
    let work = temp_dir("drift-clean");
    let project = work.join("project");
    fs_extra_copy_dir(
        &std::path::PathBuf::from(fixtures).join("project"),
        &project,
    );
    // A clean drift check needs the lock the ownership manifest verifies
    // against (an explicit, documented consumer step).
    let lock = lekalo_in(&work, &["lock", "--json", "--project", "project"]);
    assert_eq!(lock.status.code(), Some(0), "{lock:?}");
    let before = count_entries(&project);
    let output = lekalo_in(
        &work,
        &["generate", "--check", "--json", "--project", "project"],
    );
    let after = count_entries(&project);
    let _ = fs::remove_dir_all(&work);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let document: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope");
    assert_eq!(document["status"], "valid");
    assert_eq!(document["operation"], "generate");
    assert_eq!(document["mode"], "check");
    assert_eq!(document["verdict"], "clean");
    // The receipt is bound to the exact lock digest the check verified.
    assert!(document["lockDigest"]
        .as_str()
        .expect("lockDigest")
        .starts_with("sha256:"));
    assert_eq!(before, after, "a clean drift check adds no files");
}

/// Copy a directory tree recursively (test helper; std has no stable
/// dir copy).
fn fs_extra_copy_dir(from: &std::path::Path, to: &std::path::Path) {
    fs::create_dir_all(to).expect("create target");
    for entry in fs::read_dir(from).expect("read source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            fs_extra_copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

fn count_entries(root: &std::path::Path) -> usize {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for entry in fs::read_dir(dir).expect("read dir") {
            let entry = entry.expect("entry");
            out.push(entry.path());
            if entry.file_type().expect("type").is_dir() {
                walk(&entry.path(), out);
            }
        }
    }
    let mut paths = Vec::new();
    walk(root, &mut paths);
    paths.sort();
    // Count content identities, not metadata churn.
    paths.len()
}

/// The drift operation's findings-bearing receipt (verdict `reported`):
/// authored exactly the way `generate.rs` authors manifests (through the
/// core `GenerateService::inputs` pins), then a custom-lifecycle file is
/// drifted and the read-only check reports it without blocking. The live
/// receipt must equal the committed golden fixture byte-for-byte as JSON
/// (`tests/fixtures/provider/drift-reported.golden.json`): the Node
/// boundary gate Ajv-validates exactly those committed bytes against the
/// published `lekalo/generate-check/v0.6.3` schema, so live equivalence
/// is what makes the golden a faithful representative of the wire class
/// on every clean checkout (no git-ignored capture is involved).
#[test]
fn drift_reported_receipt_matches_the_published_golden() {
    use lekalo_core::artifacts::GenerateService;
    use lekalo_core::digest::sha256_hex;
    use lekalo_core::loader::LoadSelection;
    use serde_json::json;

    const REFERENCE: &str = "../../tests/fixtures/artifacts/project";
    let work = temp_dir("drift-reported");
    let project = work.join("proj");
    fs_extra_copy_dir(
        &std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/artifacts/project"
        )),
        &project,
    );

    // The artifact file drifts after the manifest records it.
    let content = "export function focusHook() {\n  return planner.focusTask();\n}\n";
    let artifact_dir = project.join("apps/api/src/planner");
    fs::create_dir_all(&artifact_dir).expect("artifact dir");
    fs::write(artifact_dir.join("focus-hook.ts"), content).expect("artifact bytes");

    // Explicit lock, then the exact GenerateService::inputs pins. The
    // selection is a relative selector resolved from the test process's
    // cwd (the crate dir) into the git-ignored crate-local target tree,
    // exactly the way `generate.rs` addresses its sandboxes.
    let lock = lekalo_in(
        project.parent().expect("work"),
        &["lock", "--json", "--project", "proj"],
    );
    assert_eq!(lock.status.code(), Some(0), "{lock:?}");
    let relative = format!(
        "target/provider-tests/{}-{}/proj",
        std::path::Path::new(REFERENCE)
            .file_name()
            .expect("fixture name")
            .to_string_lossy(),
        std::process::id()
    );
    // Move the sandbox under the crate-local target tree first: the
    // selector must resolve from the crate dir, but temp_dir lives in the
    // system temp home.
    let sandbox_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(&relative);
    let _ = fs::remove_dir_all(sandbox_root.parent().expect("parent"));
    fs::create_dir_all(sandbox_root.parent().expect("parent")).expect("sandbox parent");
    fs::rename(&project, &sandbox_root).expect("move sandbox into target");
    let project = sandbox_root;
    let selection = LoadSelection {
        project: Some(relative),
    };
    let inputs = GenerateService::inputs(&selection).expect("inputs receipt");
    let mut document = json!({
        "schema_version": "lekalo/artifact-manifest/v0.2.16",
        "identity": "dev.lekalo.artifact-manifest@0.2.16",
        "project_ref": "planner",
        "lock_ref": {
            "schema_version": "lekalo/lock/v0.3.2",
            "digest": inputs.lock_digest,
        },
        "inputs": {
            "model": {"version": inputs.model_version, "digest": inputs.model_digest},
            "ir": {"version": inputs.ir_version, "digest": inputs.ir_digest},
        },
        "artifacts": [{
            "semantic_owner": "planner.focus_task",
            "path": "apps/api/src/planner/focus-hook.ts",
            "artifact_kind": "source",
            "lifecycle": "custom",
            "content": {
                "algorithm": "sha256",
                "digest": format!("sha256:{}", sha256_hex(content.as_bytes())),
                "canonicalization": "exact-file-bytes",
            },
            "input_refs": ["planner.focus_task"],
            "regeneration_policy": "manual-only",
        }],
        "source_maps": [],
    });
    let mut draft = document.clone();
    draft
        .as_object_mut()
        .expect("object")
        .remove("manifest_digest");
    document["manifest_digest"] = json!(format!(
        "sha256:{}",
        sha256_hex(&serde_json::to_vec(&draft).expect("wire"))
    ));
    let manifest_path = project.join(".lekalo/generated/manifests/ownership.json");
    fs::create_dir_all(manifest_path.parent().expect("manifest parent")).expect("manifest dir");
    fs::write(
        &manifest_path,
        serde_json::to_vec(&document).expect("canonical wire"),
    )
    .expect("write manifest");

    // Drift the file, then the read-only check reports it.
    let drifted_dir = project.join("apps/api/src/planner");
    fs::create_dir_all(&drifted_dir).expect("drift artifact dir");
    fs::write(
        drifted_dir.join("focus-hook.ts"),
        "export function focusHook() {\n  return 'drifted';\n}\n",
    )
    .expect("drift bytes");
    let output = lekalo_in(
        project.parent().expect("work"),
        &["generate", "--check", "--json", "--project", "proj"],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "custom drift never blocks: {:?}",
        stderr_text(&output)
    );
    let receipt: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("receipt");
    assert_eq!(receipt["status"], "valid");
    assert_eq!(receipt["operation"], "generate");
    assert_eq!(receipt["mode"], "check");
    assert_eq!(receipt["verdict"], "reported");
    assert_eq!(receipt["findings"][0]["lifecycle"], "custom");
    assert_eq!(receipt["findings"][0]["verdict"], "manual-drift");
    // The live wire bytes are the committed golden, field for field: the
    // receipt carries no host data (logical paths, content-bound digests,
    // fixed field order), so the check is deterministic on every runner.
    let golden: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/provider/drift-reported.golden.json"),
        )
        .expect("committed golden receipt"),
    )
    .expect("golden parses");
    assert_eq!(receipt, golden, "live reported receipt != committed golden");
    let _ = fs::remove_dir_all(&work);
}
