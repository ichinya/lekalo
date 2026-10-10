//! Tool execution for the MCP server (issue #33): every call reuses the
//! exact typed core/CLI seams the ordinary commands use, over the granted
//! project root, on the uncached path.
//!
//! The seven read tools never spawn an adapter, never write cache,
//! runtime, report, or history files, and never widen the loader's own
//! policy: inspect/impact/context/graph/validate dispatch into the same
//! command runners the CLI uses (which keeps the payloads byte-identical
//! with the core contracts), and `lekalo_trace_export` replaces the CLI's
//! unrestricted path reader with the bounded authorized reader before
//! `TraceManifest::parse`. Success envelopes preserve the core receipts
//! verbatim; every refusal is a registered `mcp.*` diagnostic or the
//! preserved domain diagnostics, never a fabricated payload.
//!
//! Round-3 review repairs carried here: every envelope is assembled as
//! one `serde_json` document (r2 F4 — a multi-diagnostic refusal can no
//! longer splice raw text into a quoted member), the diagnostics bound is
//! enforced (r2 F20), the failure-class vocabulary is the contract enum
//! (r2 F12), the launch-group closure is resolved from the compiled graph
//! and re-checked against the disclosed payload (r2 F1), the snapshot
//! pins the admitted model and lock inputs and fails closed on any
//! re-check failure (r2 F7), and the target-status selectors project the
//! closed doctor panels instead of being discarded (r2 F16).

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{json, Map, Value};

use lekalo_core::loader::LoadSelection;
use lekalo_core::mcp::policy::{self, ScopeDecision};
use lekalo_core::mcp::screen;
use lekalo_core::mcp::tools::{Common, ContextMode, GraphMode, ImpactMode, ToolRequest};
use lekalo_core::mcp::version::{MAX_TOOL_DIAGNOSTICS, VERSION};
use lekalo_core::DomainResult;

use super::catalog::ServerCatalog;
use super::read;
use super::snapshot::{domain_digest, snapshot_ref, Inputs};

/// A completed tool execution: the closed output-contract envelope and
/// whether the transport must mark it as an error result.
pub struct ToolOutput {
    pub envelope: String,
    pub is_error: bool,
}

/// The admission scope of one resolved request.
pub(crate) struct Scope<'a> {
    pub(crate) project: &'a str,
    pub(crate) group: Option<&'a str>,
}

/// The per-tool slug, discriminator, identity, and payload key.
pub(crate) struct Family {
    pub(crate) slug: &'static str,
    discriminator: &'static str,
    identity: &'static str,
    payload_key: &'static str,
    /// The `contractPins` member beyond the common three, as its typed
    /// `(name, version)` pair (r2 F16: the pin rides the serde-built
    /// envelope, never a raw JSON fragment).
    pin: (&'static str, &'static str),
}

pub(crate) fn family_of(tool: &str) -> Family {
    let slug = match tool {
        "lekalo_inspect" => "inspect",
        "lekalo_impact" => "impact",
        "lekalo_context" => "context",
        "lekalo_graph" => "graph",
        "lekalo_validate" => "validate",
        "lekalo_target_status" => "target-status",
        _ => "trace-export",
    };
    let (discriminator, identity, payload_key, pin) = match slug {
        "inspect" => (
            "lekalo/inspect/v0.2.16",
            "dev.lekalo.inspect@0.2.16",
            "inspect",
            ("inspect", "0.2.16"),
        ),
        "impact" => (
            "lekalo/impact/v0.2.16",
            "dev.lekalo.impact@0.2.16",
            "impact",
            ("impact", "0.2.16"),
        ),
        "context" => (
            "lekalo/context/v0.2.16",
            "dev.lekalo.context@0.2.16",
            "context",
            ("context", "0.2.16"),
        ),
        "graph" => (
            "lekalo/graph/v0.2.16",
            "dev.lekalo.graph@0.2.16",
            "query",
            ("graph", "0.2.16"),
        ),
        "validate" => (
            "lekalo/validation-report/v0.6.4",
            "dev.lekalo.validation-report@0.6.4",
            "validation",
            ("validation", "0.6.4"),
        ),
        "target-status" => (
            "lekalo/doctor/v0.3.2",
            "dev.lekalo.doctor@0.3.2",
            "doctor",
            ("doctor", "0.3.2"),
        ),
        _ => (
            "lekalo/trace-manifest/v0.2.16",
            "dev.lekalo.trace-manifest@0.2.16",
            "trace",
            ("traceManifest", "0.2.16"),
        ),
    };
    Family {
        slug,
        discriminator,
        identity,
        payload_key,
        pin,
    }
}

/// The parsed view of one reused CLI success/failure envelope. Payload
/// members stay raw so the preserved core receipt bytes are spliced
/// verbatim into the MCP output (byte parity with core/CLI); the
/// diagnostics and reason codes are parsed once and re-serialized as
/// typed members (r2 F4: never spliced text inside a quoted member).
#[derive(Deserialize)]
pub(crate) struct EnvelopeView<'a> {
    status: &'a str,
    #[serde(default, borrow)]
    inspect: Option<&'a RawValue>,
    #[serde(default, borrow)]
    impact: Option<&'a RawValue>,
    #[serde(default, borrow)]
    context: Option<&'a RawValue>,
    #[serde(default, borrow)]
    graph: Option<&'a RawValue>,
    #[serde(default, rename = "modelVersion")]
    model_version: Option<Value>,
    #[serde(default, borrow)]
    validation: Option<&'a RawValue>,
    #[serde(default, borrow)]
    diagnostics: Option<&'a RawValue>,
    #[serde(default, borrow, rename = "reasonCodes")]
    reason_codes: Option<&'a RawValue>,
}

impl<'a> EnvelopeView<'a> {
    /// Parse one reused envelope; a parse failure is the typed
    /// `mcp.result-invalid` developer fault, never a partial frame.
    pub(crate) fn parse(raw: &'a str) -> Result<Self, String> {
        serde_json::from_str(raw).map_err(|error| error.to_string())
    }

    /// The raw payload member for one tool family. The doctor document
    /// is the whole Valid envelope, so the target-status family reads it
    /// through 'member_full' instead.
    pub(crate) fn payload_for(&self, family: &Family) -> Option<&RawValue> {
        match family.slug {
            "inspect" => self.inspect,
            "impact" => self.impact,
            "context" => self.context,
            "graph" => self.graph,
            "validate" => self.validation,
            _ => None,
        }
    }

    /// Whether the envelope is the accepted success wrapper.
    pub(crate) fn is_valid(&self) -> bool {
        self.status == "valid"
    }

    /// The parsed diagnostics array. A malformed diagnostics member is a
    /// developer fault (r2 F4): the caller converts it into the typed
    /// `mcp.result-invalid` refusal instead of shipping `null`.
    pub(crate) fn diagnostics(&self) -> Result<Option<Vec<Value>>, String> {
        match self.diagnostics {
            None => Ok(None),
            Some(raw) => serde_json::from_str(raw.get())
                .map(Some)
                .map_err(|error| error.to_string()),
        }
    }

    /// The parsed reason-code array.
    pub(crate) fn reason_codes(&self) -> Result<Option<Vec<String>>, String> {
        match self.reason_codes {
            None => Ok(None),
            Some(raw) => serde_json::from_str(raw.get())
                .map(Some)
                .map_err(|error| error.to_string()),
        }
    }
}

/// The diagnostics bound is contract-declared (`maxItems: 64`, r2 F20):
/// a domain result carrying more would ship a schema-invalid envelope,
/// so the whole output refuses as the typed `mcp.result-invalid` fault.
fn bounded_diagnostics(diagnostics: Option<Vec<Value>>) -> Result<Value, String> {
    let items = diagnostics.unwrap_or_default();
    if items.len() > MAX_TOOL_DIAGNOSTICS {
        return Err(format!(
            "diagnostics-bound: {} items exceed the contract cap of {MAX_TOOL_DIAGNOSTICS}",
            items.len()
        ));
    }
    Ok(Value::Array(items))
}

/// Assemble the closed output envelope as one `serde_json` document
/// (r2 F4: the single serialization form; no member is ever spliced raw
/// text into a quoted slot). `tail` carries the `result` branch of a
/// completed core call or the typed `failure` branch.
fn envelope(
    family: &Family,
    tool: &str,
    scope: &Scope,
    snapshot: &str,
    status: &str,
    diagnostics: Value,
    tail: Option<Value>,
) -> String {
    let mut document = Map::new();
    document.insert(
        "schemaVersion".to_owned(),
        json!(format!(
            "lekalo/mcp-tool-{}-output/v{}",
            family.slug, VERSION
        )),
    );
    document.insert(
        "identity".to_owned(),
        json!(format!(
            "dev.lekalo.mcp-tool-{}-output@{}",
            family.slug, VERSION
        )),
    );
    document.insert("tool".to_owned(), json!(tool));
    document.insert(
        "scope".to_owned(),
        json!({"projectId": scope.project, "groupId": scope.group}),
    );
    document.insert("snapshotRef".to_owned(), json!(snapshot));
    document.insert("status".to_owned(), json!(status));
    document.insert(
        "contractPins".to_owned(),
        json!({
            "diagnostic": "0.2.16",
            "diagnosticRegistry": "0.8.0",
            "protocolProfile": "2025-11-25",
            family.pin.0: family.pin.1,
        }),
    );
    document.insert("diagnostics".to_owned(), diagnostics);
    if let Some(Value::Object(members)) = tail {
        for (name, value) in members {
            document.insert(name, value);
        }
    }
    serde_json::to_string(&Value::Object(document)).unwrap_or_else(|_| {
        // Unreachable over owned JSON values; the typed fault keeps the
        // frame closed even here.
        json!({
            "schemaVersion": format!("lekalo/mcp-tool-{}-output/v{VERSION}", family.slug),
            "identity": format!("dev.lekalo.mcp-tool-{}-output@{VERSION}", family.slug),
            "tool": tool,
            "scope": {"projectId": scope.project, "groupId": scope.group},
            "snapshotRef": snapshot,
            "status": "invalid",
            "contractPins": {"diagnostic": "0.2.16", "diagnosticRegistry": "0.8.0", "protocolProfile": "2025-11-25"},
            "diagnostics": [],
            "failure": {"failureClass": "result-invalid", "detail": "mcp.result-invalid"},
        })
        .to_string()
    })
}

/// The typed MCP refusal branch (bounded, closed, no input echo), built
/// by the serializer so every detail string is escaped (r2 F4).
fn refusal_tail(class: &str, detail: &str) -> Value {
    let mut bounded = detail.to_owned();
    if bounded.chars().count() > 256 {
        bounded = bounded.chars().take(256).collect();
    }
    json!({"failure": {"failureClass": class, "detail": bounded}})
}

/// The result branch that preserves one discriminator-pinned receipt.
fn pinned_result(family: &Family, payload: Value) -> Value {
    json!({
        "result": {
            "schemaVersion": family.discriminator,
            "identity": family.identity,
            family.payload_key: payload,
        }
    })
}

/// The joined failure detail of one domain refusal: the preserved reason
/// codes, serialized as text through the JSON writer (r2 F4 — multiple
/// codes can no longer corrupt the quoted member).
fn failure_detail(reason_codes: Option<Vec<String>>) -> String {
    match reason_codes {
        Some(codes) if !codes.is_empty() => {
            let joined = codes.join(", ");
            if joined.chars().count() > 256 {
                joined.chars().take(256).collect()
            } else {
                joined
            }
        }
        _ => "domain-failure".to_owned(),
    }
}

/// One refusal output with the empty-input snapshot (nothing was read).
fn refusal(
    family: &Family,
    tool: &str,
    scope: &Scope,
    set: lekalo_core::diagnostics::DiagnosticSet,
    status: &'static str,
    class: &str,
    detail: &str,
) -> ToolOutput {
    let diagnostics_value = serde_json::to_value(&set).unwrap_or_else(|_| json!([]));
    let diagnostics = match diagnostics_value {
        Value::Array(items) => items,
        _ => Vec::new(),
    };
    ToolOutput {
        envelope: envelope(
            family,
            tool,
            scope,
            &snapshot_ref(&Inputs::new()),
            status,
            bounded_diagnostics(Some(diagnostics)).unwrap_or_else(|_| json!([])),
            Some(refusal_tail(class, detail)),
        ),
        is_error: true,
    }
}

/// The typed refusal envelope for one tool family, callable from the
/// resource dispatcher: the same closed members, with the scope echoed
/// only when both identities satisfy the closed grammar (unknown or
/// malformed identities project to the fixed `unknown` token; no value
/// is ever echoed).
pub(crate) fn refusal_envelope(
    tool: &str,
    project: Option<&str>,
    group: Option<&str>,
    set: lekalo_core::diagnostics::DiagnosticSet,
    status: &'static str,
    class: &str,
    detail: &str,
) -> String {
    let family = family_of(tool);
    let scope = Scope {
        project: match project {
            Some(id) if policy::admit_id(id).is_ok() => id,
            _ => "unknown",
        },
        group: match group {
            Some(id) if policy::admit_id(id).is_ok() => Some(id),
            _ => None,
        },
    };
    refusal(&family, tool, &scope, set, status, class, detail).envelope
}

fn finish_envelope(
    family: &Family,
    tool: &str,
    scope: &Scope,
    inputs: Inputs,
    mode_tag: Option<&str>,
    result: DomainResult,
) -> Result<ToolOutput, String> {
    let raw = result.to_json_string();
    let view = EnvelopeView::parse(&raw)?;
    let diagnostics = bounded_diagnostics(view.diagnostics()?)?;
    let reason_codes = view.reason_codes()?;
    match result {
        DomainResult::Valid { .. } => {
            // The doctor document is the whole Valid envelope; every
            // other family reads its own typed payload member.
            let payload = if family.slug == "target-status" {
                serde_json::from_str::<Value>(&raw).map_err(|error| error.to_string())?
            } else {
                let member = view
                    .payload_for(family)
                    .ok_or_else(|| "missing-payload".to_owned())?;
                serde_json::from_str(member.get()).map_err(|error| error.to_string())?
            };
            let tail = if family.slug == "validate" {
                // The exact validation success receipt: status,
                // modelVersion, validation, plus the optional findings
                // members when the run recorded them.
                let mut receipt = json!({
                    "result": {
                        "status": "valid",
                        "modelVersion": view.model_version.clone().unwrap_or(Value::Null),
                        "validation": payload,
                    }
                });
                if let Some(member) = diagnostics.as_array() {
                    if !member.is_empty() {
                        receipt["result"]["diagnostics"] = Value::Array(member.clone());
                    }
                }
                if let Some(reasons) = &reason_codes {
                    if !reasons.is_empty() {
                        receipt["result"]["reasonCodes"] = json!(reasons);
                    }
                }
                receipt
            } else if family.slug == "graph" {
                // The frozen core query payloads ride unmodified under the
                // mode tag; export carries the canonical whole-graph
                // document with its own discriminator. No discriminator is
                // ever injected into a query payload.
                let mode = mode_tag.unwrap_or("show");
                if mode == "export" {
                    json!({"result": {"mode": mode, "export": payload}})
                } else {
                    json!({"result": {"mode": mode, "query": payload}})
                }
            } else {
                pinned_result(family, payload)
            };
            Ok(ToolOutput {
                envelope: envelope(
                    family,
                    tool,
                    scope,
                    &snapshot_ref(&inputs),
                    "valid",
                    diagnostics,
                    Some(tail),
                ),
                is_error: false,
            })
        }
        _ => {
            let status = domain_status(result.status());
            let class = failure_class(status);
            let detail = failure_detail(reason_codes);
            Ok(ToolOutput {
                envelope: envelope(
                    family,
                    tool,
                    scope,
                    &snapshot_ref(&inputs),
                    status,
                    diagnostics,
                    Some(refusal_tail(class, &detail)),
                ),
                is_error: true,
            })
        }
    }
}

/// The domain status string of one refusal, mapped per `result.rs`.
fn domain_status(status: lekalo_core::Status) -> &'static str {
    match status {
        lekalo_core::Status::Valid => "valid",
        lekalo_core::Status::Invalid => "invalid",
        lekalo_core::Status::Denied => "denied",
        lekalo_core::Status::Unsupported => "unsupported",
        lekalo_core::Status::Unavailable => "unavailable",
        lekalo_core::Status::UnsupportedVersion => "unsupported-version",
    }
}

fn failure_class(domain_status: &str) -> &'static str {
    match domain_status {
        "invalid" => "domain-invalid",
        "denied" => "domain-denied",
        "unavailable" => "domain-unavailable",
        "unsupported" => "domain-unsupported",
        "unsupported-version" => "domain-unsupported-version",
        _ => "domain-unavailable",
    }
}

/// The one public entry: execute a fully parsed request over the catalog.
///
/// Authorization runs first (catalog grants intersect group permissions),
/// then the semantic scope closure, then the typed core seam. Every
/// refusal before any read keeps the empty-input snapshot so the envelope
/// is always closed.
pub fn execute(catalog: &ServerCatalog, request: &ToolRequest) -> Result<ToolOutput, String> {
    let output = execute_inner(catalog, request)?;
    // The single output screening pass (F17): no envelope ships with a
    // secret-shaped token; the whole output refuses with the registered
    // `mcp.sensitive-output` denial instead.
    if screen::screen_envelope(&output.envelope).is_err() {
        let common: &Common = request.common();
        let family = family_of(request.tool_name());
        let scope = Scope {
            project: &common.project_id,
            group: common.group_id.as_deref(),
        };
        return Ok(refusal(
            &family,
            request.tool_name(),
            &scope,
            lekalo_core::mcp::diagnostic::sensitive_output_set(
                "tool-result",
                "secret-shaped-output",
            ),
            "denied",
            "sensitive-output",
            "mcp.sensitive-output",
        ));
    }
    Ok(output)
}

fn execute_inner(catalog: &ServerCatalog, request: &ToolRequest) -> Result<ToolOutput, String> {
    let common: &Common = request.common();
    let family = family_of(request.tool_name());
    let scope = Scope {
        project: &common.project_id,
        group: common.group_id.as_deref(),
    };
    // 1. Catalog admission: unregistered project/group identities are a
    // denial before any read.
    if !catalog.core.admits_project(&common.project_id) {
        return Ok(refusal(
            &family,
            request.tool_name(),
            &scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("project", "unregistered"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ));
    }
    if let Some(group) = scope.group {
        let decision = catalog.core.admits_group(scope.project, group);
        if decision != ScopeDecision::Admitted {
            let reason = match decision {
                ScopeDecision::Widening => "group-of-another-project",
                _ => "unregistered",
            };
            return Ok(refusal(
                &family,
                request.tool_name(),
                &scope,
                lekalo_core::mcp::diagnostic::scope_denied_set("group", reason),
                "denied",
                "scope-denied",
                "mcp.scope-denied",
            ));
        }
    }
    // 2. Semantic scope closure per tool.
    if let Some(output) = check_closure(catalog, request, &scope, &family) {
        return Ok(output);
    }
    // 3. Dispatch on the reused typed seams.
    run(catalog, request, &scope, &family)
}

/// The module head of one semantic operand, when it is a grammar-valid
/// module id. Review r2 F1: a symbol spelled without a valid module head
/// (`focus_task`, `send_digest`) produced an EMPTY required set that
/// admitted under every group; the caller now refuses instead.
fn module_of(symbol: &str) -> Option<String> {
    symbol
        .split('.')
        .next()
        .filter(|head| policy::admit_module_id(head).is_ok())
        .map(str::to_owned)
}

/// The launch-group closure check.
///
/// Review r2 F1/codex 1: the required set is resolved from the compiled
/// graph, never from raw operand text. Symbol-scoped operations whose
/// result is bounded to the requested symbol (inspect) admit when the
/// operand's module fits the allow-list and are re-checked against the
/// disclosed payload after the run; every operation whose full core
/// result inherently spans the whole project (context capsules, impact
/// traversal, graph receipts and exports, validation, target status,
/// changed traces) refuses — refusing beats fabricating a narrowed
/// result, and the disclosed dependency facts of a capsule or an impact
/// cannot be proven inside the grant without running it.
fn check_closure(
    catalog: &ServerCatalog,
    request: &ToolRequest,
    scope: &Scope,
    family: &Family,
) -> Option<ToolOutput> {
    let group = scope.group?;
    let grant = match catalog.core.group(scope.project, group) {
        Some(grant) => grant,
        None => {
            return Some(refusal(
                family,
                request.tool_name(),
                scope,
                lekalo_core::mcp::diagnostic::scope_denied_set("group", "unregistered"),
                "denied",
                "scope-denied",
                "mcp.scope-denied",
            ))
        }
    };
    let inspect_scoped = matches!(
        request,
        ToolRequest::Inspect { .. }
            | ToolRequest::Impact {
                mode: ImpactMode::Symbol,
                ..
            }
    );
    if !inspect_scoped {
        return Some(refusal(
            family,
            request.tool_name(),
            scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("module", "requires-full-scope"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ));
    }
    let symbol = match request {
        ToolRequest::Inspect { symbol, .. } => symbol.as_str(),
        ToolRequest::Impact {
            mode: ImpactMode::Symbol,
            symbol: Some(symbol),
            ..
        } => symbol.as_str(),
        _ => unreachable!("inspect_scoped above"),
    };
    // The operand must name its module before any read: a grammar-invalid
    // head refuses instead of emptying the required set (r2 F1).
    let Some(head) = module_of(symbol) else {
        return Some(refusal(
            family,
            request.tool_name(),
            scope,
            lekalo_core::mcp::diagnostic::input_invalid_set("symbol", "unresolved-module"),
            "invalid",
            "invalid-input",
            "mcp.input-invalid",
        ));
    };
    if !grant.modules.contains(&head) {
        return Some(refusal(
            family,
            request.tool_name(),
            scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("module", "outside-group"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ));
    }
    None
}

/// The modules one disclosed core payload requires, resolved through the
/// compiled graph (r2 F1's result-closure half): every string in the
/// payload that resolves to a real semantic node contributes its module.
/// Ids that resolve to nothing (artifact tokens like `PLANNER-REQ-001`,
/// digests, spellings) contribute nothing — they are not module facts.
fn disclosed_modules(
    payload: &Value,
    graph: &lekalo_core::graph::DependencyGraph,
) -> BTreeSet<String> {
    fn walk(
        value: &Value,
        graph: &lekalo_core::graph::DependencyGraph,
        out: &mut BTreeSet<String>,
    ) {
        match value {
            Value::String(text) => {
                if let Some(node) = graph.resolve(text) {
                    if let Some(module) = node.module() {
                        out.insert(module.to_owned());
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, graph, out);
                }
            }
            Value::Object(members) => {
                for member in members.values() {
                    walk(member, graph, out);
                }
            }
            _ => {}
        }
    }
    let mut modules = BTreeSet::new();
    walk(payload, graph, &mut modules);
    modules
}

/// The compiled graph of one admitted project, for the group-closure
/// checks. A load failure is a typed backend refusal, never a fallback.
fn compiled_graph(
    entry: &super::catalog::ProjectEntry,
) -> Result<lekalo_core::graph::DependencyGraph, String> {
    let selection = LoadSelection {
        project: Some(entry.selector.clone()),
    };
    let model =
        lekalo_core::loader::normalize_model(&selection).map_err(|_| "model-load".to_owned())?;
    let compilation = lekalo_core::ir::compile(&model).map_err(|_| "model-compile".to_owned())?;
    lekalo_core::graph::build(&compilation.project).map_err(|_| "graph-build".to_owned())
}

/// The disclosed-payload closure check for a grouped symbol run (r2 F1):
/// the payload may only carry facts whose modules the grant admits.
pub(crate) fn disclosed_closure_refusal(
    catalog: &ServerCatalog,
    project: &str,
    group: &str,
    tool: &str,
    payload: &Value,
) -> Option<ToolOutput> {
    let scope = Scope {
        project,
        group: Some(group),
    };
    let family = family_of(tool);
    let Some(grant) = catalog.core.group(project, group) else {
        return Some(refusal(
            &family,
            tool,
            &scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("group", "unregistered"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ));
    };
    let Some(entry) = catalog.projects.get(project) else {
        return Some(refusal(
            &family,
            tool,
            &scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("project", "unregistered"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ));
    };
    let graph = match compiled_graph(entry) {
        Ok(graph) => graph,
        Err(_) => {
            return Some(refusal(
                &family,
                tool,
                &scope,
                lekalo_core::mcp::diagnostic::backend_unavailable_set("graph"),
                "unavailable",
                "backend-unavailable",
                "mcp.backend-unavailable",
            ));
        }
    };
    let disclosed = disclosed_modules(payload, &graph);
    if disclosed.is_subset(&grant.modules) {
        None
    } else {
        Some(refusal(
            &family,
            tool,
            &scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("module", "outside-group"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ))
    }
}

/// The admitted-input digest of one project: the canonical model bytes
/// plus the raw `lekalo.lock` bytes when present (review r2 F7 — the
/// snapshot pins the admitted inputs, not only the canonical model, so a
/// lock edit or a byte drift cannot hide behind an unchanged ref). A load
/// failure is `None`; the caller fails closed.
pub(crate) fn admitted_inputs_for(entry: &super::catalog::ProjectEntry) -> Option<Inputs> {
    let selection = LoadSelection {
        project: Some(entry.selector.clone()),
    };
    let model = lekalo_core::loader::normalize_model(&selection).ok()?;
    let mut inputs = Inputs::new();
    inputs.insert(
        "model:project".to_owned(),
        domain_digest(
            "model",
            lekalo_core::loader::canonical_model_bytes(&model).as_bytes(),
        ),
    );
    let lock_path = entry.root.join("lekalo.lock");
    let lock_digest = match std::fs::symlink_metadata(&lock_path) {
        Ok(metadata) if metadata.is_file() => {
            let bytes = std::fs::read(&lock_path).ok()?;
            domain_digest("lock", &bytes)
        }
        Ok(_) => return None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "absent".to_owned(),
        Err(_) => return None,
    };
    inputs.insert("lock:project".to_owned(), lock_digest);
    Some(inputs)
}

/// The stale-snapshot refusal: authorized input bytes changed during
/// evaluation; the candidate result is discarded, never answered.
fn stale(family: &Family, tool: &str, scope: &Scope) -> ToolOutput {
    refusal(
        family,
        tool,
        scope,
        lekalo_core::mcp::diagnostic::snapshot_stale_set("model"),
        "unavailable",
        "snapshot-stale",
        "mcp.snapshot-stale",
    )
}

/// Run the typed seam for one request.
fn run(
    catalog: &ServerCatalog,
    request: &ToolRequest,
    scope: &Scope,
    family: &Family,
) -> Result<ToolOutput, String> {
    if let ToolRequest::TraceExport { manifest_ref, .. } = request {
        return run_trace_export(catalog, scope, family, request.tool_name(), manifest_ref);
    }
    // The granted root is the only selection seam: no environment
    // variable or working-directory discovery can widen it.
    let Some(entry) = catalog.projects.get(scope.project) else {
        return Ok(refusal(
            family,
            request.tool_name(),
            scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("project", "unregistered"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ));
    };
    let project = Some(entry.selector.clone());
    // Fail closed when the admitted inputs cannot even be pinned before
    // the read (r2 F7): an unpinnable candidate is never evaluated.
    let Some(inputs_before) = admitted_inputs_for(entry) else {
        return Ok(refusal(
            family,
            request.tool_name(),
            scope,
            lekalo_core::mcp::diagnostic::backend_unavailable_set("input-identity"),
            "unavailable",
            "backend-unavailable",
            "mcp.backend-unavailable",
        ));
    };
    let result = match request {
        ToolRequest::Inspect {
            symbol, include, ..
        } => {
            let include_text = if include.is_empty() {
                None
            } else {
                let names: Vec<&str> = include.iter().map(|item| item.as_str()).collect();
                Some(names.join(","))
            };
            crate::run_inspect(symbol, include_text.as_deref(), &project)
        }
        ToolRequest::Impact {
            mode,
            symbol,
            base_ref,
            head_ref,
            worktree,
            depth,
            module,
            target,
            relations,
            profile,
            ..
        } => {
            if *mode == ImpactMode::Changed && !catalog.core.admits_git_changed_mode() {
                // Changed-mode Git access requires explicit catalog
                // admission; without it refuse instead of scanning.
                return Ok(refusal(
                    family,
                    request.tool_name(),
                    scope,
                    lekalo_core::mcp::diagnostic::backend_unavailable_set("git.changed-mode"),
                    "unavailable",
                    "backend-unavailable",
                    "mcp.backend-unavailable",
                ));
            }
            crate::run_impact(crate::ImpactArgs {
                symbol: symbol.clone(),
                changed: *mode == ImpactMode::Changed,
                base: base_ref.clone(),
                head: head_ref.clone(),
                worktree: *worktree,
                depth: depth.unwrap_or(3),
                module: module.clone(),
                target: target.clone(),
                relation: relations.clone(),
                profile: profile.clone().unwrap_or_else(|| "default".to_owned()),
                project: project.clone(),
            })
        }
        ToolRequest::Context {
            mode,
            symbol,
            changed_symbols,
            changed_symbol_count,
            budget,
            spans,
            ..
        } => {
            // The exact evaluated set is the typed vector the closure
            // checked: no comma round-trip, and the declared count must
            // still match the dispatched symbols. The count mismatch is
            // the contract-spelled refusal (r2 F12: `invalid-input`).
            if *changed_symbol_count != changed_symbols.len() as u64 {
                return Ok(refusal(
                    family,
                    request.tool_name(),
                    scope,
                    lekalo_core::mcp::diagnostic::input_invalid_set(
                        "changedSymbolCount",
                        "count-mismatch",
                    ),
                    "invalid",
                    "invalid-input",
                    "mcp.input-invalid",
                ));
            }
            crate::run_context_typed(
                if *mode == ContextMode::Symbol {
                    symbol.clone()
                } else {
                    None
                },
                if *mode == ContextMode::Changed {
                    Some(changed_symbols.clone())
                } else {
                    None
                },
                *budget,
                *spans,
                &project,
            )
        }
        ToolRequest::Graph {
            mode,
            symbol,
            from,
            to,
            transitive,
            ..
        } => {
            let command = match mode {
                GraphMode::Show => crate::GraphCommands::Show {
                    symbol: symbol.clone().unwrap_or_default(),
                    project: project.clone(),
                },
                GraphMode::Callers => crate::GraphCommands::Callers {
                    symbol: symbol.clone().unwrap_or_default(),
                    transitive: *transitive,
                    project: project.clone(),
                },
                GraphMode::Path => crate::GraphCommands::Path {
                    from: from.clone().unwrap_or_default(),
                    to: to.clone().unwrap_or_default(),
                    project: project.clone(),
                },
                GraphMode::Export => crate::GraphCommands::Export {
                    project: project.clone(),
                    format: crate::GraphFormat::Json,
                    spans: false,
                },
            };
            crate::run_graph(command, true)
        }
        ToolRequest::Validate { module, strict, .. } => {
            crate::run_validate(project.clone(), module.clone(), *strict, true)
        }
        ToolRequest::TargetStatus {
            target_id,
            profile_id,
            ..
        } => run_target_status(
            catalog,
            project.clone(),
            target_id.as_deref(),
            profile_id.as_deref(),
        ),
        ToolRequest::TraceExport { .. } => unreachable!("matched above"),
    };
    // The admitted inputs are re-checked after the run; a change or a
    // re-check failure discards the candidate (r2 F7, fail closed).
    let Some(inputs_after) = admitted_inputs_for(entry) else {
        return Ok(stale(family, request.tool_name(), scope));
    };
    let mut inputs = Inputs::new();
    if inputs_before != inputs_after {
        return Ok(stale(family, request.tool_name(), scope));
    }
    for (identity, digest) in &inputs_after {
        inputs.insert(identity.clone(), digest.clone());
    }
    // The group ceiling also bounds the disclosed payload (r2 F1): a
    // grouped symbol run whose result carries another module's facts
    // refuses before serialization.
    if scope.group.is_some() && result.status() == lekalo_core::Status::Valid {
        let raw = result.to_json_string();
        let document: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        let payload_for_check = if family.slug == "target-status" {
            document
        } else {
            document
                .get(family.payload_key)
                .cloned()
                .unwrap_or(Value::Null)
        };
        if let Some(group) = scope.group {
            if let Some(output) = disclosed_closure_refusal(
                catalog,
                scope.project,
                group,
                request.tool_name(),
                &payload_for_check,
            ) {
                return Ok(output);
            }
        }
    }
    let mode_tag = match request {
        ToolRequest::Graph { mode, .. } => Some(match mode {
            GraphMode::Show => "show",
            GraphMode::Callers => "callers",
            GraphMode::Path => "path",
            GraphMode::Export => "export",
        }),
        _ => None,
    };
    finish_envelope(family, request.tool_name(), scope, inputs, mode_tag, result)
}

/// `lekalo_target_status`: the typed doctor lock/inventory/capability-
/// profile panel. `targetId` selects the lock or adapter panels;
/// `profileId` selects the capability-profile panel. The selection
/// vocabulary is closed; anything else is a typed input refusal before
/// any read. Review r2 F16: the selection projects the doctor report to
/// the named panels (a filtered check set with its own derived verdict)
/// instead of being validated and discarded.
fn run_target_status(
    catalog: &ServerCatalog,
    project: Option<String>,
    target_id: Option<&str>,
    profile_id: Option<&str>,
) -> DomainResult {
    const PANELS: [&str; 4] = ["lock", "inventory", "trust", "profiles"];
    let mut selected: Vec<&str> = Vec::new();
    for selection in target_id.into_iter().chain(profile_id) {
        if !PANELS.contains(&selection) {
            return DomainResult::invalid(lekalo_core::mcp::diagnostic::input_invalid_set(
                "targetId",
                "unknown-panel",
            ));
        }
        if !selected.contains(&selection) {
            selected.push(selection);
        }
    }
    // F9: a spawned `git` is reachable only through the catalog-gated
    // capability. Without the explicit changed-mode admission the status
    // projection degrades to typed git-unavailable facts instead of
    // spawning any process.
    let result = if catalog.core.admits_git_changed_mode() {
        crate::run_status(project)
    } else {
        crate::run_status_without_git(project)
    };
    if selected.is_empty() {
        return result;
    }
    project_doctor_panels(&result, &selected)
}

/// The doctor panel projection: filter the status report's checks to the
/// selected closed panels and derive the panel verdict with the same
/// rule the doctor uses for `status` (blocked only on blocked checks).
/// The projection never fabricates a check: an empty panel ships an
/// empty check set with the derived `ready` verdict, honestly reporting
/// that the panel has no observations.
fn project_doctor_panels(result: &DomainResult, selected: &[&str]) -> DomainResult {
    const PANEL_CHECKS: [(&str, &[&str]); 4] = [
        ("lock", &["lock.freshness", "artifacts.drift"]),
        ("inventory", &["adapters.inventory"]),
        ("trust", &["adapters.trust"]),
        ("profiles", &["capabilities.profiles"]),
    ];
    let raw = result.to_json_string();
    let Ok(mut document) = serde_json::from_str::<Value>(&raw) else {
        return result.clone();
    };
    let Some(checks) = document.get_mut("checks").and_then(Value::as_array_mut) else {
        return result.clone();
    };
    checks.retain(|check| {
        let Some(id) = check.get("id").and_then(Value::as_str) else {
            return false;
        };
        PANEL_CHECKS
            .iter()
            .filter(|(name, _)| selected.contains(name))
            .any(|(_, ids)| ids.contains(&id))
    });
    // The panel verdict follows the status rule: blocked when a check is
    // blocked, degraded when any check reports, ready otherwise.
    let verdict = checks.iter().fold("ready", |current, check| {
        let state = check.get("state").and_then(Value::as_str).unwrap_or("ok");
        match (current, state) {
            (_, "blocked") => "blocked",
            ("blocked", _) => "blocked",
            (_, "unknown") => "blocked",
            (_, "degraded") => "degraded",
            (previous, _) => previous,
        }
    });
    if let Some(member) = document.get_mut("verdict") {
        *member = json!(verdict);
    }
    let projected = serde_json::to_string(&document).unwrap_or_else(|_| raw.clone());
    // The projected report keeps the receipt envelope shape; the human
    // summary is derived from the panel outcome (the MCP surface never
    // renders it, and this DomainResult never reaches the CLI).
    DomainResult::receipt(
        projected,
        format!("target status panel [{}]: {}", selected.join(","), verdict),
    )
}

/// `lekalo_trace_export`: the manifest reference is an opaque catalog id;
/// the bytes are admitted through the bounded no-follow reader and then
/// parsed by the preserved `TraceManifest::parse`. No auto-collection,
/// no path argument, no construction from the project. The admitted
/// bytes are re-checked after parse; a changed input refuses with the
/// stale-snapshot diagnostic instead of a partial success.
fn run_trace_export(
    catalog: &ServerCatalog,
    scope: &Scope,
    family: &Family,
    tool: &str,
    manifest_ref: &str,
) -> Result<ToolOutput, String> {
    let Some(logical) = catalog.core.admits_manifest(scope.project, manifest_ref) else {
        return Ok(refusal(
            family,
            tool,
            scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("manifest", "unregistered"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ));
    };
    let Some(entry) = catalog.projects.get(scope.project) else {
        return Ok(refusal(
            family,
            tool,
            scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("project", "unregistered"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ));
    };
    if policy::admit_relative_path(logical).is_err() {
        return Ok(refusal(
            family,
            tool,
            scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("manifest", "not-admitted"),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        ));
    }
    let bytes = match read::read_admitted(&entry.root, logical, read::MANIFEST_READ_CAP) {
        Ok(bytes) => bytes,
        Err(read::ReadRefusal::NotAdmitted) => {
            return Ok(refusal(
                family,
                tool,
                scope,
                lekalo_core::mcp::diagnostic::scope_denied_set("manifest", "not-admitted"),
                "denied",
                "scope-denied",
                "mcp.scope-denied",
            ));
        }
        Err(refusal_kind) => {
            let (status, class, detail, set) = match refusal_kind {
                read::ReadRefusal::Link => (
                    "unavailable",
                    "backend-unavailable",
                    "mcp.backend-unavailable",
                    lekalo_core::mcp::diagnostic::backend_unavailable_set("confinement.no-follow"),
                ),
                _ => (
                    "invalid",
                    "domain-invalid",
                    "trace.io-unreadable",
                    lekalo_core::trace::io_failure("file-unreadable"),
                ),
            };
            return Ok(refusal(family, tool, scope, set, status, class, detail));
        }
    };
    let digest = domain_digest("trace-manifest", &bytes);
    let mut inputs = Inputs::new();
    inputs.insert(format!("manifest:{manifest_ref}"), digest.clone());
    let manifest = match lekalo_core::trace::TraceManifest::parse(&bytes) {
        Ok(manifest) => manifest,
        Err(diagnostics) => {
            return Ok(refusal(
                family,
                tool,
                scope,
                diagnostics,
                "invalid",
                "domain-invalid",
                "trace.manifest-invalid",
            ));
        }
    };
    // Re-check the admitted bytes before serialization: a changed input
    // discards the candidate result.
    match read::read_admitted(&entry.root, logical, read::MANIFEST_READ_CAP) {
        Ok(rechecked) => {
            if domain_digest("trace-manifest", &rechecked) != digest {
                return Ok(stale(family, tool, scope));
            }
        }
        Err(_) => return Ok(stale(family, tool, scope)),
    }
    let canonical = match manifest.canonical_bytes() {
        Ok(canonical) => canonical,
        Err(diagnostics) => {
            return Ok(refusal(
                family,
                tool,
                scope,
                diagnostics,
                "invalid",
                "domain-invalid",
                "trace.canonical",
            ));
        }
    };
    let manifest_digest = match manifest.digest() {
        Ok(digest) => digest,
        Err(diagnostics) => {
            return Ok(refusal(
                family,
                tool,
                scope,
                diagnostics,
                "invalid",
                "domain-invalid",
                "trace.digest",
            ));
        }
    };
    let payload: Value = serde_json::from_str(&canonical).map_err(|error| error.to_string())?;
    let tail = json!({
        "result": {
            "schemaVersion": family.discriminator,
            "identity": family.identity,
            "trace": payload,
            "manifestDigest": manifest_digest,
        }
    });
    Ok(ToolOutput {
        envelope: envelope(
            family,
            tool,
            scope,
            &snapshot_ref(&inputs),
            "valid",
            json!([]),
            Some(tail),
        ),
        is_error: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lekalo_core::mcp::diagnostic;

    #[test]
    fn the_diagnostics_bound_refuses_instead_of_shipping_schema_invalid_output() {
        // Review r2 F20: the contract-declared cap is enforced — a
        // domain result carrying more than 64 diagnostics refuses as the
        // typed result-invalid fault instead of an invalid envelope.
        let mut items: Vec<Value> = (0..MAX_TOOL_DIAGNOSTICS)
            .map(|index| json!({"id": "mcp.input-invalid", "ordinal": index}))
            .collect();
        assert_eq!(
            bounded_diagnostics(Some(items.clone())).unwrap(),
            json!(items)
        );
        items.push(json!({"id": "mcp.input-invalid", "ordinal": 64}));
        let fault = bounded_diagnostics(Some(items)).unwrap_err();
        assert!(fault.contains("diagnostics-bound"), "{fault}");
    }

    #[test]
    fn the_failure_detail_joins_every_reason_code_through_the_writer() {
        // Review r2 F4/codex 3: the multi-code detail is a plain joined
        // string serialized by serde — the old quote-splice corruption
        // (`a","b`) cannot recur, and single-code details are unchanged.
        assert_eq!(
            failure_detail(Some(vec![
                "semantic.entity-identity-field-missing".to_owned(),
                "semantic.query-read-unresolved".to_owned(),
            ])),
            "semantic.entity-identity-field-missing, semantic.query-read-unresolved"
        );
        assert_eq!(failure_detail(Some(Vec::new())), "domain-failure");
        assert_eq!(failure_detail(None), "domain-failure");
        // The tail is built by serde: quotes in a code are escaped, and
        // the branch always parses as JSON.
        let tail = refusal_tail("domain-invalid", "codes: \"a\" and \"b\"");
        let parsed: Value = serde_json::from_str(&tail.to_string()).expect("serde-built tail");
        assert_eq!(parsed["failure"]["detail"], json!("codes: \"a\" and \"b\""));
    }

    #[test]
    fn every_refusal_envelope_parses_and_carries_the_registered_diagnostic() {
        // The refusal builder is the single refusal form: whatever the
        // class or detail, the envelope parses closed and the bound
        // holds (r2 F12's wire-check, at the unit level).
        for (class, detail) in [
            ("scope-denied", "mcp.scope-denied"),
            ("invalid-input", "mcp.input-invalid"),
            ("sensitive-output", "mcp.sensitive-output"),
            ("result-invalid", "mcp.result-invalid"),
        ] {
            let envelope = refusal_envelope(
                "lekalo_inspect",
                Some("planner"),
                None,
                diagnostic::scope_denied_set("project", "unregistered"),
                "denied",
                class,
                detail,
            );
            let value: Value =
                serde_json::from_str(&envelope).expect("the refusal envelope always parses");
            assert_eq!(value["failure"]["failureClass"], json!(class));
            assert_eq!(value["status"], "denied");
            assert_eq!(value["diagnostics"][0]["registry_version"], "0.8.0");
        }
    }
}
