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

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::value::RawValue;

use lekalo_core::loader::LoadSelection;
use lekalo_core::mcp::policy::{self, modules_admitted, ScopeDecision};
use lekalo_core::mcp::screen;
use lekalo_core::mcp::tools::{Common, ContextMode, GraphMode, ImpactMode, ToolRequest};
use lekalo_core::mcp::version::VERSION;
use lekalo_core::DomainResult;

use super::catalog::ServerCatalog;
use super::read;
use super::snapshot::{domain_digest, snapshot_ref, Inputs};
use super::stdio::compact_json;

/// A completed tool execution: the closed output-contract envelope and
/// whether the transport must mark it as an error result.
pub struct ToolOutput {
    pub envelope: String,
    pub is_error: bool,
}

/// The admission scope of one resolved request.
struct Scope<'a> {
    project: &'a str,
    group: Option<&'a str>,
}

/// The per-tool slug, discriminator, identity, and payload key.
pub(crate) struct Family {
    pub(crate) slug: &'static str,
    discriminator: &'static str,
    identity: &'static str,
    payload_key: &'static str,
    /// The `contractPins` member beyond the common three.
    pin: &'static str,
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
            "\"inspect\":\"0.2.16\"",
        ),
        "impact" => (
            "lekalo/impact/v0.2.16",
            "dev.lekalo.impact@0.2.16",
            "impact",
            "\"impact\":\"0.2.16\"",
        ),
        "context" => (
            "lekalo/context/v0.2.16",
            "dev.lekalo.context@0.2.16",
            "context",
            "\"context\":\"0.2.16\"",
        ),
        "graph" => (
            "lekalo/graph/v0.2.16",
            "dev.lekalo.graph@0.2.16",
            "graph",
            "\"graph\":\"0.2.16\"",
        ),
        "validate" => (
            "lekalo/validation-report/v0.6.4",
            "dev.lekalo.validation-report@0.6.4",
            "validation",
            "\"validation\":\"0.6.4\"",
        ),
        "target-status" => (
            "lekalo/doctor/v0.3.2",
            "dev.lekalo.doctor@0.3.2",
            "doctor",
            "\"doctor\":\"0.3.2\"",
        ),
        _ => (
            "lekalo/trace-manifest/v0.2.16",
            "dev.lekalo.trace-manifest@0.2.16",
            "trace",
            "\"traceManifest\":\"0.2.16\"",
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
/// verbatim into the MCP output (byte parity with core/CLI).
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
    model_version: Option<serde_json::Value>,
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
}

/// Assemble the closed output envelope. `tail` is the `result` branch of
/// a completed core call or the typed `failure` branch; the diagnostics
/// ride as the exact raw wire text when present.
fn envelope(
    family: &Family,
    tool: &str,
    scope: &Scope,
    snapshot: &str,
    status: &str,
    diagnostics_raw: Option<String>,
    tail: Option<String>,
) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("{\"schemaVersion\":\"lekalo/mcp-tool-");
    out.push_str(family.slug);
    out.push_str("-output/v");
    out.push_str(VERSION);
    out.push_str("\",\"identity\":\"dev.lekalo.mcp-tool-");
    out.push_str(family.slug);
    out.push_str("-output@");
    out.push_str(VERSION);
    out.push_str("\",\"tool\":\"");
    out.push_str(tool);
    out.push_str("\",\"scope\":{\"projectId\":\"");
    out.push_str(scope.project);
    out.push_str("\",\"groupId\":");
    match scope.group {
        Some(group) => {
            out.push('"');
            out.push_str(group);
            out.push('"');
        }
        None => out.push_str("null"),
    }
    out.push_str("},\"snapshotRef\":\"");
    out.push_str(snapshot);
    out.push_str("\",\"status\":\"");
    out.push_str(status);
    out.push_str("\",\"contractPins\":{\"diagnostic\":\"0.2.16\",\"diagnosticRegistry\":\"0.8.0\",\"protocolProfile\":\"2025-11-25\",");
    out.push_str(family.pin);
    out.push_str("},\"diagnostics\":");
    out.push_str(&diagnostics_raw.unwrap_or_else(|| "[]".to_owned()));
    if let Some(tail) = tail {
        out.push(',');
        out.push_str(&tail);
    }
    out.push('}');
    out
}

/// The typed MCP refusal branch (bounded, closed, no input echo).
fn refusal_tail(class: &str, detail: &str) -> String {
    let mut detail = detail.to_owned();
    if detail.len() > 256 {
        detail.truncate(256);
    }
    format!("\"failure\":{{\"failureClass\":\"{class}\",\"detail\":\"{detail}\"}}")
}

/// The result branch that preserves one discriminator-pinned receipt.
fn pinned_result(family: &Family, raw: &str) -> String {
    format!(
        "\"result\":{{\"schemaVersion\":\"{}\",\"identity\":\"{}\",\"{}\":{}}}",
        family.discriminator, family.identity, family.payload_key, raw
    )
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
    let diagnostics = serde_json::to_string(&set).unwrap_or_else(|_| "[]".to_owned());
    ToolOutput {
        envelope: envelope(
            family,
            tool,
            scope,
            &snapshot_ref(&Inputs::new()),
            status,
            Some(diagnostics),
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
    match result {
        DomainResult::Valid { .. } => {
            // The doctor document is the whole Valid envelope; every
            // other family reads its own raw payload member.
            let spliced = if family.slug == "target-status" {
                compact_json(&raw)
            } else {
                let payload = view
                    .payload_for(family)
                    .ok_or_else(|| "missing-payload".to_owned())?;
                payload.get().to_owned()
            };
            let tail = if family.slug == "validate" {
                // The exact validation success receipt: status,
                // modelVersion, validation, plus the optional findings
                // members when the run recorded them.
                let mut receipt = format!(
                    "\"result\":{{\"status\":\"valid\",\"modelVersion\":{},\"validation\":{}",
                    view.model_version
                        .clone()
                        .unwrap_or(serde_json::Value::Null),
                    spliced
                );
                if let Some(diagnostics) = view.diagnostics {
                    receipt.push_str(",\"diagnostics\":");
                    receipt.push_str(&compact_json(diagnostics.get()));
                }
                if let Some(reasons) = view.reason_codes {
                    receipt.push_str(",\"reasonCodes\":");
                    receipt.push_str(&compact_json(reasons.get()));
                }
                receipt.push('}');
                receipt
            } else if family.slug == "graph" {
                // The frozen core query payloads ride unmodified under the
                // mode tag; export carries the canonical whole-graph
                // document with its own discriminator. No discriminator is
                // ever injected into a query payload.
                let mode = mode_tag.unwrap_or("show");
                if mode == "export" {
                    format!("\"result\":{{\"mode\":\"export\",\"export\":{}}}", spliced)
                } else {
                    format!("\"result\":{{\"mode\":\"{}\",\"query\":{}}}", mode, spliced)
                }
            } else {
                pinned_result(family, &spliced)
            };
            Ok(ToolOutput {
                envelope: envelope(
                    family,
                    tool,
                    scope,
                    &snapshot_ref(&inputs),
                    "valid",
                    view.diagnostics.map(|raw| compact_json(raw.get())),
                    Some(tail),
                ),
                is_error: false,
            })
        }
        _ => {
            let status = domain_status(result.status());
            let class = failure_class(status);
            let detail = view
                .reason_codes
                .map(|codes| {
                    compact_json(codes.get())
                        .trim_matches(|c| c == '[' || c == ']' || c == '"' || c == ' ')
                        .to_owned()
                })
                .unwrap_or_else(|| "domain-failure".to_owned());
            Ok(ToolOutput {
                envelope: envelope(
                    family,
                    tool,
                    scope,
                    &snapshot_ref(&inputs),
                    status,
                    view.diagnostics.map(|raw| compact_json(raw.get())),
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

/// The module ids one request requires, for the group closure check.
fn required_modules(request: &ToolRequest) -> BTreeSet<String> {
    let module_of = |symbol: &str| -> Option<String> {
        symbol
            .split('.')
            .next()
            .filter(|head| policy::admit_module_id(head).is_ok())
            .map(str::to_owned)
    };
    match request {
        ToolRequest::Inspect { symbol, .. } => module_of(symbol).into_iter().collect(),
        ToolRequest::Impact {
            mode: ImpactMode::Symbol,
            symbol: Some(symbol),
            ..
        } => module_of(symbol).into_iter().collect(),
        ToolRequest::Context {
            mode: ContextMode::Symbol,
            symbol: Some(symbol),
            ..
        } => module_of(symbol).into_iter().collect(),
        ToolRequest::Context {
            mode: ContextMode::Changed,
            changed_symbols,
            ..
        } => changed_symbols
            .iter()
            .filter_map(|s| module_of(s))
            .collect(),
        ToolRequest::Graph {
            mode: GraphMode::Path,
            from: Some(from),
            to: Some(to),
            ..
        } => module_of(from).into_iter().chain(module_of(to)).collect(),
        ToolRequest::Graph {
            mode: GraphMode::Show | GraphMode::Callers,
            symbol: Some(symbol),
            ..
        } => module_of(symbol).into_iter().collect(),
        _ => BTreeSet::new(),
    }
}

/// The group closure check: full-scope operations refuse under a group
/// (never a fabricated full graph), symbol-scoped operations check their
/// module closure before any read.
fn check_closure(
    catalog: &ServerCatalog,
    request: &ToolRequest,
    scope: &Scope,
    family: &Family,
) -> Option<ToolOutput> {
    let group = scope.group?;
    let full_scope = matches!(
        request,
        ToolRequest::Graph {
            mode: GraphMode::Export,
            ..
        } | ToolRequest::Validate { .. }
            | ToolRequest::TargetStatus { .. }
            | ToolRequest::Impact {
                mode: ImpactMode::Changed,
                ..
            }
            | ToolRequest::TraceExport { .. }
    );
    let denied = if full_scope {
        Some("requires-full-scope")
    } else {
        let required = required_modules(request);
        match modules_admitted(&catalog.core, scope.project, Some(group), &required) {
            ScopeDecision::Admitted => None,
            _ => Some("outside-group"),
        }
    };
    denied.map(|reason| {
        refusal(
            family,
            request.tool_name(),
            scope,
            lekalo_core::mcp::diagnostic::scope_denied_set("module", reason),
            "denied",
            "scope-denied",
            "mcp.scope-denied",
        )
    })
}

/// The exact content digest of the admitted model inputs, or `None` when
/// the project does not load (the tool call itself will fail closed).
fn model_digest(project: &Option<String>) -> Option<String> {
    let selection = LoadSelection {
        project: project.clone(),
    };
    let model = lekalo_core::loader::normalize_model(&selection).ok()?;
    Some(domain_digest(
        "model",
        lekalo_core::loader::canonical_model_bytes(&model).as_bytes(),
    ))
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
    let digest_before = model_digest(&project);
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
            // still match the dispatched symbols.
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
                    "input-invalid",
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
    let digest_after = model_digest(&project);
    let mut inputs = Inputs::new();
    match (&digest_before, &digest_after) {
        (Some(before), Some(after)) if before == after => {
            inputs.insert("model:project".to_owned(), after.clone());
        }
        (Some(_), Some(_)) => return Ok(stale(family, request.tool_name(), scope)),
        _ => {}
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
/// any read.
fn run_target_status(
    catalog: &ServerCatalog,
    project: Option<String>,
    target_id: Option<&str>,
    profile_id: Option<&str>,
) -> DomainResult {
    const PANELS: [&str; 4] = ["lock", "inventory", "trust", "profiles"];
    for selection in target_id.into_iter().chain(profile_id) {
        if !PANELS.contains(&selection) {
            return DomainResult::invalid(lekalo_core::mcp::diagnostic::input_invalid_set(
                "targetId",
                "unknown-panel",
            ));
        }
    }
    // F9: a spawned `git` is reachable only through the catalog-gated
    // capability. Without the explicit changed-mode admission the status
    // projection degrades to typed git-unavailable facts instead of
    // spawning any process.
    if catalog.core.admits_git_changed_mode() {
        crate::run_status(project)
    } else {
        crate::run_status_without_git(project)
    }
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
    let tail = format!(
        "\"result\":{{\"schemaVersion\":\"{}\",\"identity\":\"{}\",\"trace\":{canonical},\"manifestDigest\":\"{manifest_digest}\"}}",
        family.discriminator, family.identity
    );
    Ok(ToolOutput {
        envelope: envelope(
            family,
            tool,
            scope,
            &snapshot_ref(&inputs),
            "valid",
            None,
            Some(tail),
        ),
        is_error: false,
    })
}
