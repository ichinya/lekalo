//! Extensibility and boundary acceptance through the built CLI.
#[test]
fn external_services_are_explicit_locked_and_confined() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut command = std::process::Command::new("node");
    if let Some(root) = std::env::var_os("RUNNER_TEMP") {
        command.env(
            "NODE_PATH",
            std::path::Path::new(&root).join("lekalo-ajv-8.20.0/node_modules"),
        );
    }
    let output = command
        .args(["--test", "scripts/test-external-services.mjs"])
        .current_dir(root)
        .env("LEKALO_BIN", env!("CARGO_BIN_EXE_lekalo"))
        .output()
        .expect("Node is required");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
