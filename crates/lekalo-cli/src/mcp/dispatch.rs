//! The JSON-RPC dispatch for the MCP server (issue #33): the pinned
//! 2025-11-25 lifecycle over stdio.
//!
//! After `initialize`/`notifications/initialized`, the served methods are
//! ping, tools/list, tools/call, resources/list, resources/templates/list,
//! resources/read, resources/subscribe, and resources/unsubscribe. Only
//! implemented capabilities are advertised: tools only, with resource
//! subscribe/listChanged false (F8) until a bounded watcher ships; no
//! prompts, sampling, elicitation, tasks, HTTP transport, or client-driven
//! root expansion. The launcher `--group` is the disclosure ceiling of
//! every tool call and resource read (F3/F4); inbound frames refuse
//! duplicate keys (F10); the lifecycle is enforced before any other
//! method (F12). A timeout produces a typed bounded tool error while the
//! client is still waiting and the abandoned worker keeps holding its
//! concurrency slot until it finishes (F14/F19); a cancellation
//! notification suppresses the late success and never gets a reply.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use lekalo_core::mcp::policy::Limits;
use lekalo_core::mcp::resources::{self, Cursor, Family, UriScope};
use lekalo_core::mcp::tools::{self, parse_request, RequestError};

use super::catalog::ServerCatalog;
use super::execution;
use super::stdio::log_event;
use super::stdio::Writer;

/// The slot-ownership states of one dispatched worker (`SLOT_BUSY` while
/// the worker still holds its concurrency slot; `SLOT_RELEASED` once the
/// dispatcher or the worker handed it back — exactly once).
const SLOT_BUSY: usize = 0;
const SLOT_RELEASED: usize = 1;

/// The JSON-RPC protocol codes this server uses.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// One live server session: the immutable catalog plus the mutable
/// lifecycle state (subscriptions, cancellation, admission counters).
pub struct Session {
    pub catalog: ServerCatalog,
    pub limits: Limits,
    /// The trusted launcher group: the default disclosure scope of every
    /// tool call that does not name one explicitly.
    pub default_group: Option<String>,
    pub writer: Writer,
    initialized: Mutex<bool>,
    cancelled: Mutex<BTreeSet<String>>,
    subscriptions: Mutex<BTreeSet<String>>,
    live: AtomicUsize,
    /// Workers abandoned after their deadline: they keep executing, so
    /// they stay counted against the concurrency bound until they
    /// finish (their slot is never freed early).
    detached: AtomicUsize,
}

impl Session {
    /// One admitted session over the resolved catalog.
    pub fn new(
        catalog: ServerCatalog,
        limits: Limits,
        default_group: Option<String>,
        writer: Writer,
    ) -> Self {
        Self {
            catalog,
            limits,
            default_group,
            writer,
            initialized: Mutex::new(false),
            cancelled: Mutex::new(BTreeSet::new()),
            subscriptions: Mutex::new(BTreeSet::new()),
            live: AtomicUsize::new(0),
            detached: AtomicUsize::new(0),
        }
    }
}

/// The incoming request header, structurally validated before routing.
struct Incoming {
    id: Value,
    method: String,
    params: Option<Value>,
}

fn header_from_value(document: &Value) -> Option<Incoming> {
    let method = document.get("method")?.as_str()?.to_owned();
    Some(Incoming {
        id: document.get("id").cloned().unwrap_or(Value::Null),
        method,
        params: document.get("params").cloned(),
    })
}

/// Serve one stdio session to EOF. Requests are processed sequentially
/// with a per-request deadline; a timed-out worker is abandoned with its
/// result suppressed (the receiver is dropped), so one cancelled or
/// timed-out request never poisons the session.
pub fn serve(session: Arc<Session>) {
    let stdin = std::io::stdin();
    let mut reader = stdin.lock();
    loop {
        let frame = match super::stdio::read_frame(&mut reader) {
            Ok(frame) => frame,
            Err(super::stdio::FrameError::Eof) => break,
            Err(super::stdio::FrameError::OverLimit) => {
                // A bounded error, never a partial JSON frame; the id is
                // unknowable without parsing, so the error id is null.
                session
                    .writer
                    .send_error(&Value::Null, INVALID_REQUEST, "mcp.request-limit");
                continue;
            }
            Err(_) => {
                session
                    .writer
                    .send_error(&Value::Null, PARSE_ERROR, "parse-error");
                continue;
            }
        };
        // One checked parse: duplicate keys are refused before any map
        // insertion (last-key-wins is never an admission path), then the
        // nesting depth, then the header.
        let document = match lekalo_core::mcp::parse_checked(&frame) {
            Ok(document) => document,
            Err(lekalo_core::mcp::ParseFailure::Duplicate) => {
                session
                    .writer
                    .send_error(&Value::Null, INVALID_REQUEST, "mcp.duplicate-key");
                continue;
            }
            Err(lekalo_core::mcp::ParseFailure::Malformed) => {
                session
                    .writer
                    .send_error(&Value::Null, PARSE_ERROR, "parse-error");
                continue;
            }
        };
        if super::stdio::check_depth(&document).is_err() {
            session
                .writer
                .send_error(&Value::Null, INVALID_REQUEST, "mcp.request-limit");
            continue;
        }
        let Some(incoming) = header_from_value(&document) else {
            session
                .writer
                .send_error(&Value::Null, PARSE_ERROR, "parse-error");
            continue;
        };
        if incoming.method.starts_with("notifications/") {
            handle_notification(&session, &incoming.method, incoming.params);
            continue;
        }
        // The lifecycle gate: before `notifications/initialized`, the
        // only served requests are `initialize` and `ping` (hosts probe
        // during the handshake); every other method is refused.
        if !*session.initialized.lock().expect("flag")
            && !matches!(incoming.method.as_str(), "initialize" | "ping")
        {
            session.writer.send_error(
                &incoming.id,
                INVALID_REQUEST,
                "mcp.lifecycle-not-initialized",
            );
            continue;
        }
        if let Some(response) = route(&session, &incoming) {
            // A cancelled request id ships no reply at all (the worker
            // already ran; the late envelope is suppressed here).
            if response.is_null() {
                continue;
            }
            // The single outbound chokepoint: a routed result is the
            // success member; the closed protocol-error marker becomes a
            // JSON-RPC error frame (measure-and-substitute happens in the
            // writer for both).
            if let Some(error) = response.get("lekaloProtocolError") {
                let code = error
                    .get("code")
                    .and_then(Value::as_i64)
                    .unwrap_or(INVALID_REQUEST);
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("protocol-error");
                session.writer.send_error(&incoming.id, code, message);
            } else {
                session.writer.send_result(&incoming.id, response);
            }
        }
    }
    // EOF shuts down outstanding work; dropped receivers suppress any
    // late worker success.
    log_event("eof-shutdown");
}

fn handle_notification(session: &Session, method: &str, params: Option<Value>) {
    match method {
        "notifications/initialized" => {
            *session.initialized.lock().expect("flag") = true;
        }
        "notifications/cancelled" => {
            // Cancelled notifications themselves have no reply. The late
            // result of a cancelled request is suppressed by registering
            // the id; the completing worker consults this set before the
            // reply is written. The 2025-11-25 profile names the member
            // `requestId`; the legacy `id` spelling is tolerated.
            let id = params
                .as_ref()
                .and_then(|params| params.get("requestId").or_else(|| params.get("id")));
            if let Some(id) = id {
                session
                    .cancelled
                    .lock()
                    .expect("flag")
                    .insert(id_to_key(id));
            }
        }
        _ => {
            // Unknown notifications are ignored per the JSON-RPC rules.
        }
    }
}

fn id_to_key(id: &Value) -> String {
    match id {
        Value::String(text) => format!("s:{text}"),
        Value::Number(number) => format!("n:{number}"),
        _ => "other".to_owned(),
    }
}

/// Route one request; the payload is the JSON-RPC `result` member.
fn route(session: &Arc<Session>, incoming: &Incoming) -> Option<Value> {
    match incoming.method.as_str() {
        "initialize" => Some(initialize()),
        "ping" => Some(serde_json::json!({})),
        "tools/list" => Some(tools_list()),
        "tools/call" => Some(tools_call(session, incoming)),
        "resources/list" => Some(resources_list(session, incoming)),
        "resources/templates/list" => Some(templates_list()),
        "resources/read" => Some(resources_read(session, incoming)),
        "resources/subscribe" => Some(resources_subscribe(session, incoming)),
        "resources/unsubscribe" => Some(resources_unsubscribe(session, incoming)),
        _ => Some(protocol_error_value(METHOD_NOT_FOUND, "method-not-found")),
    }
}

/// The initialize handshake: the supported profile is answered exactly;
/// only implemented capabilities are advertised. A host advertising a
/// different date gets the pinned profile back and may disconnect; no
/// universal compatibility is claimed.
fn initialize() -> Value {
    let profile = lekalo_core::mcp::version::PROTOCOL_PROFILE;
    let version = env!("CARGO_PKG_VERSION");
    serde_json::json!({
        "protocolVersion": profile,
        "capabilities": {
            "tools": {"listChanged": false},
            "resources": {"subscribe": false, "listChanged": false}
        },
        "serverInfo": {"name": "lekalo", "version": version}
    })
}

/// The exact seven read tools with their embedded closed schemas and
/// read-only annotations.
fn tools_list() -> Value {
    let tools: Vec<Value> = tools::CATALOG
        .iter()
        .map(|tool| {
            let slug = tool.slug;
            let find = |table: &'static [(&'static str, &'static [u8])]| {
                table
                    .iter()
                    .find(|(name, _)| *name == slug)
                    .and_then(|(_, bytes)| serde_json::from_slice::<Value>(bytes).ok())
                    .unwrap_or(Value::Null)
            };
            let input_schema = find(&lekalo_core::mcp::schemas::INPUT_SCHEMAS);
            let output_schema = find(&lekalo_core::mcp::schemas::OUTPUT_SCHEMAS);
            serde_json::json!({
                "name": tool.name,
                "title": tool.title,
                "description": format!("Read-only Lekalo {slug} projection over the admitted project scope; the output matches its pinned core contract."),
                "inputSchema": input_schema,
                "outputSchema": output_schema,
                "annotations": {
                    "readOnlyHint": true,
                    "destructiveHint": false,
                    "openWorldHint": false
                }
            })
        })
        .collect();
    serde_json::json!({ "tools": tools })
}

/// The tools/call admission and execution path: structural parameter
/// violations are -32602; a known tool's semantic failure is a typed
/// tool result; timeouts are typed bounded errors; a live cancellation
/// suppresses the late result.
fn tools_call(session: &Arc<Session>, incoming: &Incoming) -> Value {
    let Some(params) = incoming.params.as_ref() else {
        return protocol_error_value(INVALID_PARAMS, "invalid-params");
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return protocol_error_value(INVALID_PARAMS, "invalid-params");
    };
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    if tools::descriptor(name).is_none() {
        // Unknown tools are -32602; the name is a wire token and never an
        // echoed operand.
        return protocol_error_value(INVALID_PARAMS, "unknown-tool");
    }
    let request = match parse_request(name, &arguments) {
        Ok(mut request) => {
            // The trusted launcher group is the disclosure ceiling: a
            // client may name exactly that group, never any other; a
            // different explicit groupId is the typed widening refusal.
            match request.apply_launch_group(session.default_group.as_deref()) {
                Ok(()) => request,
                Err(boundary) => {
                    let set = lekalo_core::mcp::diagnostic::input_invalid_set(
                        boundary.field,
                        boundary.reason,
                    );
                    let envelope = execution::refusal_envelope(
                        name,
                        request_project(&arguments).as_deref(),
                        None,
                        set,
                        "invalid",
                        "input-invalid",
                        "mcp.input-invalid",
                    );
                    return tool_result(&incoming.id, &envelope, true);
                }
            }
        }
        Err(RequestError::Params(field)) => {
            return protocol_error_value(INVALID_PARAMS, field.field);
        }
        Err(RequestError::Semantic(boundary)) => {
            // The typed mcp.input-invalid refusal as a tool result.
            let set =
                lekalo_core::mcp::diagnostic::input_invalid_set(boundary.field, boundary.reason);
            let envelope = execution::refusal_envelope(
                name,
                request_project(&arguments).as_deref(),
                None,
                set,
                "invalid",
                "invalid-input",
                "mcp.input-invalid",
            );
            return tool_result(&incoming.id, &envelope, true);
        }
    };
    // Effective limits: caller parameters may reduce, never raise.
    let timeout_ms =
        Limits::effective_timeout_ms(request.common().timeout_ms, session.limits.timeout_ms);
    // Admission: bounded live workers. The sequential read loop is the
    // queue, so admitted concurrency never exceeds the recorded bound.
    if session.live.load(Ordering::SeqCst) >= session.limits.concurrent_calls.max(1) {
        let set = lekalo_core::mcp::diagnostic::request_limit_set("concurrency");
        let envelope = execution::refusal_envelope(
            name,
            Some(request.common().project_id.as_str()),
            request.common().group_id.as_deref(),
            set,
            "denied",
            "request-limit",
            "mcp.request-limit",
        );
        return tool_result(&incoming.id, &envelope, true);
    }
    session.live.fetch_add(1, Ordering::SeqCst);
    let worker_session = Arc::clone(session);
    let worker_request = request.clone();
    let (sender, receiver) = mpsc::channel();
    // Slot ownership (F14/F19): the dispatcher and the worker coordinate
    // the single release of the concurrency slot through one flag.
    // `SLOT_BUSY` -> `SLOT_RELEASED` happens exactly once: a completed
    // outcome releases on the dispatcher; a timed-out outcome leaves the
    // slot owned by the worker, which keeps executing and releases on
    // completion.
    let slot_released = Arc::new(AtomicUsize::new(SLOT_BUSY));
    let worker_released = Arc::clone(&slot_released);
    let spawned = std::thread::Builder::new()
        .name("lekalo-mcp-worker".to_owned())
        .spawn(move || {
            let output = execution::execute(&worker_session.catalog, &worker_request);
            let _ = sender.send(output);
            // Once the result is handed off, this worker is abandoned
            // only when the dispatcher timed out without taking it (and
            // so never released). Release the slot exactly once, if the
            // dispatcher did not already.
            if worker_released.swap(SLOT_RELEASED, Ordering::SeqCst) == SLOT_BUSY {
                worker_session.live.fetch_sub(1, Ordering::SeqCst);
                worker_session.detached.fetch_sub(1, Ordering::SeqCst);
            }
        });
    if spawned.is_err() {
        // A spawn failure frees the slot it reserved.
        session.live.fetch_sub(1, Ordering::SeqCst);
        let set = lekalo_core::mcp::diagnostic::backend_unavailable_set("worker");
        let envelope = execution::refusal_envelope(
            name,
            Some(request.common().project_id.as_str()),
            request.common().group_id.as_deref(),
            set,
            "unavailable",
            "backend-unavailable",
            "mcp.backend-unavailable",
        );
        return tool_result(&incoming.id, &envelope, true);
    }
    // The worker owns the sender from here; this thread holds only the
    // receiver, so a dropped receiver reliably suppresses late results.
    drop(spawned);
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let outcome = loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break None;
        }
        match receiver.recv_timeout(remaining) {
            Ok(output) => break Some(output),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break None,
        }
    };
    if outcome.is_none() {
        // Timeout: the worker keeps executing, so it keeps its slot and
        // releases on completion (its swap still observes SLOT_BUSY,
        // because this arm does not release).
        session.detached.fetch_add(1, Ordering::SeqCst);
    } else {
        // Completed (or the sender dropped): the dispatcher owns the
        // release; the worker's later swap observes SLOT_RELEASED and
        // never double-fires.
        if slot_released.swap(SLOT_RELEASED, Ordering::SeqCst) == SLOT_BUSY {
            session.live.fetch_sub(1, Ordering::SeqCst);
        }
    }
    // The late-success suppression: a cancelled request id never
    // receives the completed envelope, and a timed-out worker is
    // abandoned with the receiver dropped.
    if session
        .cancelled
        .lock()
        .expect("flag")
        .contains(&id_to_key(&incoming.id))
    {
        drop(receiver);
        return Value::Null;
    }
    match outcome {
        Some(Ok(output)) => tool_result(&incoming.id, &output.envelope, output.is_error),
        Some(Err(_fault)) => {
            // An execution seam fault is the typed result-invalid
            // developer refusal; the fault text is never echoed.
            log_event("execution-fault");
            let set = lekalo_core::mcp::diagnostic::result_invalid_set("execution");
            let envelope = execution::refusal_envelope(
                name,
                Some(request.common().project_id.as_str()),
                request.common().group_id.as_deref(),
                set,
                "invalid",
                "result-invalid",
                "mcp.result-invalid",
            );
            tool_result(&incoming.id, &envelope, true)
        }
        None => {
            // Timeout: a typed bounded tool error while the client is
            // still waiting; the abandoned worker's late success has no
            // receiver left to reach.
            drop(receiver);
            let set = lekalo_core::mcp::diagnostic::timeout_set(timeout_ms);
            let envelope = execution::refusal_envelope(
                name,
                Some(request.common().project_id.as_str()),
                request.common().group_id.as_deref(),
                set,
                "unavailable",
                "timeout",
                "mcp.timeout",
            );
            tool_result(&incoming.id, &envelope, true)
        }
    }
}

fn request_project(arguments: &Value) -> Option<String> {
    arguments
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The protocol-level error marker: `route` cannot answer a
/// tools/call refusal itself (the id belongs to the writer), so the
/// marked value is converted into a JSON-RPC error frame in the serve
/// loop — one serialization chokepoint, no string interpolation.
fn protocol_error_value(code: i64, message: &str) -> Value {
    serde_json::json!({ "lekaloProtocolError": { "code": code, "message": message } })
}

/// The MCP tool result: `structuredContent` plus a text content block
/// carrying the same serialized JSON for compatible clients. The envelope
/// is parsed once into a `serde_json::Value` (the single serialization
/// chokepoint in the writer measures the whole encoded frame).
fn tool_result(_id: &Value, envelope: &str, is_error: bool) -> Value {
    let structured: Value = serde_json::from_str(envelope).unwrap_or(Value::Null);
    let text = serde_json::to_string(&structured).unwrap_or_else(|_| "\"\"".to_owned());
    serde_json::json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured,
        "isError": is_error
    })
}

/// The admitted resource identities: project- and group-qualified family
/// URIs in deterministic order; symbol identities are template-only.
fn enumerate(session: &Session) -> (Vec<String>, String) {
    let mut uris = Vec::new();
    for project in session.catalog.core.projects() {
        for family in [
            Family::ProjectSummary,
            Family::ModuleList,
            Family::TargetStatus,
            Family::DiagnosticsSummary,
        ] {
            uris.push(resources::resource_uri(project, family, None));
        }
        for group in session.catalog.core.groups_of(project) {
            for family in [
                Family::ProjectSummary,
                Family::ModuleList,
                Family::TargetStatus,
                Family::DiagnosticsSummary,
            ] {
                uris.push(resources::group_resource_uri(
                    project,
                    group.id.as_str(),
                    family,
                    None,
                ));
            }
        }
    }
    uris.sort();
    let revision = crate::mcp::snapshot::domain_digest("resources", uris.join("\n").as_bytes());
    let mut inputs = super::snapshot::Inputs::new();
    inputs.insert("resources:enumeration".to_owned(), revision);
    (uris, super::snapshot::snapshot_ref(&inputs))
}

/// resources/list with the bounded page and the snapshot-bound cursor; a
/// stale cursor refuses instead of answering. Enumeration discloses only
/// identities inside the effective launch-group scope (F4).
fn resources_list(session: &Session, incoming: &Incoming) -> Value {
    let (uris, snapshot) = enumerate(session);
    let requested_cursor = incoming
        .params
        .as_ref()
        .and_then(|params| params.get("cursor"))
        .and_then(Value::as_str);
    let offset = match requested_cursor {
        Some(text) => match Cursor::decode(text) {
            Some(cursor) if cursor.snapshot == snapshot => cursor.offset,
            _ => {
                let set = lekalo_core::mcp::diagnostic::snapshot_stale_set("cursor");
                let envelope = execution::refusal_envelope(
                    "lekalo_inspect",
                    None,
                    None,
                    set,
                    "unavailable",
                    "snapshot-stale",
                    "mcp.snapshot-stale",
                );
                return tool_result(&incoming.id, &envelope, true);
            }
        },
        None => 0,
    };
    let page: Vec<&String> = uris
        .iter()
        .skip(offset)
        .take(session.limits.page_size)
        .collect();
    let items: Vec<Value> = page
        .iter()
        .map(|uri| {
            serde_json::json!({
                "uri": uri,
                "name": uri,
                "mimeType": "application/json"
            })
        })
        .collect();
    let next = offset + page.len();
    let mut response = serde_json::json!({ "resources": items });
    if next < uris.len() && !page.is_empty() {
        let cursor = Cursor {
            snapshot: snapshot.clone(),
            offset: next,
        }
        .encode();
        response["nextCursor"] = Value::from(cursor);
    }
    response
}

fn templates_list() -> Value {
    let templates: Vec<Value> = resources::templates()
        .iter()
        .map(|template| {
            serde_json::json!({
                "uriTemplate": template.uri_template,
                "name": template.name,
                "mimeType": template.mime_type
            })
        })
        .collect();
    serde_json::json!({ "resourceTemplates": templates })
}

/// resources/read: parse, authorize, and produce the closed resource
/// envelope as application/json text. Content never contains raw source,
/// dotfiles, or absolute private roots. The launch group is the enforced
/// ceiling (F4): an unqualified URI resolves under the launch group; a
/// group-qualified URI must name exactly that group; the effective
/// group's module closure is checked on the projected payload families.
fn resources_read(session: &Arc<Session>, incoming: &Incoming) -> Value {
    let uri = incoming
        .params
        .as_ref()
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (mut scope, family, qualifier) = match resources::parse_uri(uri) {
        Ok(parsed) => parsed,
        Err(_) => {
            return protocol_error_value(INVALID_PARAMS, "invalid-resource-uri");
        }
    };
    // The launcher's --group is the disclosure ceiling for the whole
    // resource surface too: an unqualified identity resolves under the
    // launch group, and a group-qualified identity must name exactly it.
    if let Some(launch_group) = session.default_group.as_deref() {
        match &scope.group {
            None => scope.group = Some(launch_group.to_owned()),
            Some(requested) if requested == launch_group => {}
            Some(_) => {
                let set = lekalo_core::mcp::diagnostic::scope_denied_set(
                    "group",
                    "widen-past-launch-group",
                );
                let envelope = execution::refusal_envelope(
                    "lekalo_inspect",
                    Some(&scope.project),
                    Some(launch_group),
                    set,
                    "denied",
                    "scope-denied",
                    "mcp.scope-denied",
                );
                return tool_result(&incoming.id, &envelope, true);
            }
        }
    }
    // Authorization: the project must be admitted, and the effective
    // group (if any) must belong to exactly that project.
    if !session.catalog.core.admits_project(&scope.project) {
        let set = lekalo_core::mcp::diagnostic::scope_denied_set("project", "unregistered");
        let envelope = execution::refusal_envelope(
            "lekalo_inspect",
            Some(&scope.project),
            scope.group.as_deref(),
            set,
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        );
        return tool_result(&incoming.id, &envelope, true);
    }
    if let Some(group) = &scope.group {
        if lekalo_core::mcp::policy::ScopeDecision::Admitted
            != session.catalog.core.admits_group(&scope.project, group)
        {
            let set = lekalo_core::mcp::diagnostic::scope_denied_set("group", "unregistered");
            let envelope = execution::refusal_envelope(
                "lekalo_inspect",
                Some(&scope.project),
                Some(group),
                set,
                "denied",
                "scope-denied",
                "mcp.scope-denied",
            );
            return tool_result(&incoming.id, &envelope, true);
        }
    }
    // Family closure: symbol/targets/diagnostics read module-scoped
    // payload; under an effective group the required module must stay
    // inside the allow-list (the same closure rule tools/call applies).
    if let Some(group) = &scope.group {
        let required = match (&family, qualifier.as_deref()) {
            (Family::SymbolMetadata, Some(symbol)) => {
                let mut set = std::collections::BTreeSet::new();
                if let Some(head) = symbol.split('.').next() {
                    if lekalo_core::mcp::policy::admit_module_id(head).is_ok() {
                        set.insert(head.to_owned());
                    }
                }
                set
            }
            (Family::ProjectSummary | Family::ModuleList, _) => {
                // Whole-project projections refuse under a group instead
                // of fabricating a narrowed result (same rule as tools).
                let set =
                    lekalo_core::mcp::diagnostic::scope_denied_set("module", "requires-full-scope");
                let envelope = execution::refusal_envelope(
                    "lekalo_inspect",
                    Some(&scope.project),
                    Some(group),
                    set,
                    "denied",
                    "scope-denied",
                    "mcp.scope-denied",
                );
                return tool_result(&incoming.id, &envelope, true);
            }
            _ => std::collections::BTreeSet::new(),
        };
        if lekalo_core::mcp::policy::ScopeDecision::Admitted
            != lekalo_core::mcp::policy::modules_admitted(
                &session.catalog.core,
                &scope.project,
                Some(group),
                &required,
            )
        {
            let set = lekalo_core::mcp::diagnostic::scope_denied_set("module", "outside-group");
            let envelope = execution::refusal_envelope(
                "lekalo_inspect",
                Some(&scope.project),
                Some(group),
                set,
                "denied",
                "scope-denied",
                "mcp.scope-denied",
            );
            return tool_result(&incoming.id, &envelope, true);
        }
    }
    match build_resource(session, &scope, family, qualifier.as_deref()) {
        Ok(document) => serde_json::json!({
            "contents": [{
                "uri": uri,
                "mimeType": "application/json",
                "text": document
            }]
        }),
        Err(refusal_envelope) => tool_result(&incoming.id, &refusal_envelope, true),
    }
}

/// The resource revision: the domain digest re-spelled into the
/// contract's closed sha256-`<hex>` form.
fn resource_revision(domain: &str, bytes: &[u8]) -> String {
    let digest = crate::mcp::snapshot::domain_digest(domain, bytes);
    format!(
        "sha256-{}",
        digest.split_once(':').expect("domain prefix").1
    )
}

/// Build the closed `lekalo/mcp-resource/v0.8.0` payload for one
/// authorized read. The typed projections reuse the same command seams as
/// the tools so both surfaces stay semantically identical; the underlying
/// failures project as the family's typed refusal. The document is a
/// closed `serde_json::Value`; `kind` carries the contract enum spelling
/// (F7), never the URI family key.
fn build_resource(
    session: &Arc<Session>,
    scope: &UriScope,
    family: Family,
    qualifier: Option<&str>,
) -> Result<String, String> {
    let entry = session
        .catalog
        .projects
        .get(&scope.project)
        .ok_or_else(|| refused(scope, "project", "unregistered"))?;
    let project = Some(entry.selector.clone());
    let (revision, data) = match family {
        Family::ProjectSummary => {
            let selection = lekalo_core::loader::LoadSelection {
                project: project.clone(),
            };
            let model = match lekalo_core::loader::normalize_model(&selection) {
                Ok(model) => model,
                Err(_) => return Err(denied_summary(scope)),
            };
            let compilation = match lekalo_core::ir::compile(&model) {
                Ok(compilation) => compilation,
                Err(_) => return Err(denied_summary(scope)),
            };
            let revision = resource_revision(
                "model",
                lekalo_core::loader::canonical_model_bytes(&model).as_bytes(),
            );
            (
                revision,
                format!(
                    "{{\"project\":\"{}\",\"modelVersion\":\"{}\",\"moduleCount\":{},\"definitionCount\":{},\"unknownStates\":[]}}",
                    scope.project,
                    model.model_version.as_str(),
                    compilation.project.modules.len(),
                    compilation.project.definitions.len(),
                ),
            )
        }
        Family::ModuleList => {
            let selection = lekalo_core::loader::LoadSelection {
                project: project.clone(),
            };
            let model = match lekalo_core::loader::normalize_model(&selection) {
                Ok(model) => model,
                Err(_) => return Err(denied_summary(scope)),
            };
            let compilation = match lekalo_core::ir::compile(&model) {
                Ok(compilation) => compilation,
                Err(_) => return Err(denied_summary(scope)),
            };
            let revision = resource_revision(
                "model",
                lekalo_core::loader::canonical_model_bytes(&model).as_bytes(),
            );
            // Definitions are counted per module from the sorted semantic
            // ids; the page is deterministic and bounded by the modules
            // the compilation itself contains.
            let mut counts: Vec<(String, usize)> = compilation
                .project
                .modules
                .iter()
                .map(|module| (module.id.as_str().to_owned(), 0))
                .collect();
            for definition in &compilation.project.definitions {
                if let Some(head) = definition.id().as_str().split('.').next() {
                    if let Some(count) = counts.iter_mut().find(|(id, _)| id == head) {
                        count.1 += 1;
                    }
                }
            }
            let modules: Vec<String> = counts
                .iter()
                .map(|(id, count)| format!("{{\"id\":\"{id}\",\"definitionCount\":{count}}}"))
                .collect();
            (revision, format!("{{\"modules\":[{}]}}", modules.join(",")))
        }
        Family::SymbolMetadata => {
            let Some(symbol) = qualifier else {
                return Err(refused(scope, "symbol", "required"));
            };
            let raw = crate::run_inspect(symbol, None, &project).to_json_string();
            let view = match execution::EnvelopeView::parse(&raw) {
                Ok(view) if view.is_valid() => view,
                _ => return Err(refused(scope, "symbol", "unknown")),
            };
            // The raw inspect payload bytes, spliced verbatim.
            let payload = view
                .payload_for(&execution::family_of("lekalo_inspect"))
                .ok_or_else(|| refused(scope, "symbol", "invalid"))?;
            let revision = resource_revision("symbol", payload.get().as_bytes());
            (
                revision,
                format!(
                    "{{\"schemaVersion\":\"lekalo/inspect/v0.2.16\",\"identity\":\"dev.lekalo.inspect@0.2.16\",\"inspect\":{}}}",
                    payload.get()
                ),
            )
        }
        Family::TargetStatus => match crate::run_status(project) {
            result @ lekalo_core::DomainResult::Valid { .. } => {
                let raw = result.to_json_string();
                // The doctor document is the whole envelope.
                let revision = resource_revision("doctor", raw.as_bytes());
                (
                    revision,
                    format!(
                        "{{\"schemaVersion\":\"lekalo/doctor/v0.3.2\",\"identity\":\"dev.lekalo.doctor@0.3.2\",\"doctor\":{raw}}}"
                    ),
                )
            }
            _ => return Err(refused(scope, "targets", "unavailable")),
        },
        Family::DiagnosticsSummary => {
            match crate::run_validate(project.clone(), None, false, true) {
                result @ lekalo_core::DomainResult::Valid { .. } => {
                    let raw = result.to_json_string();
                    let view: serde_json::Value = serde_json::from_str(&raw).unwrap_or_default();
                    let count = view
                        .get("diagnostics")
                        .and_then(|diagnostics| diagnostics.as_array())
                        .map(|items| items.len())
                        .unwrap_or(0);
                    let revision = resource_revision("validation", raw.as_bytes());
                    (
                        revision,
                        format!(
                            "{{\"diagnosticCount\":{count},\"evidenceCount\":0,\"complete\":true,\"reasons\":[]}}"
                        ),
                    )
                }
                _ => return Err(refused(scope, "diagnostics", "unavailable")),
            }
        }
    };
    // The output screening pass runs over the resource data too (F17):
    // a secret-shaped payload refuses with `mcp.sensitive-output`.
    if lekalo_core::mcp::screen::screen_envelope(&data).is_err() {
        return Err(refused_sensitive(scope));
    }
    let data: serde_json::Value = serde_json::from_str(&data).map_err(|_| {
        let set = lekalo_core::mcp::diagnostic::result_invalid_set("resource-payload");
        execution::refusal_envelope(
            "lekalo_inspect",
            Some(&scope.project),
            scope.group.as_deref(),
            set,
            "invalid",
            "result-invalid",
            "mcp.result-invalid",
        )
    })?;
    let document = serde_json::json!({
        "schemaVersion": lekalo_core::mcp::version::RESOURCE_SCHEMA_VERSION,
        "identity": lekalo_core::mcp::version::RESOURCE_IDENTITY,
        "kind": family.contract_key(),
        "uri": resources::build_uri(scope, family, qualifier),
        "scope": {
            "projectId": scope.project,
            "groupId": scope.group
        },
        "revision": revision,
        "data": data
    });
    serde_json::to_string(&document).map_err(|_| "resource-serialize".to_owned())
}

/// The scope-refusal envelope for the resource dispatcher.
fn refused(scope: &UriScope, subject: &str, reason: &str) -> String {
    let set = lekalo_core::mcp::diagnostic::scope_denied_set(subject, reason);
    execution::refusal_envelope(
        "lekalo_inspect",
        Some(&scope.project),
        scope.group.as_deref(),
        set,
        "denied",
        "scope-denied",
        "mcp.scope-denied",
    )
}

/// The sensitive-output refusal for a resource payload: the registered
/// `mcp.sensitive-output` denial, never a redacted partial payload.
fn refused_sensitive(scope: &UriScope) -> String {
    let set =
        lekalo_core::mcp::diagnostic::sensitive_output_set("resource", "secret-shaped-output");
    execution::refusal_envelope(
        "lekalo_inspect",
        Some(&scope.project),
        scope.group.as_deref(),
        set,
        "denied",
        "sensitive-output",
        "mcp.sensitive-output",
    )
}

/// The summary refusal path before any model load.
fn denied_summary(scope: &UriScope) -> String {
    let set = lekalo_core::mcp::diagnostic::backend_unavailable_set("model");
    execution::refusal_envelope(
        "lekalo_inspect",
        Some(&scope.project),
        scope.group.as_deref(),
        set,
        "unavailable",
        "backend-unavailable",
        "mcp.backend-unavailable",
    )
}

fn resources_subscribe(session: &Arc<Session>, incoming: &Incoming) -> Value {
    let uri = incoming
        .params
        .as_ref()
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (scope, _family, _qualifier) = match resources::parse_uri(uri) {
        Ok(parsed) => parsed,
        Err(_) => {
            return protocol_error_value(INVALID_PARAMS, "invalid-resource-uri");
        }
    };
    // Subscribe is an authorization decision, not a bookkeeping one
    // (F13): the exact read-scope check of resources/read runs first —
    // project admission, launch-group ceiling, and group membership —
    // and only an identity this session may read can be watched.
    if !session.catalog.core.admits_project(&scope.project) {
        let set = lekalo_core::mcp::diagnostic::scope_denied_set("project", "unregistered");
        let envelope = execution::refusal_envelope(
            "lekalo_inspect",
            Some(&scope.project),
            scope.group.as_deref(),
            set,
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        );
        return tool_result(&incoming.id, &envelope, true);
    }
    if let Some(group) = &scope.group {
        if lekalo_core::mcp::policy::ScopeDecision::Admitted
            != session.catalog.core.admits_group(&scope.project, group)
        {
            let set = lekalo_core::mcp::diagnostic::scope_denied_set("group", "unregistered");
            let envelope = execution::refusal_envelope(
                "lekalo_inspect",
                Some(&scope.project),
                Some(group),
                set,
                "denied",
                "scope-denied",
                "mcp.scope-denied",
            );
            return tool_result(&incoming.id, &envelope, true);
        }
    }
    if let Some(launch_group) = session.default_group.as_deref() {
        let inside = match &scope.group {
            Some(requested) => requested == launch_group,
            None => true,
        };
        if !inside {
            let set =
                lekalo_core::mcp::diagnostic::scope_denied_set("group", "widen-past-launch-group");
            let envelope = execution::refusal_envelope(
                "lekalo_inspect",
                Some(&scope.project),
                Some(launch_group),
                set,
                "denied",
                "scope-denied",
                "mcp.scope-denied",
            );
            return tool_result(&incoming.id, &envelope, true);
        }
    }
    let mut subscriptions = session.subscriptions.lock().expect("flag");
    if subscriptions.len() >= lekalo_core::mcp::version::MAX_WATCHED_IDENTITIES {
        let set = lekalo_core::mcp::diagnostic::request_limit_set("watched");
        let envelope = execution::refusal_envelope(
            "lekalo_inspect",
            None,
            None,
            set,
            "denied",
            "request-limit",
            "mcp.request-limit",
        );
        return tool_result(&incoming.id, &envelope, true);
    }
    subscriptions.insert(uri.to_owned());
    serde_json::json!({})
}

fn resources_unsubscribe(session: &Arc<Session>, incoming: &Incoming) -> Value {
    let uri = incoming
        .params
        .as_ref()
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    session.subscriptions.lock().expect("flag").remove(uri);
    serde_json::json!({})
}
