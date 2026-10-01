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
        "lekalo/workflow-provider/v0.6.3"
    );
    assert_eq!(
        document["manifest"]["identity"],
        "dev.lekalo.workflow-provider@0.6.3"
    );
    assert_eq!(document["manifest"]["productVersion"], "0.6.3");
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
    assert_eq!(schema_of("validate"), "lekalo/validation-profile/v0.4.0");
    assert_eq!(schema_of("verify"), "lekalo/orchestration/v0.2.16");
    assert_eq!(schema_of("generate"), "lekalo/orchestration/v0.2.16");
    assert_eq!(schema_of("trace.export"), "lekalo/trace-manifest/v0.2.16");
    // The pins cover the same families exactly.
    let pins: Vec<&str> = document["manifest"]["schemaPins"]
        .as_array()
        .expect("pins")
        .iter()
        .map(|pin| pin["schemaVersion"].as_str().expect("pin"))
        .collect();
    assert_eq!(pins.len(), 7);
    assert!(
        pins.contains(&"lekalo/diagnostic/v0.2.16"),
        "diagnostics are pinned"
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
        "provider dev.lekalo.workflow-provider@0.6.3 product 0.6.3 operations \
         context,doctor,generate,impact,readiness,status,trace.export,validate,verify\n"
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
