//! Private scoped project views and OS-enforced execution. The adapter
//! never receives write access to the real project. Only a verified plan
//! is published by core after the isolated process and all its children exit.

use super::{plan, scopes, transport, wire, TargetFailure};
use crate::project_fs::Fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[cfg(windows)]
#[path = "confinement_windows.rs"]
mod windows;

pub(super) struct Sandbox {
    owned: tempfile::TempDir,
    pub project: PathBuf,
    runtime: PathBuf,
    write_roots: Vec<PathBuf>,
}

fn refusal(detail: &'static str) -> TargetFailure {
    TargetFailure::TransportFailed { detail }
}

impl Sandbox {
    pub fn publish(
        &self,
        root: &Path,
        writes: &[wire::WriteEntry],
        before: &plan::Snapshot,
    ) -> Result<(), TargetFailure> {
        use std::io::Write;
        plan::validate_preconditions(writes, before)?;
        let staged = Fs::open(&self.project).map_err(|_| refusal("sandbox-view"))?;
        let real = Fs::open(root).map_err(|_| refusal("project-root"))?;
        // Retain before bytes for ordinary I/O rollback. Files are replaced
        // atomically, never modified in place (including hard-linked files).
        let mut documents = Vec::new();
        for entry in writes {
            let (dir, name) = entry.path.rsplit_once('/').unwrap_or(("", &entry.path));
            let previous = match real.read_file_opt(dir, name, super::version::MAX_FILE_BYTES) {
                Ok(value) => value,
                Err(crate::project_fs::FsErrorKind::NotFound) => None,
                Err(_) => return Err(refusal("publish-preflight")),
            };
            if previous.as_ref().map(|b| plan::sha256_hex(b)).as_ref() != before.get(&entry.path) {
                return Err(TargetFailure::plan_mismatch(None, "before-drift"));
            }
            let after = if entry.action == wire::WriteAction::Delete {
                None
            } else {
                Some(
                    staged
                        .read_file_opt(dir, name, super::version::MAX_FILE_BYTES)
                        .map_err(|_| refusal("publish-preflight"))?
                        .ok_or_else(|| refusal("publish-preflight"))?,
                )
            };
            documents.push((entry, previous, after));
        }
        let mut applied: Vec<usize> = Vec::new();
        let mut directories = Vec::new();
        for (index, (entry, _, bytes)) in documents.iter().enumerate() {
            let result = (|| -> std::io::Result<()> {
                let destination = checked_destination(root, &entry.path, &mut directories)?;
                match bytes {
                    Some(bytes) => {
                        let mut temporary =
                            tempfile::NamedTempFile::new_in(destination.parent().expect("parent"))?;
                        temporary.write_all(bytes)?;
                        temporary.as_file().sync_all()?;
                        if entry.action == wire::WriteAction::Create {
                            temporary
                                .persist_noclobber(&destination)
                                .map_err(|e| e.error)?;
                        } else {
                            temporary.persist(&destination).map_err(|e| e.error)?;
                        }
                    }
                    None => std::fs::remove_file(destination)?,
                }
                Ok(())
            })();
            if result.is_err() {
                let mut partial = false;
                for &done in applied.iter().rev() {
                    let (entry, original, _) = &documents[done];
                    let restored = (|| -> std::io::Result<()> {
                        let destination = checked_destination(root, &entry.path, &mut directories)?;
                        if let Some(original) = original {
                            let mut file = tempfile::NamedTempFile::new_in(
                                destination.parent().expect("parent"),
                            )?;
                            file.write_all(original)?;
                            file.as_file().sync_all()?;
                            file.persist(destination).map_err(|e| e.error)?;
                        } else {
                            std::fs::remove_file(destination)?;
                        }
                        Ok(())
                    })();
                    partial |= restored.is_err();
                }
                for directory in directories.iter().rev() {
                    let _ = std::fs::remove_dir(directory);
                }
                return Err(TargetFailure::OperationFailed {
                    class: wire::ErrorClass::Infrastructure,
                    code: "core-publication".into(),
                    partial,
                });
            }
            applied.push(index);
        }
        Ok(())
    }
    pub fn new(
        root: &Path,
        reads: &[String],
        writes: &[String],
        apply: bool,
    ) -> Result<Self, TargetFailure> {
        let root = std::fs::canonicalize(root).map_err(|_| refusal("project-root"))?;
        #[cfg(windows)]
        windows::check_project_boundary(&root).map_err(|_| refusal("sandbox-project-access"))?;
        #[cfg(target_os = "linux")]
        if ["/usr", "/bin", "/lib", "/lib64", "/proc", "/dev"]
            .iter()
            .any(|p| root.starts_with(p))
        {
            return Err(refusal("sandbox-project-location"));
        }
        #[cfg(target_os = "macos")]
        if ["/System", "/usr/lib", "/dev"]
            .iter()
            .any(|p| root.starts_with(p))
        {
            return Err(refusal("sandbox-project-location"));
        }
        let owned = tempfile::Builder::new()
            .prefix("lekalo-target-sandbox-")
            .tempdir()
            .map_err(|_| refusal("sandbox-create"))?;
        let project = owned.path().join("project");
        let runtime = owned.path().join("runtime");
        std::fs::create_dir(&project)
            .and_then(|()| std::fs::create_dir(&runtime))
            .map_err(|_| refusal("sandbox-create"))?;
        let fs = Fs::open(&root).map_err(|_| refusal("sandbox-input"))?;
        let mut snapshot = plan::snapshot_scopes(&fs, reads).map_err(super::snapshot_rejection)?;
        for (path, digest) in
            plan::snapshot_scopes(&fs, writes).map_err(super::snapshot_rejection)?
        {
            snapshot.insert(path, digest);
        }
        let mut total = 0usize;
        for (path, digest) in snapshot {
            if !scopes::is_logical_path(&path) {
                return Err(refusal("sandbox-path"));
            }
            let dest = project.join(&path);
            if digest == "directory" {
                std::fs::create_dir_all(dest).map_err(|_| refusal("sandbox-copy"))?;
            } else {
                let (dir, name) = path.rsplit_once('/').unwrap_or(("", &path));
                let bytes = fs
                    .read_file_opt(dir, name, super::version::MAX_FILE_BYTES)
                    .map_err(|_| refusal("sandbox-input"))?
                    .ok_or_else(|| refusal("sandbox-input"))?;
                total = total.saturating_add(bytes.len());
                if total > 64 * 1024 * 1024 || plan::sha256_hex(&bytes) != digest {
                    return Err(refusal("sandbox-input"));
                }
                // Write authority reveals only the existing output's shape,
                // never its source bytes. Reading content requires an explicit
                // read scope even for a path that can be replaced or deleted.
                let bytes = if plan::covered_by(&path, reads) {
                    bytes
                } else {
                    Vec::new()
                };
                std::fs::create_dir_all(dest.parent().expect("inside project"))
                    .and_then(|()| std::fs::write(dest, bytes))
                    .map_err(|_| refusal("sandbox-copy"))?;
            }
        }
        let mut write_roots = Vec::new();
        if apply {
            for scope in writes {
                let relative = scope.strip_suffix("/**").unwrap_or(scope);
                let destination = project.join(relative);
                if scope.ends_with("/**") {
                    std::fs::create_dir_all(&destination).map_err(|_| refusal("sandbox-copy"))?;
                } else if !destination.exists() {
                    // Exact absent files need a writable parent to create.
                    // The staged full-tree verifier still permits only the plan.
                    let parent = destination.parent().expect("project child");
                    std::fs::create_dir_all(parent).map_err(|_| refusal("sandbox-copy"))?;
                    write_roots.push(parent.to_path_buf());
                    continue;
                }
                write_roots.push(destination);
            }
        }
        Ok(Self {
            owned,
            project,
            runtime,
            write_roots,
        })
    }

    fn command(
        &self,
        command: &transport::AdapterCommand,
    ) -> Result<transport::AdapterCommand, TargetFailure> {
        let program =
            resolve_program(&command.program).ok_or_else(|| refusal("sandbox-program"))?;
        let filename = program
            .file_name()
            .ok_or_else(|| refusal("sandbox-program"))?;
        let destination = self.runtime.join(filename);
        std::fs::copy(&program, &destination).map_err(|_| refusal("sandbox-runtime"))?;
        let mut args = command.args.clone();
        // A script explicitly supplied as the interpreter's first argument
        // is executable code, copied read-only. Other argv values confer no
        // filesystem grants. Packages with additional runtime assets need a
        // future explicit runtime bundle contract, not an ambient fallback.
        if let Some(first) = args.first_mut() {
            let script = Path::new(first);
            if script.is_file() {
                let script_name = script
                    .file_name()
                    .ok_or_else(|| refusal("sandbox-runtime"))?;
                let target = self.runtime.join(script_name);
                if target == destination {
                    return Err(refusal("sandbox-runtime"));
                }
                std::fs::copy(script, &target).map_err(|_| refusal("sandbox-runtime"))?;
                *first = target.to_string_lossy().into_owned();
            }
        }
        // Node otherwise walks every host ancestor of an absolute main
        // script with lstat. The private runtime contains no links; preserving
        // that spelling avoids granting access to C:\ or user-home ancestors.
        if matches!(filename.to_str(), Some("node" | "node.exe")) {
            args.splice(
                0..0,
                [
                    "--preserve-symlinks".into(),
                    "--preserve-symlinks-main".into(),
                ],
            );
        }
        Ok(transport::AdapterCommand {
            program: destination,
            args,
        })
    }

    pub fn run(
        &self,
        command: &transport::AdapterCommand,
        request: &[u8],
        limits: &transport::TransportLimits,
        file_transport: bool,
        cancel: Option<&AtomicBool>,
    ) -> Result<transport::TransportSuccess, TargetFailure> {
        let _custody = self.owned.path();
        if request.len() > limits.max_request_bytes {
            return Err(refusal("request-size"));
        }
        let mut command = self.command(command)?;
        if file_transport {
            let path = self.runtime.join("request.json");
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&path).map_err(|_| refusal("request-write"))?;
            file.write_all(request)
                .map_err(|_| refusal("request-write"))?;
            command.args.extend([
                "--lekalo-request-file".to_owned(),
                path.to_string_lossy().into_owned(),
            ]);
        }
        #[cfg(windows)]
        let result = windows::run(&command, request, limits, self, cancel);
        #[cfg(target_os = "linux")]
        let result = self.run_linux(&command, request, limits, cancel);
        #[cfg(target_os = "macos")]
        let result = self.run_macos(&command, request, limits, cancel);
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let result = Err(transport::TransportFailure::Spawn);
        result.map_err(super::transport_failure)
    }

    #[cfg(target_os = "linux")]
    fn run_linux(
        &self,
        command: &transport::AdapterCommand,
        request: &[u8],
        limits: &transport::TransportLimits,
        cancel: Option<&AtomicBool>,
    ) -> Result<transport::TransportSuccess, transport::TransportFailure> {
        let mut args: Vec<String> = [
            "--die-with-parent",
            "--unshare-all",
            "--new-session",
            "--clearenv",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
        ]
        .map(str::to_owned)
        .to_vec();
        for root in ["/usr", "/bin", "/lib", "/lib64"] {
            if Path::new(root).exists() {
                args.extend(["--ro-bind".into(), root.into(), root.into()]);
            }
        }
        for root in [&self.runtime, &self.project] {
            args.extend([
                "--ro-bind".into(),
                root.to_string_lossy().into_owned(),
                root.to_string_lossy().into_owned(),
            ]);
        }
        for root in &self.write_roots {
            args.extend([
                "--bind".into(),
                root.to_string_lossy().into_owned(),
                root.to_string_lossy().into_owned(),
            ]);
        }
        args.extend([
            "--chdir".into(),
            self.project.to_string_lossy().into_owned(),
            "--".into(),
            command.program.to_string_lossy().into_owned(),
        ]);
        args.extend(command.args.clone());
        let wrapper = transport::AdapterCommand {
            program: "/usr/bin/bwrap".into(),
            args,
        };
        transport::run_private(&wrapper, request, limits, &self.project, cancel)
    }

    #[cfg(target_os = "macos")]
    fn run_macos(
        &self,
        command: &transport::AdapterCommand,
        request: &[u8],
        limits: &transport::TransportLimits,
        cancel: Option<&AtomicBool>,
    ) -> Result<transport::TransportSuccess, transport::TransportFailure> {
        let wrapper = self.macos_command(command);
        transport::run_private(&wrapper, request, limits, &self.project, cancel)
    }

    #[cfg(target_os = "macos")]
    fn macos_command(&self, command: &transport::AdapterCommand) -> transport::AdapterCommand {
        let quoted = |p: &Path| serde_json::to_string(&p.to_string_lossy()).expect("string");
        let mut profile = format!("(version 1)(deny default)(allow process-exec)(allow sysctl-read)(allow mach-lookup (global-name \"com.apple.system.logger\"))(allow file-read* (subpath \"/System\") (subpath \"/usr/lib\") (subpath \"/dev\") (subpath {}) (subpath {}))", quoted(&self.runtime), quoted(&self.project));
        for root in &self.write_roots {
            profile.push_str(&format!("(allow file-write* (subpath {}))", quoted(root)));
        }
        let mut args = vec![
            "-p".into(),
            profile,
            command.program.to_string_lossy().into_owned(),
        ];
        args.extend(command.args.clone());
        transport::AdapterCommand {
            program: "/usr/bin/sandbox-exec".into(),
            args,
        }
    }
}

fn checked_destination(
    root: &Path,
    logical: &str,
    created: &mut Vec<PathBuf>,
) -> std::io::Result<PathBuf> {
    let mut current = root.to_path_buf();
    let segments: Vec<_> = logical.split('/').collect();
    for (index, segment) in segments.iter().enumerate() {
        current.push(segment);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                let link = metadata.file_type().is_symlink();
                #[cfg(windows)]
                let link = {
                    use std::os::windows::fs::MetadataExt;
                    link || metadata.file_attributes() & 0x400 != 0
                };
                if link
                    || (index + 1 < segments.len() && !metadata.is_dir())
                    || (index + 1 == segments.len() && !metadata.is_file())
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "project-entry",
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if index + 1 < segments.len() {
                    std::fs::create_dir(&current)?;
                    created.push(current.clone());
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(current)
}

fn resolve_program(program: &Path) -> Option<PathBuf> {
    if program.is_absolute() || program.components().count() > 1 {
        return program.is_file().then(|| program.to_path_buf());
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = candidate.with_extension("exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Qualification-only evidence from fixed code and owned synthetic data.
    // Product diagnostics must continue to discard raw backend/provider stderr.
    #[cfg(unix)]
    #[test]
    fn unix_confined_runtime_qualification() {
        let root = tempfile::tempdir().unwrap();
        let sandbox = Sandbox::new(root.path(), &[], &[], false).unwrap();
        let command = transport::AdapterCommand {
            program: "node".into(),
            args: vec![
                "-e".into(),
                "process.stdout.write('lekalo-confined-node-ok')".into(),
            ],
        };
        let limits = transport::TransportLimits {
            timeout_ms: 5_000,
            max_output_bytes: 1_024,
            max_stderr_bytes: 8_192,
            ..Default::default()
        };
        let result = sandbox.run(&command, b"", &limits, false, None).unwrap();
        #[cfg(target_os = "macos")]
        if result.exit_code != 0 {
            let staged = sandbox.command(&command).unwrap();
            let canonical = std::fs::canonicalize(sandbox.owned.path()).unwrap();
            for (name, extra, canonicalize) in [
                ("canonical-only", "", true),
                (
                    "root-metadata",
                    "(allow file-read-metadata (literal \"/\"))",
                    true,
                ),
                ("root-read", "(allow file-read* (literal \"/\"))", true),
                (
                    "root-read-original-paths",
                    "(allow file-read* (literal \"/\"))",
                    false,
                ),
            ] {
                let mut wrapper = sandbox.macos_command(&staged);
                if canonicalize {
                    wrapper.args[1] = wrapper.args[1].replace(
                        sandbox.owned.path().to_str().unwrap(),
                        canonical.to_str().unwrap(),
                    );
                }
                wrapper.args[1].push_str(extra);
                let variant =
                    transport::run_private(&wrapper, b"", &limits, &sandbox.project, None);
                eprintln!("private synthetic macOS variant {name}: {variant:?}");
            }
        }
        assert_eq!(
            result.exit_code,
            0,
            "private synthetic launch evidence: stderr={:?}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, b"lekalo-confined-node-ok");

        // Loading a copied module and reading the request exercise runtime
        // paths that the inline startup control above does not touch.
        let sandbox = Sandbox::new(root.path(), &[], &[], false).unwrap();
        let command = transport::AdapterCommand {
            program: "node".into(),
            args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/target-protocol/fake-adapter.mjs")
                .to_string_lossy()
                .into_owned()],
        };
        let request = include_bytes!(
            "../../../../tests/fixtures/target-protocol/valid/describe-request.json"
        );
        let limits = transport::TransportLimits {
            max_output_bytes: 8_192,
            ..limits
        };
        let result = sandbox
            .run(&command, request, &limits, false, None)
            .unwrap();
        assert_eq!(
            result.exit_code,
            0,
            "private synthetic adapter evidence: stderr={:?}",
            String::from_utf8_lossy(&result.stderr)
        );
        let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(response["operation"], "describe");
    }

    #[test]
    fn isolated_node_handshake_has_a_working_positive_control() {
        let root = tempfile::tempdir().unwrap();
        let mut client = super::super::TargetClient::default();
        let command = transport::AdapterCommand {
            program: "node".into(),
            args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/target-protocol/fake-adapter.mjs")
                .to_string_lossy()
                .into_owned()],
        };
        let result = client.describe(&command, root.path());
        assert!(result.is_ok(), "{result:?}");
    }
}
