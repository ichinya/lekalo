//! Native compiler runtime bundles. Only the executable entry's literal header
//! opts in; wire requests and project files confer no additional authority.
use super::{
    compiler_peer::{self, Role, Session},
    confinement::{Sandbox, SandboxPolicy},
    plan, scopes,
    transport::{AdapterCommand, TransportFailure, TransportLimits, TransportSuccess},
    TargetFailure,
};
use crate::project_fs::Fs;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::AtomicBool;

const HEADER: &[u8] = b"#!/usr/bin/env node\n// lekalo-runtime-bundle: runtime-bundle.json\n";
const MAX_NATIVE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 16 * 1024 * 1024;

pub(crate) struct PackageResource {
    pub path: String,
    pub digest: String,
}

/// Transitive code/data closure of a manifest-pinned runtime document. Both
/// install planning and package admission verify it without launching code.
pub(crate) fn package_files(
    root: &Path,
    manifest: &crate::adapter_package::manifest::ManifestDocument,
) -> Result<Vec<PackageResource>, TargetFailure> {
    let command = AdapterCommand {
        program: "node".into(),
        args: vec![root.join(manifest.entry()).to_string_lossy().into_owned()],
    };
    if !declared(&command) {
        return Ok(Vec::new());
    }
    let fs = Fs::open(root).map_err(|_| refusal())?;
    let pin = manifest
        .files()
        .iter()
        .find(|file| file.path() == "runtime-bundle.json")
        .ok_or_else(refusal)?;
    let data = checked(
        &fs,
        &Resource {
            path: pin.path().to_owned(),
            digest: pin.digest().as_str().to_owned(),
            bytes: pin.bytes() as usize,
        },
        64 * 1024,
    )?;
    let document: Document = serde_json::from_slice(&data).map_err(|_| refusal())?;
    validate_document(&document)?;
    if document.schema_version != "lekalo/compiler-runtime/v1.3.0"
        || document.entry.path != manifest.entry()
        || document.compilers.len() != 5
        || document.libraries.len() != 108
    {
        return Err(refusal());
    }
    let entry = checked(&fs, &document.entry, MAX_ENTRY_BYTES)?;
    if !entry.starts_with(HEADER) {
        return Err(refusal());
    }
    let platform = host_platform();
    let selected: Vec<_> = document
        .compilers
        .iter()
        .filter(|compiler| compiler.platform == platform)
        .collect();
    let [compiler] = selected.as_slice() else {
        return Err(refusal());
    };
    if compiler.kind != "typescript-native"
        || compiler.version != "7.0.2"
        || compiler.resource.path
            != format!(
                "runtime/{}/{}",
                compiler.platform,
                if cfg!(windows) { "tsc.exe" } else { "tsc" }
            )
    {
        return Err(refusal());
    }
    checked(&fs, &compiler.resource, MAX_NATIVE_BYTES)?;
    let mut files = vec![PackageResource {
        path: compiler.resource.path.clone(),
        digest: compiler.resource.digest.clone(),
    }];
    let mut total = 0usize;
    for library in document.libraries {
        let name = library
            .path
            .strip_prefix("runtime/libraries/")
            .ok_or_else(refusal)?;
        if !(name == "lib.d.ts" || name.starts_with("lib.") && name.ends_with(".d.ts"))
            || name.contains(['/', '\\'])
        {
            return Err(refusal());
        }
        total += checked(&fs, &library, 4 * 1024 * 1024)?.len();
        if total > 16 * 1024 * 1024 {
            return Err(refusal());
        }
        files.push(PackageResource {
            path: library.path,
            digest: library.digest,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    if files.windows(2).any(|rows| rows[0].path == rows[1].path) {
        return Err(refusal());
    }
    Ok(files)
}

fn host_platform() -> String {
    format!(
        "{}-{}",
        match std::env::consts::OS {
            "macos" => "darwin",
            "windows" => "win32",
            os => os,
        },
        match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            arch => arch,
        }
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resource {
    path: String,
    digest: String,
    bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Compiler {
    kind: String,
    version: String,
    platform: String,
    resource: Resource,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: String,
    entry: Resource,
    compilers: Vec<Compiler>,
    libraries: Vec<Resource>,
}

fn validate_document(document: &Document) -> Result<(), TargetFailure> {
    if document.schema_version != "lekalo/compiler-runtime/v1.3.0"
        || document.compilers.len() != 5
        || document.libraries.len() != 108
        || !document
            .libraries
            .iter()
            .any(|row| row.path == "runtime/libraries/lib.d.ts")
    {
        return Err(refusal());
    }
    let mut platforms = Vec::new();
    for compiler in &document.compilers {
        let platform = compiler.platform.as_str();
        if compiler.kind != "typescript-native"
            || compiler.version != "7.0.2"
            || ![
                "darwin-arm64",
                "darwin-x64",
                "linux-arm64",
                "linux-x64",
                "win32-x64",
            ]
            .contains(&platform)
            || compiler.resource.path
                != format!(
                    "runtime/{platform}/{}",
                    if platform == "win32-x64" {
                        "tsc.exe"
                    } else {
                        "tsc"
                    }
                )
            || compiler.resource.bytes == 0
            || compiler.resource.bytes > MAX_NATIVE_BYTES
            || !super::wire::is_sha256_digest(&compiler.resource.digest)
        {
            return Err(refusal());
        }
        platforms.push(platform);
    }
    platforms.sort_unstable();
    if platforms.windows(2).any(|rows| rows[0] == rows[1]) {
        return Err(refusal());
    }
    Ok(())
}

#[derive(Serialize)]
struct Context<'a> {
    schema_version: &'static str,
    kind: &'a str,
    version: &'a str,
    platform: &'a str,
    digest: &'a str,
    libraries_digest: String,
}
#[derive(Serialize)]
struct LibraryPin {
    path: String,
    sha256: String,
    bytes: usize,
}

fn refusal() -> TargetFailure {
    TargetFailure::TransportFailed {
        detail: "runtime-bundle",
    }
}
fn read(fs: &Fs, path: &str, bound: usize) -> Result<Vec<u8>, TargetFailure> {
    if !scopes::is_logical_path(path) {
        return Err(refusal());
    }
    let (directory, name) = path.rsplit_once('/').unwrap_or(("", path));
    fs.read_file_opt(directory, name, bound)
        .map_err(|_| refusal())?
        .ok_or_else(refusal)
}
fn checked(fs: &Fs, resource: &Resource, bound: usize) -> Result<Vec<u8>, TargetFailure> {
    if resource.bytes == 0 || resource.bytes > bound {
        return Err(refusal());
    }
    let bytes = read(fs, &resource.path, bound)?;
    if bytes.len() != resource.bytes
        || resource.digest != format!("sha256:{}", plan::sha256_hex(&bytes))
    {
        return Err(refusal());
    }
    Ok(bytes)
}
pub(super) fn declared(command: &AdapterCommand) -> bool {
    command
        .args
        .first()
        .and_then(|name| std::fs::File::open(name).ok())
        .is_some_and(|mut file| {
            use std::io::Read;
            let mut header = [0; HEADER.len()];
            file.read_exact(&mut header).is_ok() && header == HEADER
        })
}

/// Stage verified bytes before either peer starts. The compiler gets an empty
/// project view, no application read roots, and no granted environment pairs.
pub(super) fn run(
    sandbox: &Sandbox,
    command: &AdapterCommand,
    request: &[u8],
    limits: &TransportLimits,
    cancel: Option<&AtomicBool>,
    env: &[(String, String)],
) -> Result<TransportSuccess, TargetFailure> {
    if !sandbox.policy.children_denied
        || sandbox.policy.native_execution
        || sandbox.policy.network_allowlist
    {
        return Err(refusal());
    }
    let entry = Path::new(command.args.first().ok_or_else(refusal)?);
    let parent = entry.parent().ok_or_else(refusal)?;
    let fs = Fs::open(parent).map_err(|_| refusal())?;
    let document: Document = serde_json::from_slice(&read(&fs, "runtime-bundle.json", 64 * 1024)?)
        .map_err(|_| refusal())?;
    validate_document(&document)?;
    if document.schema_version != "lekalo/compiler-runtime/v1.3.0"
        || document.compilers.len() != 5
        || document.entry.path
            != entry
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(refusal)?
    {
        return Err(refusal());
    }
    let entry_bytes = checked(&fs, &document.entry, MAX_ENTRY_BYTES)?;
    if !entry_bytes.starts_with(HEADER) {
        return Err(refusal());
    }
    let platform = format!(
        "{}-{}",
        match std::env::consts::OS {
            "macos" => "darwin",
            "windows" => "win32",
            os => os,
        },
        match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            arch => arch,
        }
    );
    let selected: Vec<_> = document
        .compilers
        .iter()
        .filter(|row| row.platform == platform)
        .collect();
    let [compiler] = selected.as_slice() else {
        return Err(refusal());
    };
    if compiler.kind != "typescript-native" || compiler.version != "7.0.2" {
        return Err(refusal());
    }
    let native_bytes = checked(&fs, &compiler.resource, MAX_NATIVE_BYTES)?;
    let compiler_sandbox = Sandbox::new(
        &sandbox.project,
        &[],
        &[],
        false,
        SandboxPolicy::strict().with_bounded_resources(sandbox.policy.bounded_resources),
    )?;
    let native_name = if cfg!(windows) { "tsc.exe" } else { "tsc" };
    let executable = compiler_sandbox.runtime.join(native_name);
    std::fs::write(&executable, native_bytes).map_err(|_| refusal())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o500))
            .map_err(|_| refusal())?;
    }
    if document.libraries.len() != 108 {
        return Err(refusal());
    }
    let mut library_pins = Vec::new();
    let mut library_bytes = 0usize;
    for library in &document.libraries {
        let name = library
            .path
            .strip_prefix("runtime/libraries/")
            .ok_or_else(refusal)?;
        if !(name == "lib.d.ts" || name.starts_with("lib.") && name.ends_with(".d.ts"))
            || name.contains(['/', '\\'])
        {
            return Err(refusal());
        }
        let data = checked(&fs, library, 4 * 1024 * 1024)?;
        library_bytes += data.len();
        if library_bytes > 16 * 1024 * 1024 {
            return Err(refusal());
        }
        std::fs::write(compiler_sandbox.runtime.join(name), data).map_err(|_| refusal())?;
        library_pins.push(LibraryPin {
            path: name.to_owned(),
            sha256: library.digest[7..].to_owned(),
            bytes: library.bytes,
        });
    }
    library_pins.sort_by(|left, right| left.path.cmp(&right.path));
    if library_pins
        .windows(2)
        .any(|rows| rows[0].path == rows[1].path)
    {
        return Err(refusal());
    }
    let libraries_digest = format!(
        "sha256:{}",
        plan::sha256_hex(&serde_json::to_vec(&library_pins).map_err(|_| refusal())?)
    );
    let staged_entry = sandbox.runtime.join(&document.entry.path);
    std::fs::write(&staged_entry, entry_bytes).map_err(|_| refusal())?;
    let context_value = serde_json::to_value(Context {
        schema_version: "lekalo/compiler-runtime/v1.3.0",
        kind: &compiler.kind,
        version: &compiler.version,
        platform: &compiler.platform,
        digest: &compiler.resource.digest,
        libraries_digest,
    })
    .map_err(|_| refusal())?;
    let context = serde_json::to_string(&context_value).map_err(|_| refusal())?;
    let mut producer = command.clone();
    producer.args[0] = staged_entry.to_string_lossy().into_owned();
    if producer
        .args
        .iter()
        .any(|value| value == "--lekalo-compiler-session-json")
    {
        return Err(refusal());
    }
    producer
        .args
        .extend(["--lekalo-compiler-session-json".into(), context]);
    let native = AdapterCommand {
        program: executable,
        args: vec![
            "--api".into(),
            "--cwd".into(),
            "/lekalo/project".into(),
            "--callbacks=readFile,fileExists,directoryExists,getAccessibleEntries,realpath".into(),
        ],
    };
    let session = Session::new(limits);
    std::thread::scope(|scope| {
        let native_session = session.clone();
        let session_ref = &session;
        let compiler_ref = &compiler_sandbox;
        let native_job = scope.spawn(move || {
            let result = compiler_peer::with(native_session, Role::Compiler, || {
                compiler_ref.run_inner(
                    &native,
                    b"",
                    limits,
                    false,
                    cancel,
                    &[("GOMAXPROCS".into(), "2".into())],
                )
            });
            if !session_ref.is_stopped() {
                session_ref.fail(match &result {
                    Err(TargetFailure::Timeout) => TransportFailure::Timeout,
                    Err(TargetFailure::Cancelled) => TransportFailure::Cancelled,
                    Err(TargetFailure::OutputLimit { stream }) => {
                        TransportFailure::OutputLimit { stream: *stream }
                    }
                    _ => TransportFailure::Spawn,
                });
            }
            result
        });
        let result = compiler_peer::with(session.clone(), Role::Producer, || {
            sandbox.run_inner(&producer, request, limits, true, cancel, env)
        });
        let verified = if result.is_ok() {
            session.verify(cancel)
        } else {
            Ok(())
        };
        session.stop();
        let native_result = native_job.join();
        session.join_relays();
        if native_result.is_err() {
            return Err(refusal());
        }
        if cancel.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            return Err(TargetFailure::Cancelled);
        }
        if let Some(failure) = session.failure() {
            return Err(super::transport_failure(failure));
        }
        verified.map_err(super::transport_failure)?;
        if let Ok(success) = &result {
            if let Ok(response) = serde_json::from_slice::<serde_json::Value>(&success.stdout) {
                if response
                    .get("protocol_version")
                    .and_then(serde_json::Value::as_str)
                    == Some("1.3.0")
                    && response
                        .get("evidence")
                        .and_then(|e| e.get("compiler_runtime"))
                        != Some(&context_value)
                {
                    return Err(refusal());
                }
            }
            sandbox.record_runtime(
                serde_json::from_value(context_value.clone()).map_err(|_| refusal())?,
            );
        }
        result
    })
}
