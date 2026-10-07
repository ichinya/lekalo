//! The per-exchange process lifetime (issue #106): read exactly one
//! bounded request from stdin or the host-owned `--lekalo-request-file`
//! coordinate, decode and validate it, dispatch one operation callback,
//! and emit exactly one compact canonical response on stdout.
//!
//! The one input-source contract (implemented identically in all four
//! runtimes): the host-owned file coordinate wins whenever present. A
//! piped stdin alongside it is the ordinary host shape (the host always
//! spawns adapters with piped stdio), so pipe-ness is never ambiguous.
//! Empty stdin without a file flag is the only missing-input refusal.

use std::io::{Read, Write};

use crate::build::{build_error, build_success, response_bytes, BuildError};
use crate::decode::{admit_request, validate_request, Refusal};
use crate::generated::protocol;
use crate::json::{canonical, Value};
use crate::logger;

/// The handler output of one operation: builder fragments.
#[derive(Default)]
pub struct Output {
    pub capabilities: Option<Value>,
    pub payload: Option<Value>,
    pub plan_id: Option<String>,
    pub progress: Option<Value>,
    pub writes: Option<Value>,
    /// A closed in-envelope error the handler chose deliberately.
    pub error: Option<(&'static str, &'static str, String)>,
}

/// The adapter contract: one identity plus one handler per operation.
pub trait Adapter {
    fn identity(&self) -> Value;
    fn handle(&self, operation: &str, request: &Value) -> Output;
}

fn read_request_text() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(marker) = args.iter().position(|arg| arg == "--lekalo-request-file") {
        let path = args.get(marker + 1)?;
        return std::fs::read_to_string(path).ok();
    }
    let mut buffer = Vec::new();
    std::io::stdin().lock().read_to_end(&mut buffer).ok()?;
    String::from_utf8(buffer).ok()
}

/// Serve exactly one exchange; returns the process exit code.
pub fn serve(adapter: &dyn Adapter) -> i32 {
    let identity = adapter.identity();
    let tag = identity
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("adapter");
    let Some(raw) = read_request_text() else {
        logger::error(tag, "transport: request input unreadable");
        return 1;
    };
    let request = match admit_request(&raw) {
        Ok(request) => request,
        Err(refusal) => {
            logger::error(
                tag,
                &format!(
                    "request refused: {}{}",
                    refusal.reason,
                    refusal_detail(&refusal)
                ),
            );
            return 1;
        }
    };
    if let Err(refusal) = validate_request(&request) {
        logger::error(tag, &format!("request refused: {}", refusal.reason));
        return 1;
    }
    let operation = request
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let output = adapter.handle(&operation, &request);
    let response = match emit_response(&request, adapter, &operation, output) {
        Ok(response) => response,
        Err(reason) => {
            logger::error(tag, &format!("response refused: {reason}"));
            match build_error(
                &request,
                &identity,
                "infrastructure",
                "sdk-build-refused",
                "the adapter refused to emit an invalid response",
                None,
            ) {
                Ok(envelope) => envelope,
                Err(_) => return 1,
            }
        }
    };
    {
        // Bounded emission (issue #106): the completed envelope is admitted
        // (builders already validate the full closed contract) and
        // serialized into a bounded buffer before a single stdout byte is
        // written. The ceiling is the accepted default OR the request's
        // declared limits.max_output_bytes, whichever is tighter; a
        // response over the ceiling refuses here — never a truncated ok.
        let bytes = response_bytes(&response);
        let mut ceiling = protocol::bounds::DEFAULT_MAX_OUTPUT_BYTES as usize;
        if let Some(declared) = request
            .get("limits")
            .and_then(|limits| limits.get("max_output_bytes"))
            .and_then(Value::as_u64)
        {
            if declared >= 1 && (declared as usize) < ceiling {
                ceiling = declared as usize;
            }
        }
        if bytes.len() > ceiling {
            logger::error(tag, "response refused: output-limit");
            return 1;
        }
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(bytes.as_bytes());
        let _ = stdout.flush();
    }
    0
}

fn refusal_detail(_refusal: &Refusal) -> String {
    // Refusals carry only the closed safe reason token; payload content
    // never reaches stderr (the decode/validate split exists so a future
    // bounded detail can land in exactly one place).
    String::new()
}

fn emit_response(
    request: &Value,
    adapter: &dyn Adapter,
    operation: &str,
    output: Output,
) -> Result<Value, String> {
    let identity = adapter.identity();
    if let Some((class, code, message)) = output.error {
        return build_error(request, &identity, class, code, &message, None)
            .map_err(|error| error.reason);
    }
    if operation == "describe" && output.capabilities.is_none() {
        return Err("member".to_owned());
    }
    build_success(
        request,
        &identity,
        output.capabilities.as_ref(),
        output.payload.as_ref(),
        output.writes.as_ref(),
        output.progress.as_ref(),
        output.plan_id.as_deref(),
    )
    .map_err(|error: BuildError| error.reason)
}

#[cfg(unix)]
#[expect(dead_code)]
fn is_tty_stdin() -> bool {
    // No portable tty probe without libc. Diagnostics-only since issue
    // #106: pipe-ness never refuses an exchange (the host always spawns
    // adapters with piped stdin, including on file transport), so the
    // serve path no longer consults this.
    false
}

#[cfg(not(unix))]
#[expect(dead_code)]
fn is_tty_stdin() -> bool {
    false
}

/// The cooperative deadline token surface (host termination stays the
/// host's responsibility; the token exposes the accepted request limit).
pub struct DeadlineToken {
    pub deadline_ms: Option<u64>,
}

impl DeadlineToken {
    pub fn from_request(request: &Value) -> Self {
        let deadline_ms = request
            .get("limits")
            .and_then(|limits| limits.get("timeout_ms"))
            .and_then(Value::as_u64);
        Self { deadline_ms }
    }
}

/// Wire-protocol constant re-export for the serve callers.
pub const WIRE_TOKEN: &str = protocol::WIRE_TOKEN;

/// Canonical bytes of any closed value (parity helper).
pub fn canonical_bytes(value: &Value) -> String {
    canonical(value)
}
