//! Verify real host selection and executable bytes on every CI runner.
use serde_json::{json, Value};
use std::{fs, path::Path, process::Command};

fn write(root: &Path, name: &str, value: &Value) {
    fs::write(root.join(name), serde_json::to_vec(value).unwrap()).unwrap();
}

#[test]
fn offline_lock_selects_the_host_and_checks_its_executable() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path();
    let command = "./provider";
    let executable = if cfg!(windows) {
        "provider.exe"
    } else {
        "provider"
    };
    // Offline verification hashes the executable; nothing is launched.
    let executable_bytes = b"platform fixture";
    fs::write(root.join(executable), executable_bytes).unwrap();
    write(
        root,
        "lekalo.yaml",
        &json!({
            "schema": "lekalo/runtime-config/v1",
            "components": {"app": {"path": "src", "runtime": {"command": [command]}}}
        }),
    );
    let configuration = lekalo_core::runtime::load(root).unwrap();
    let digest = lekalo_core::runtime::digest(&serde_json::to_vec(&configuration).unwrap());
    let row = json!({
        "schema": "lekalo/runtime-lock/v2", "config_digest": digest,
        "components": {"app": {"rules": [], "runtime": {
            "command": [command],
            "files": {"0": lekalo_core::runtime::digest(executable_bytes)},
            "description": {"id": "fixture-provider", "version": "1.0.0",
                "rules_api": [], "capabilities": [], "operations": ["check"]},
            "package": null
        }}}
    });
    let host = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let other = if std::env::consts::OS == "linux" {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    };
    let mut foreign = row.clone();
    foreign["components"]["app"]["runtime"]["files"]["0"] = json!(lekalo_core::runtime::digest(
        b"a different platform's executable"
    ));
    let mut document = json!({
        "schema": "lekalo/runtime-lock/v3", "config_digest": digest,
        "platforms": {(host.clone()): row, (other): foreign}
    });
    let offline = || {
        Command::new(env!("CARGO_BIN_EXE_lekalo"))
            .args(["--json", "runtime", "lock", "--offline", "--project"])
            .arg(root)
            .output()
            .unwrap()
    };
    let refusal = |detail: &str| {
        let result = offline();
        assert!(!result.status.success());
        let report: Value = serde_json::from_slice(&result.stderr).unwrap();
        assert_eq!(report["diagnostics"][0]["data"]["detail"], detail);
    };
    write(root, "lekalo.runtime.lock", &document);
    let result = offline();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["lock"], document);
    fs::write(root.join(executable), b"changed bytes").unwrap();
    refusal("runtime-changed");
    document["platforms"].as_object_mut().unwrap().remove(&host);
    write(root, "lekalo.runtime.lock", &document);
    refusal("lock-platform-unavailable");
}
