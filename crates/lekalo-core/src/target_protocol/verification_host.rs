//! Native verification uses the same deny-default OS boundary as adapters.
//! Only a new private copy is writable; this API never publishes its files.

use super::confinement::{Sandbox, SandboxPolicy};
use super::transport::{AdapterCommand, TransportLimits, TransportSuccess};
use super::TargetFailure;
use std::path::Path;

/// Execute a verifier over an isolated copy of its disposable project.
/// Network access is denied and children remain confined. An unavailable
/// sandbox is an error; there is no ambient-process fallback.
pub fn run_confined_verification(
    command: &AdapterCommand,
    root: &Path,
    limits: &TransportLimits,
    env: &[(String, String)],
) -> Result<TransportSuccess, TargetFailure> {
    let refused = || TargetFailure::ScopeViolation {
        path: None,
        detail: "verification-input",
    };
    let root = std::fs::canonicalize(root).map_err(|_| refused())?;
    let mut scopes = Vec::new();
    for entry in std::fs::read_dir(&root).map_err(|_| refused())? {
        let entry = entry.map_err(|_| refused())?;
        let name = entry.file_name().into_string().map_err(|_| refused())?;
        if !super::scopes::is_logical_path(&name) {
            return Err(refused());
        }
        let metadata = std::fs::symlink_metadata(entry.path()).map_err(|_| refused())?;
        if metadata.file_type().is_symlink() {
            return Err(refused());
        }
        scopes.push(if metadata.is_dir() {
            format!("{name}/**")
        } else if metadata.is_file() {
            name
        } else {
            return Err(refused());
        });
    }
    scopes.sort();
    // Both read and write scopes refer exclusively to the private stage.
    // The caller's original disposable copy is itself read-only to the
    // child, as are the actual project and all external/runtime paths.
    let mut policy = SandboxPolicy::strict();
    // Native test runners invoke their test/oracle children. They inherit
    // the same namespace/seatbelt/LPAC boundary and owned process tree.
    policy.children_denied = false;
    let sandbox = Sandbox::new(&root, &scopes, &scopes, true, policy)?;
    let mut command = command.clone();
    if let Some(first) = command.args.first_mut() {
        let script = root.join(&*first);
        if script.is_file() {
            let script = std::fs::canonicalize(script).map_err(|_| refused())?;
            let relative = script.strip_prefix(&root).map_err(|_| refused())?;
            *first = sandbox
                .project_root()
                .join(relative)
                .to_string_lossy()
                .into_owned();
        }
    }
    sandbox.run(&command, b"", limits, false, None, env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_verifier_children_execute_inside_the_private_stage() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        let mut policy = SandboxPolicy::strict();
        policy.children_denied = false;
        let scopes = vec!["src/**".to_owned()];
        let sandbox = Sandbox::new(root.path(), &scopes, &scopes, true, policy).unwrap();
        let command = AdapterCommand {
            program: "node".into(),
            args: vec!["-e".to_owned(), r#"
const fs = require('node:fs');
const cp = require('node:child_process');
fs.writeFileSync('src/before', process.execPath);
const childCode = 'let blocked=false;try{require("node:fs").writeFileSync(process.argv[1],"escaped")}catch{blocked=true}if(!blocked)process.exit(9);process.stdout.write("CHILD-OK")';
const child = cp.spawnSync(process.execPath, ['-e', childCode, process.argv[1]], {stdio:'inherit', timeout:2000});
fs.writeFileSync('src/result', JSON.stringify({status:child.status,stdout:child.stdout,stderr:child.stderr,error:child.error?.message}));
if (child.status !== 0) process.exit(1);
process.stdout.write('VERIFIER-OK');
"#.to_owned(), root.path().join("outside-marker").to_string_lossy().into_owned()],
        };
        let limits = TransportLimits {
            timeout_ms: 6000,
            max_output_bytes: 4096,
            max_stderr_bytes: 4096,
            max_request_bytes: 1,
        };
        let result = sandbox.run(&command, b"", &limits, false, None, &[]);
        let before = std::fs::read_to_string(sandbox.project_root().join("src/before"));
        let child_result = std::fs::read_to_string(sandbox.project_root().join("src/result"));
        assert!(
            result
                .as_ref()
                .is_ok_and(|value| value.exit_code == 0 && value.stdout == b"CHILD-OKVERIFIER-OK"),
            "result={result:?}; before={before:?}; child={child_result:?}"
        );
        assert!(
            !root.path().join("outside-marker").exists(),
            "the child cannot write the original root"
        );
    }
}
