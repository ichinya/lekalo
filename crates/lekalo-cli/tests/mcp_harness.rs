//! The shared stdio harness of the issue #33 wire tests: one disposable
//! served root (the hermetic planner plus the trace manifest under
//! `traces/` and the trusted catalog admitting it), the real server child
//! over a real pipe, and closed accessors over the session output.
//!
//! Every child runs through the alias-free temp spelling: GitHub's
//! Windows runners export `%TEMP%` with the 8.3 profile alias and the
//! selection policy denies alias spellings before any command logic.

#![allow(dead_code)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const PLANNER: &str = "tests/fixtures/inspect/planner";
pub const TRACE: &str = "tests/fixtures/trace/full.trace.json";

/// The alias-free spelling of one path (the selection policy denies
/// 8.3-alias directories before any command logic).
pub fn alias_free(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

pub fn workspace_root() -> PathBuf {
    alias_free(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

/// A disposable served root: a fresh copy of the hermetic planner plus
/// the trace manifest under `traces/`, and the trusted catalog that
/// admits it. The server child runs with its CWD at this root.
pub struct ServedRoot {
    pub root: PathBuf,
}

impl Default for ServedRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl ServedRoot {
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("temp root").keep();
        let planner = root.join("planner");
        copy_dir(&workspace_root().join(PLANNER), &planner);
        std::fs::create_dir_all(planner.join("traces")).expect("traces dir");
        std::fs::copy(
            workspace_root().join(TRACE),
            planner.join("traces/full.trace.json"),
        )
        .expect("trace fixture");
        let config = r#"{
  "schemaVersion": "lekalo/mcp-server-config/v0.8.0",
  "identity": "dev.lekalo.mcp-server-config@0.8.0",
  "projects": [{"id": "planner", "root": "planner"}],
  "groups": [{"id": "planner-review", "projectId": "planner", "modules": ["planner"]}],
  "manifests": [{"id": "baseline", "projectId": "planner", "path": "traces/full.trace.json"}],
  "git": {"changedMode": false}
}"#;
        std::fs::write(root.join("mcp.local.json"), config).expect("catalog config");
        Self { root }
    }

    /// The `--project` operand for single-project startup.
    pub fn selector(&self) -> &'static str {
        "planner"
    }

    /// Run one server session over the given frames (each already a
    /// complete LF-terminated string list), from the served root, with
    /// the plain single-project catalog.
    pub fn serve(&self, frames: &[String], extra_args: &[&str]) -> SessionOutput {
        self.serve_with_catalog(None, frames, extra_args)
    }

    pub fn serve_with_catalog(
        &self,
        config: Option<&str>,
        frames: &[String],
        extra_args: &[&str],
    ) -> SessionOutput {
        let mut args: Vec<String> =
            vec!["mcp".to_owned(), "serve".to_owned(), "--stdio".to_owned()];
        match config {
            Some(path) => {
                args.push("--catalog-config".to_owned());
                args.push(path.to_owned());
            }
            None => {
                args.push("--project".to_owned());
                args.push(self.selector().to_owned());
            }
        }
        args.extend(extra_args.iter().map(|argument| (*argument).to_owned()));
        let mut child = Command::new(env!("CARGO_BIN_EXE_lekalo"))
            .args(&args)
            .current_dir(alias_free(&self.root))
            .env_remove("LEKALO_PROJECT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the server");
        {
            let stdin = child.stdin.as_mut().expect("stdin");
            for frame in frames {
                stdin.write_all(frame.as_bytes()).expect("write frame");
                stdin.write_all(b"\n").expect("write frame LF");
            }
        }
        let output = child.wait_with_output().expect("server session");
        SessionOutput {
            stdout: String::from_utf8(output.stdout).expect("stdout utf8"),
            stderr: String::from_utf8(output.stderr).expect("stderr utf8"),
            code: output.status.code().unwrap_or_default(),
        }
    }

    /// The full recursive file inventory of the served root with each
    /// file's FNV-1a digest. Path spellings are lowercase forward-slash
    /// relative names so the before/after comparison is platform-stable.
    pub fn inventory(&self) -> Vec<(String, String)> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).expect("read dir") {
                let entry = entry.expect("entry");
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, base, out);
                } else {
                    let bytes = std::fs::read(&path).expect("read file");
                    let relative = path
                        .strip_prefix(base)
                        .expect("inside root")
                        .to_string_lossy()
                        .replace('\\', "/")
                        .to_lowercase();
                    out.push((relative, format!("{:x}", md_slice(&bytes))));
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &self.root, &mut out);
        out.sort();
        out
    }
}

/// FNV-1a over the bytes: a stable, dependency-free content digest for
/// inventory equality (not a published contract digest).
fn md_slice(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn copy_dir(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target");
    for entry in std::fs::read_dir(source).expect("read source") {
        let entry = entry.expect("entry");
        let path = entry.path();
        let destination = target.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &destination);
        } else {
            std::fs::copy(&path, &destination).expect("copy file");
        }
    }
}

pub struct SessionOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl SessionOutput {
    /// The parsed JSON-RPC responses; stdout must be frames only.
    pub fn responses(&self) -> Vec<serde_json::Value> {
        let mut parsed = Vec::new();
        for line in self.stdout.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let value: serde_json::Value = serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("stdout line is one JSON frame: {error}: {line}"));
            parsed.push(value);
        }
        parsed
    }

    pub fn response(&self, id: i64) -> serde_json::Value {
        self.responses()
            .into_iter()
            .find(|response| response["id"] == id)
            .unwrap_or_else(|| panic!("no response for id {id}"))
    }

    /// The response for one id, when it exists (cancellation suppresses).
    pub fn response_opt(&self, id: i64) -> Option<serde_json::Value> {
        self.responses()
            .into_iter()
            .find(|response| response["id"] == id)
    }

    /// The structured tool output for one tools/call id.
    pub fn tool(&self, id: i64) -> serde_json::Value {
        self.response(id)["result"]["structuredContent"].clone()
    }

    pub fn error_code(&self, id: i64) -> Option<i64> {
        self.response(id)["error"]["code"].as_i64()
    }

    /// The error code of the null-id bounded response (over-limit or
    /// malformed frames cannot carry a safe id).
    pub fn null_id_error_code(&self) -> Option<i64> {
        self.responses()
            .iter()
            .find(|response| response["id"].is_null())
            .and_then(|response| response["error"]["code"].as_i64())
    }
}

pub fn frame(id: i64, method: &str, params: serde_json::Value) -> String {
    serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string()
}

pub fn initialize(id: i64) -> String {
    frame(
        id,
        "initialize",
        serde_json::json!({"protocolVersion": "2025-11-25"}),
    )
}

pub fn call(id: i64, name: &str, arguments: serde_json::Value) -> String {
    frame(
        id,
        "tools/call",
        serde_json::json!({"name": name, "arguments": arguments}),
    )
}

pub fn lifecycle() -> Vec<String> {
    vec![
        initialize(1),
        serde_json::json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string(),
    ]
}

/// The pinned lifecycle frames under the harness' own module path.
#[allow(non_snake_case)]
pub fn FRAME_LIFECYCLE() -> Vec<String> {
    lifecycle()
}

/// Run one ordinary lekalo command (used by the CLI-independence tests).
pub fn lekalo_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .current_dir(alias_free(dir))
        .env_remove("LEKALO_PROJECT")
        .output()
        .expect("run the real lekalo binary")
}

pub fn stdout_text(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout utf8")
}

pub fn stderr_text(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr utf8")
}

pub fn exit_code(output: &Output) -> u8 {
    output.status.code().expect("exit code") as u8
}
