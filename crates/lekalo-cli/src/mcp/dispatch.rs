//! The JSON-RPC dispatch for the MCP server (issue #33): the pinned
//! 2025-11-25 lifecycle over stdio.
//!
//! After `initialize`/`notifications/initialized`, the served methods are
//! ping, tools/list, tools/call, resources/list, resources/templates/list,
//! resources/read, resources/subscribe, and resources/unsubscribe. Only
//! implemented capabilities are advertised: tools plus resource
//! subscribe/listChanged; no prompts, sampling, elicitation, tasks, HTTP
//! transport, or client-driven root expansion. A timeout produces a typed
//! bounded tool error while the client is still waiting; a cancellation
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
use super::stdio::{error_response, log_event, result_response, Writer};

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
    pub writer: Writer,
    initialized: Mutex<bool>,
    cancelled: Mutex<BTreeSet<String>>,
    subscriptions: Mutex<BTreeSet<String>>,
    live: AtomicUsize,
}

impl Session {
    /// One admitted session over the resolved catalog.
    pub fn new(catalog: ServerCatalog, limits: Limits, writer: Writer) -> Self {
        Self {
            catalog,
            limits,
            writer,
            initialized: Mutex::new(false),
            cancelled: Mutex::new(BTreeSet::new()),
            subscriptions: Mutex::new(BTreeSet::new()),
            live: AtomicUsize::new(0),
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
                session.writer.send(&error_response(
                    &Value::Null,
                    INVALID_REQUEST,
                    "mcp.request-limit",
                ));
                continue;
            }
            Err(_) => {
                session
                    .writer
                    .send(&error_response(&Value::Null, PARSE_ERROR, "parse-error"));
                continue;
            }
        };
        // One structural parse: nesting depth first, then the header.
        let document: Result<Value, _> = serde_json::from_str(&frame);
        let document = match document {
            Ok(document) => document,
            Err(_) => {
                session
                    .writer
                    .send(&error_response(&Value::Null, PARSE_ERROR, "parse-error"));
                continue;
            }
        };
        if super::stdio::check_depth(&document).is_err() {
            session.writer.send(&error_response(
                &Value::Null,
                INVALID_REQUEST,
                "mcp.request-limit",
            ));
            continue;
        }
        let Some(incoming) = header_from_value(&document) else {
            session
                .writer
                .send(&error_response(&Value::Null, PARSE_ERROR, "parse-error"));
            continue;
        };
        if incoming.method.starts_with("notifications/") {
            handle_notification(&session, &incoming.method, incoming.params);
            continue;
        }
        if let Some(response) = route(&session, &incoming) {
            session.writer.send(&response);
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
            // reply is written.
            if let Some(id) = params.as_ref().and_then(|params| params.get("id")) {
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

/// Route one request; `None` means no reply.
fn route(session: &Arc<Session>, incoming: &Incoming) -> Option<String> {
    match incoming.method.as_str() {
        "initialize" => Some(initialize(incoming)),
        "ping" => Some(result_response(&incoming.id, "{}")),
        "tools/list" => Some(tools_list(incoming)),
        "tools/call" => Some(tools_call(session, incoming)),
        "resources/list" => Some(resources_list(session, incoming)),
        "resources/templates/list" => Some(templates_list(incoming)),
        "resources/read" => Some(resources_read(session, incoming)),
        "resources/subscribe" => Some(resources_subscribe(session, incoming)),
        "resources/unsubscribe" => Some(resources_unsubscribe(session, incoming)),
        _ => Some(error_response(
            &incoming.id,
            METHOD_NOT_FOUND,
            "method-not-found",
        )),
    }
}

/// The initialize handshake: the supported profile is answered exactly;
/// only implemented capabilities are advertised. A host advertising a
/// different date gets the pinned profile back and may disconnect; no
/// universal compatibility is claimed.
fn initialize(incoming: &Incoming) -> String {
    let profile = lekalo_core::mcp::version::PROTOCOL_PROFILE;
    let version = env!("CARGO_PKG_VERSION");
    result_response(
        &incoming.id,
        &format!(
            "{{\"protocolVersion\":\"{profile}\",\"capabilities\":{{\"tools\":{{\"listChanged\":false}},\"resources\":{{\"subscribe\":true,\"listChanged\":true}}}},\"serverInfo\":{{\"name\":\"lekalo\",\"version\":\"{version}\"}}}}"
        ),
    )
}

/// The exact seven read tools with their embedded closed schemas and
/// read-only annotations.
fn tools_list(incoming: &Incoming) -> String {
    let descriptors: Vec<String> = tools::CATALOG
        .iter()
        .map(|tool| {
            let slug = tool.slug;
            let find = |table: &'static [(&'static str, &'static [u8])]| {
                table
                    .iter()
                    .find(|(name, _)| *name == slug)
                    .map(|(_, bytes)| super::stdio::compact_json(&String::from_utf8_lossy(bytes)))
                    .unwrap_or_default()
            };
            let input_schema = find(&lekalo_core::mcp::schemas::INPUT_SCHEMAS);
            let output_schema = find(&lekalo_core::mcp::schemas::OUTPUT_SCHEMAS);
            let name = tool.name;
            let title = tool.title;
            format!(
                "{{\"name\":\"{name}\",\"title\":\"{title}\",\"description\":\"Read-only Lekalo {slug} projection over the admitted project scope; the output matches its pinned core contract.\",\"inputSchema\":{input_schema},\"outputSchema\":{output_schema},\"annotations\":{{\"readOnlyHint\":true,\"destructiveHint\":false,\"openWorldHint\":false}}}}"
            )
        })
        .collect();
    result_response(
        &incoming.id,
        &format!("{{\"tools\":[{}]}}", descriptors.join(",")),
    )
}

/// The tools/call admission and execution path: structural parameter
/// violations are -32602; a known tool's semantic failure is a typed
/// tool result; timeouts are typed bounded errors; a live cancellation
/// suppresses the late result.
fn tools_call(session: &Arc<Session>, incoming: &Incoming) -> String {
    let Some(params) = incoming.params.as_ref() else {
        return error_response(&incoming.id, INVALID_PARAMS, "invalid-params");
    };
    let Some(name) = params.get("name").and_then(Value::as_str) else {
        return error_response(&incoming.id, INVALID_PARAMS, "invalid-params");
    };
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    if tools::descriptor(name).is_none() {
        // Unknown tools are -32602; the name is a wire token and never an
        // echoed operand.
        return error_response(&incoming.id, INVALID_PARAMS, "unknown-tool");
    }
    let request = match parse_request(name, &arguments) {
        Ok(request) => request,
        Err(RequestError::Params(field)) => {
            return error_response(&incoming.id, INVALID_PARAMS, field.field);
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
    let spawned = std::thread::Builder::new()
        .name("lekalo-mcp-worker".to_owned())
        .spawn(move || {
            let output = execution::execute(&worker_session.catalog, &worker_request);
            let _ = sender.send(output);
        });
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
    session.live.fetch_sub(1, Ordering::SeqCst);
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
        return String::new();
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

/// The MCP tool result: `structuredContent` plus a text content block
/// carrying the same serialized JSON for compatible clients. The output
/// bound counts the entire encoded frame, including the duplication.
fn tool_result(id: &Value, envelope: &str, is_error: bool) -> String {
    let text = serde_json::to_string(envelope).unwrap_or_else(|_| "\"\"".to_owned());
    result_response(
        id,
        &format!(
            "{{\"content\":[{{\"type\":\"text\",\"text\":{text}}}],\"structuredContent\":{envelope},\"isError\":{is_error}}}"
        ),
    )
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
/// stale cursor refuses instead of answering.
fn resources_list(session: &Session, incoming: &Incoming) -> String {
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
    let items: Vec<String> = page
        .iter()
        .map(|uri| {
            format!("{{\"uri\":\"{uri}\",\"name\":\"{uri}\",\"mimeType\":\"application/json\"}}")
        })
        .collect();
    let next = offset + page.len();
    let next_cursor = if next < uris.len() && !page.is_empty() {
        format!(
            ",\"nextCursor\":\"{}\"",
            Cursor {
                snapshot: snapshot.clone(),
                offset: next
            }
            .encode()
        )
    } else {
        String::new()
    };
    result_response(
        &incoming.id,
        &format!("{{\"resources\":[{}]{next_cursor}}}", items.join(",")),
    )
}

fn templates_list(incoming: &Incoming) -> String {
    let templates: Vec<String> = resources::templates()
        .iter()
        .map(|template| {
            format!(
                "{{\"uriTemplate\":\"{}\",\"name\":\"{}\",\"mimeType\":\"application/json\"}}",
                template.uri_template, template.name
            )
        })
        .collect();
    result_response(
        &incoming.id,
        &format!("{{\"resourceTemplates\":[{}]}}", templates.join(",")),
    )
}

/// resources/read: parse, authorize, and produce the closed resource
/// envelope as application/json text. Content never contains raw source,
/// dotfiles, or absolute private roots.
fn resources_read(session: &Arc<Session>, incoming: &Incoming) -> String {
    let uri = incoming
        .params
        .as_ref()
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let (scope, family, qualifier) = match resources::parse_uri(uri) {
        Ok(parsed) => parsed,
        Err(_) => {
            return error_response(&incoming.id, INVALID_PARAMS, "invalid-resource-uri");
        }
    };
    // Authorization: the project must be admitted, and a group-qualified
    // URI must name a group of exactly that project.
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
    match build_resource(session, &scope, family, qualifier.as_deref()) {
        Ok(data) => {
            let text = serde_json::to_string(data.as_str()).unwrap_or_else(|_| "\"\"".to_owned());
            result_response(
                &incoming.id,
                &format!(
                    "{{\"contents\":[{{\"uri\":\"{uri}\",\"mimeType\":\"application/json\",\"text\":{text}}}]}}"
                ),
            )
        }
        Err(refusal_envelope) => tool_result(&incoming.id, &refusal_envelope, true),
    }
}

/// Build the closed `lekalo/mcp-resource/v0.8.0` payload for one
/// authorized read. The typed projections reuse the same command seams as
/// the tools so both surfaces stay semantically identical; the underlying
/// failures project as the family's typed refusal.
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
            let revision = crate::mcp::snapshot::domain_digest(
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
            let revision = crate::mcp::snapshot::domain_digest(
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
            let revision = crate::mcp::snapshot::domain_digest("symbol", payload.get().as_bytes());
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
                let revision = crate::mcp::snapshot::domain_digest("doctor", raw.as_bytes());
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
                    let revision =
                        crate::mcp::snapshot::domain_digest("validation", raw.as_bytes());
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
    Ok(format!(
        "{{\"schemaVersion\":\"{}\",\"identity\":\"{}\",\"kind\":\"{}\",\"uri\":\"{}\",\"scope\":{{\"projectId\":\"{}\",\"groupId\":{}}},\"revision\":\"{}\",\"data\":{}}}",
        lekalo_core::mcp::version::RESOURCE_SCHEMA_VERSION,
        lekalo_core::mcp::version::RESOURCE_IDENTITY,
        family.key(),
        resources::build_uri(scope, family, qualifier),
        scope.project,
        match &scope.group {
            Some(group) => format!("\"{group}\""),
            None => "null".to_owned(),
        },
        revision,
        data
    ))
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

fn resources_subscribe(session: &Arc<Session>, incoming: &Incoming) -> String {
    let uri = incoming
        .params
        .as_ref()
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if resources::parse_uri(uri).is_err() {
        return error_response(&incoming.id, INVALID_PARAMS, "invalid-resource-uri");
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
    result_response(&incoming.id, "{}")
}

fn resources_unsubscribe(session: &Arc<Session>, incoming: &Incoming) -> String {
    let uri = incoming
        .params
        .as_ref()
        .and_then(|params| params.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    session.subscriptions.lock().expect("flag").remove(uri);
    result_response(&incoming.id, "{}")
}
