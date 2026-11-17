//! The same acceptance battery is run by Cargo's automatically discovered CLI
//! test target, so runtime protocol regressions cannot bypass the Rust CI lane.
use std::path::Path;
use std::process::Command;

#[test]
fn unknown_external_runtimes_and_portable_contracts() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut command = Command::new("node");
    // The process CI lane already provisions this exact dependency outside the
    // checkout. Local contributors can use NODE_PATH or their existing modules.
    if let Some(root) = std::env::var_os("RUNNER_TEMP") {
        command.env(
            "NODE_PATH",
            Path::new(&root).join("lekalo-ajv-8.20.0/node_modules"),
        );
    }
    let output = command
        .args(["--test", "scripts/test-external-runtime.mjs"])
        .current_dir(workspace)
        .env("LEKALO_BIN", env!("CARGO_BIN_EXE_lekalo"))
        .output()
        .expect("Node and Git are required by the runtime protocol acceptance tests");
    assert!(
        output.status.success(),
        "runtime acceptance failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
