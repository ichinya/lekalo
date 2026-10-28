//! `lekalo-target-python` — observed Python declaration scanner for the
//! `lekalo.target/v1` process protocol (issue #146).
//!
//! One bounded exchange per spawn: `describe` advertises the scan
//! capability ladder; `scan` statically indexes `*.py`/`*.pyi` under the
//! injected `ResolvedProjectProfile` read roots into native symbols,
//! reference edges, and source-bound `any-surface` uncertainty. The
//! adapter never executes Python, never imports the studied package,
//! never writes, and never opens the network.

mod ids;
mod profile;
mod scan;
mod symbols;

use std::io::{Read, Write};

use lekalo_adapter_sdk as sdk;
use sdk::json::{canonical, strict_parse, Value};
use sdk::{build_adapter_identity, is_logical_path, is_request_id, is_sha256_digest, is_token};

const WIRE: &str = "lekalo.target/v1";
const ADAPTER_ID: &str = "lekalo-target-python";
const ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");
const TARGET: &str = "python";
const IR_VERSION: &str = "0.2.16";
/// The declared ladder: the host probes at 0.3.2 and upgrades to the
/// newest common version — 1.1.0 for native-symbol custody.
const DECLARED: &[&str] = &["0.3.2", "0.6.4", "0.9.0", "0.9.1", "1.1.0"];
const REQUEST_FILE_FLAG: &str = "--lekalo-request-file";
const DEFAULT_MAX_OUTPUT: usize = 8 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

fn main() {
    std::process::exit(serve());
}

fn serve() -> i32 {
    let args: Vec<String> = std::env::args().collect();
    let identity = match adapter_identity() {
        Ok(identity) => identity,
        Err(reason) => return fatal(None, &identity_fallback(), reason),
    };
    let profile = match profile::load(&args) {
        Ok(profile) => profile,
        Err(reason) => return fatal(None, &identity, reason),
    };
    let Some(raw) = read_request(&args) else {
        return fatal(None, &identity, "transport-unreadable");
    };
    if raw.len() > MAX_REQUEST_BYTES {
        return fatal(None, &identity, "request-oversize");
    }
    let request = match strict_parse(&raw) {
        Ok(request) => request,
        Err(_) => return fatal(None, &identity, "request-not-json"),
    };
    if let Err(reason) = admit(&request) {
        return fatal(Some(&request), &identity, reason);
    }
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or("");
    let envelope = dispatch(&request, &identity, profile.as_ref(), operation);
    let bytes = canonical(&envelope);
    if bytes.len() > output_ceiling(&request) {
        // A refused-oversize envelope is small by construction; emit it
        // instead of any truncated success.
        let refused = error_envelope(
            &request,
            &identity,
            "infrastructure",
            "output-limit",
            "the response exceeded the declared output bound",
        );
        let bytes = canonical(&refused);
        if bytes.len() > output_ceiling(&request) {
            return 1;
        }
        let _ = std::io::stdout().lock().write_all(bytes.as_bytes());
        return 0;
    }
    let _ = std::io::stdout().lock().write_all(bytes.as_bytes());
    0
}

/// A refusal that still carries a parseable envelope: the host maps a
/// bare non-zero exit to `target.crash`, so every refusal reachable
/// after identity construction emits the closed error row on stdout.
fn fatal(request: Option<&Value>, identity: &Value, reason: &str) -> i32 {
    sdk::logger::error(ADAPTER_ID, &format!("request refused: {reason}"));
    let fallback = Value::Object(vec![]);
    let request = request.unwrap_or(&fallback);
    let envelope = error_envelope(request, identity, "invalid", "request-refused", reason);
    let _ = std::io::stdout()
        .lock()
        .write_all(canonical(&envelope).as_bytes());
    0
}

fn identity_fallback() -> Value {
    Value::Object(vec![
        ("digest".to_owned(), Value::Str("sha256:".to_owned())),
        ("id".to_owned(), Value::Str(ADAPTER_ID.to_owned())),
        ("version".to_owned(), Value::Str(ADAPTER_VERSION.to_owned())),
    ])
}

fn output_ceiling(request: &Value) -> usize {
    request
        .get("limits")
        .and_then(|limits| limits.get("max_output_bytes"))
        .and_then(Value::as_u64)
        .map_or(DEFAULT_MAX_OUTPUT, |declared| {
            (declared as usize).min(DEFAULT_MAX_OUTPUT)
        })
}

/// The one-input contract: a host-owned file coordinate wins; empty
/// stdin without it is the missing-input refusal.
fn read_request(args: &[String]) -> Option<String> {
    if let Some(marker) = args.iter().position(|arg| arg == REQUEST_FILE_FLAG) {
        let path = args.get(marker + 1)?;
        return std::fs::read_to_string(path).ok();
    }
    let mut buffer = Vec::new();
    std::io::stdin().lock().read_to_end(&mut buffer).ok()?;
    if buffer.is_empty() {
        return None;
    }
    String::from_utf8(buffer).ok()
}

/// Structural admission, mirroring the SDK's closed request shape for
/// the operation subset this adapter implements.
fn admit(request: &Value) -> Result<(), &'static str> {
    const ALLOWED: &[&str] = &[
        "dry_run",
        "ir_path",
        "limits",
        "lint_request",
        "native_request",
        "operation",
        "plan_id",
        "profile",
        "profile_capabilities",
        "profile_digest",
        "project_root",
        "protocol",
        "protocol_version",
        "request_id",
        "target",
    ];
    if !request.is_object() {
        return Err("shape");
    }
    for key in request.member_names() {
        if !ALLOWED.contains(&key) {
            return Err("unknown-member");
        }
    }
    for member in [
        "protocol",
        "protocol_version",
        "operation",
        "request_id",
        "project_root",
    ] {
        if request.get(member).is_none() || request.get(member).is_some_and(Value::is_null) {
            return Err("shape");
        }
    }
    if request.get("protocol").and_then(Value::as_str) != Some(WIRE) {
        return Err("token");
    }
    let version = request
        .get("protocol_version")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !DECLARED.contains(&version) {
        return Err("version");
    }
    if !request
        .get("request_id")
        .and_then(Value::as_str)
        .is_some_and(is_request_id)
    {
        return Err("request-id");
    }
    if request.get("project_root").and_then(Value::as_str) != Some(".") {
        return Err("grammar");
    }
    for member in ["ir_path"] {
        if let Some(value) = request.get(member) {
            if value.is_null() || !value.as_str().is_some_and(is_logical_path) {
                return Err("grammar");
            }
        }
    }
    for member in ["target", "profile"] {
        if let Some(value) = request.get(member) {
            if value.is_null() || !value.as_str().is_some_and(is_token) {
                return Err("grammar");
            }
        }
    }
    if let Some(digest) = request.get("profile_digest") {
        if digest.is_null() || !digest.as_str().is_some_and(is_sha256_digest) {
            return Err("bounds");
        }
    }
    if let Some(limits) = request.get("limits") {
        if limits.is_null() || !limits.is_object() {
            return Err("shape");
        }
    }
    Ok(())
}

fn dispatch(
    request: &Value,
    identity: &Value,
    profile: Option<&profile::Profile>,
    operation: &str,
) -> Value {
    match operation {
        "describe" => describe(request, identity, profile),
        "scan" => scan_request(request, identity, profile),
        _ => error_envelope(
            request,
            identity,
            "unsupported",
            "operation-unsupported",
            "the adapter implements describe and scan only",
        ),
    }
}

fn describe(request: &Value, identity: &Value, profile: Option<&profile::Profile>) -> Value {
    let (operations, states, scopes, profiles): (
        Vec<&str>,
        Vec<(&str, &str)>,
        Vec<&str>,
        Vec<String>,
    ) = match profile {
        Some(profile) => (
            vec!["describe", "scan"],
            vec![("scan.symbols", "full")],
            profile.roots.iter().map(|root| root.scope.as_str()).collect(),
            vec![profile.id.clone()],
        ),
        None => (
            vec!["describe"],
            vec![("scan.symbols", "unsupported")],
            vec![],
            vec!["standalone".to_owned()],
        ),
    };
    let capabilities = sdk::build_capabilities(
        identity,
        DECLARED,
        &operations,
        &["stdin", "file"],
        &[TARGET],
        &profiles.iter().map(String::as_str).collect::<Vec<_>>(),
        &scopes,
        &[],
        &[IR_VERSION],
        false,
        Some(states),
        None,
    );
    let capabilities = match capabilities {
        Ok(value) => value,
        Err(_) => {
            return error_envelope(
                request,
                identity,
                "infrastructure",
                "capabilities-refused",
                "the capability builder refused the declaration",
            )
        }
    };
    let mut members = base(request, identity, "ok");
    members.push(("capabilities".to_owned(), capabilities));
    Value::Object(members)
}

fn scan_request(request: &Value, identity: &Value, profile: Option<&profile::Profile>) -> Value {
    let Some(profile) = profile else {
        return error_envelope(
            request,
            identity,
            "invalid",
            "profile-required",
            "scan requires an injected resolved project profile",
        );
    };
    if let Some(target) = request.get("target").and_then(Value::as_str) {
        if target != TARGET {
            return error_envelope(
                request,
                identity,
                "invalid",
                "target-mismatch",
                "the request targets a different adapter",
            );
        }
    }
    if let Some(bound) = request.get("profile").and_then(Value::as_str) {
        if bound != profile.id {
            return error_envelope(
                request,
                identity,
                "invalid",
                "profile-mismatch",
                "the bound profile differs from the injected profile",
            );
        }
    }
    if let Some(digest) = request.get("profile_digest").and_then(Value::as_str) {
        if digest != profile.digest {
            return error_envelope(
                request,
                identity,
                "invalid",
                "profile-digest-mismatch",
                "the bound profile digest differs from the injected profile",
            );
        }
    }
    let version = request
        .get("protocol_version")
        .and_then(Value::as_str)
        .unwrap_or("0.3.2");
    match scan::run(profile, version) {
        Ok(outcome) => {
            let mut members = base(request, identity, "ok");
            // Scan custody binds the evidence to the injected profile
            // from 0.9.0 onward; native uncertainty rows are 1.1.0-only.
            if scan::tier(version) != scan::Tier::Predecessor {
                if let Some((_, evidence)) = members.iter_mut().find(|(k, _)| k == "evidence") {
                    if let Value::Object(rows) = evidence {
                        rows.push((
                            "scan_profile_digest".to_owned(),
                            Value::Str(profile.digest.clone()),
                        ));
                    }
                }
            }
            if scan::tier(version) == scan::Tier::Native {
                if let Some((_, evidence)) = members.iter_mut().find(|(k, _)| k == "evidence") {
                    if let Value::Object(rows) = evidence {
                        rows.push((
                            "native_uncertainty".to_owned(),
                            Value::Array(outcome.uncertainty),
                        ));
                    }
                }
            }
            members.push((
                "result".to_owned(),
                Value::Object(vec![
                    ("entries".to_owned(), Value::Array(outcome.entries)),
                    ("truncated".to_owned(), Value::Bool(outcome.truncated)),
                ]),
            ));
            Value::Object(members)
        }
        Err(reason) => error_envelope(request, identity, "infrastructure", reason, "scan refused"),
    }
}

fn base(request: &Value, identity: &Value, status: &str) -> Vec<(String, Value)> {
    vec![
        (
            "evidence".to_owned(),
            Value::Object(vec![("adapter".to_owned(), identity.clone())]),
        ),
        (
            "operation".to_owned(),
            request.get("operation").cloned().unwrap_or(Value::Null),
        ),
        ("protocol".to_owned(), Value::Str(WIRE.to_owned())),
        (
            "protocol_version".to_owned(),
            request
                .get("protocol_version")
                .cloned()
                .unwrap_or(Value::Null),
        ),
        (
            "request_id".to_owned(),
            request.get("request_id").cloned().unwrap_or(Value::Null),
        ),
        ("status".to_owned(), Value::Str(status.to_owned())),
    ]
}

fn error_envelope(
    request: &Value,
    identity: &Value,
    class: &'static str,
    code: &'static str,
    message: &str,
) -> Value {
    let mut members = base(request, identity, "error");
    members.push((
        "error".to_owned(),
        Value::Object(vec![
            ("class".to_owned(), Value::Str(class.to_owned())),
            ("code".to_owned(), Value::Str(code.to_owned())),
            ("message".to_owned(), Value::Str(message.to_owned())),
        ]),
    ));
    Value::Object(members)
}

/// The evidence adapter block. The digest is the adapter's own binary
/// digest — the same bytes the host confines and digests.
fn adapter_identity() -> Result<Value, &'static str> {
    let seed = std::env::current_exe()
        .ok()
        .and_then(|path| std::fs::read(&path).ok())
        .map(|bytes| ids::digest_hex(&bytes))
        .unwrap_or_else(|| sdk::sha256_hex("lekalo-target-python"));
    build_adapter_identity(ADAPTER_ID, ADAPTER_VERSION, &format!("sha256:{seed}"))
        .map_err(|_| "identity")
}
