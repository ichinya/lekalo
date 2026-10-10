//! The `lekalo mcp` command family (issue #33): `describe` emits the
//! embedded deterministic capability manifest without reading a project
//! or starting a server; `serve` requires explicit project selection,
//! has read-only authorization unconditionally, and owns stdout
//! exclusively as MCP JSON-RPC.
//!
//! The lifecycle is fully isolated from the ordinary command rendering:
//! no CLI startup path attempts MCP negotiation, an install, or any
//! network access, so a missing, disabled, or crashed server can never
//! make the ordinary CLI unusable.

mod catalog;
mod dispatch;
mod execution;
mod read;
mod snapshot;
mod stdio;

use lekalo_core::DomainResult;

/// The `mcp` subcommands.
#[derive(Debug, clap::Subcommand)]
pub(crate) enum McpCommands {
    /// Emit the embedded deterministic schema/capability manifest.
    Describe,
    /// Serve the read-only MCP session over one stdio transport.
    Serve {
        /// The transport; stdio is the only one this build pins.
        #[arg(long)]
        stdio: bool,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
        /// A named group already defined in the trusted catalog; it can
        /// narrow, never invent, memberships.
        #[arg(long, value_name = "ID")]
        group: Option<String>,
        /// The trusted launcher catalog (a launcher operand, never a
        /// remotely supplied tool argument).
        #[arg(long, value_name = "FILE")]
        catalog_config: Option<String>,
        /// The per-request deadline ceiling reduction.
        #[arg(long, value_name = "MS")]
        timeout_ms: Option<u64>,
        /// The response bound reduction.
        #[arg(long, value_name = "BYTES")]
        max_response_bytes: Option<usize>,
    },
}

/// The selected MCP subcommand outcome: `describe` rides the ordinary
/// status-owned envelope stream; `serve` owns stdout exclusively as
/// protocol frames, so its terminal receipt never prints (the AdapterRun
/// ownership pattern).
pub(crate) enum McpRun {
    Envelope(DomainResult),
    Owned(DomainResult),
}

/// Run the selected MCP subcommand. `json` is the global `--json` flag;
/// `serve` rejects it with the ordinary pre-server usage diagnostics
/// because stdout is protocol property from the first frame.
pub(crate) fn run(command: McpCommands, json: bool) -> McpRun {
    match command {
        McpCommands::Describe => McpRun::Envelope(describe(json)),
        McpCommands::Serve {
            stdio,
            project,
            group,
            catalog_config,
            timeout_ms,
            max_response_bytes,
        } => McpRun::Owned(serve_start(
            stdio,
            project,
            group,
            catalog_config,
            timeout_ms,
            max_response_bytes,
            json,
        )),
    }
}

/// `lekalo mcp describe --json`: the embedded capabilities manifest. No
/// project read, no server, no side effects; the same bytes every time.
fn describe(json: bool) -> DomainResult {
    // The embedded contracts are verified once per process; a drift is a
    // developer fault that fails closed instead of serving.
    if let Err(fault) = lekalo_core::mcp::schemas::verified() {
        return DomainResult::invalid(lekalo_core::mcp::diagnostic::result_invalid_set(&fault));
    }
    let manifest = lekalo_core::mcp::capability::manifest();
    let pretty = serde_json::to_string_pretty(&manifest).unwrap_or_default();
    let human = format!(
        "lekalo mcp capabilities {} (protocol {}, read-only)\ntools: {}\nresources: {}",
        lekalo_core::mcp::version::VERSION,
        lekalo_core::mcp::version::PROTOCOL_PROFILE,
        lekalo_core::mcp::tools::TOOL_NAMES.join(" "),
        lekalo_core::mcp::resources::FAMILY_KEYS.join(" "),
    );
    if json {
        DomainResult::graph(
            lekalo_core::mcp::capability::describe_json(),
            human,
            Vec::new(),
        )
    } else {
        DomainResult::receipt(pretty, human)
    }
}

/// `lekalo mcp serve --stdio --project DIR [--group ID]
/// [--catalog-config FILE]`: resolve the trusted catalog, apply the
/// effective limits, and own stdout until EOF.
#[allow(clippy::too_many_arguments)]
fn serve_start(
    stdio: bool,
    project: Option<String>,
    group: Option<String>,
    catalog_config: Option<String>,
    timeout_ms: Option<u64>,
    max_response_bytes: Option<usize>,
    json: bool,
) -> DomainResult {
    // Pre-server usage diagnostics, before any protocol byte.
    if json {
        return DomainResult::usage_error();
    }
    if !stdio {
        return DomainResult::usage_error();
    }
    let catalog = match (&catalog_config, &project) {
        (Some(path), _) => {
            let catalog_loaded = match catalog::load(path) {
                Ok(loaded) => loaded,
                Err(result) => return result,
            };
            let mut catalog = catalog_loaded;
            // An explicit --project must name a config project (by id) or
            // narrow the served set to exactly that project. Review r2
            // F3: the narrowing covers the core catalog too, so no
            // enumeration or authorization path can widen past it.
            if let Some(selected) = &project {
                if !catalog.core.admits_project(selected) {
                    return DomainResult::invalid(lekalo_core::mcp::diagnostic::scope_denied_set(
                        "project",
                        "unregistered",
                    ));
                }
                let keep = selected.clone();
                catalog.core = catalog.core.narrowed(&keep);
                catalog.projects.retain(|id, _| *id == keep);
            }
            catalog
        }
        (None, Some(root)) => {
            // Single-project startup: one admitted project, no groups, no
            // manifest grants, no changed-mode Git admission. The operand
            // spelling stays the loader-facing selector: the core
            // selection policy still applies to every load, so a spelling
            // it refuses surfaces its own registered diagnostic on the
            // first tool call instead of a widened MCP grant.
            let resolved = match resolve_root(root) {
                Ok(resolved) => resolved,
                Err(result) => return result,
            };
            catalog::ServerCatalog::single(root, &resolved)
        }
        (None, None) => {
            // `serve` requires explicit project selection.
            return DomainResult::usage_error();
        }
    };
    // A --group must already be defined in the trusted catalog for the
    // served project set; it narrows, never invents.
    if let Some(group) = &group {
        let admitted = catalog.core.projects().any(|project| {
            catalog.core.admits_group(project, group)
                == lekalo_core::mcp::policy::ScopeDecision::Admitted
        });
        if !admitted {
            return DomainResult::invalid(lekalo_core::mcp::diagnostic::scope_denied_set(
                "group",
                "unregistered",
            ));
        }
    }
    let mut limits = catalog.limits;
    limits.timeout_ms =
        lekalo_core::mcp::policy::Limits::effective_timeout_ms(timeout_ms, limits.timeout_ms);
    limits.max_response_bytes = lekalo_core::mcp::policy::Limits::effective_max_response_bytes(
        max_response_bytes,
        limits.max_response_bytes,
    );
    // The group flag rides every tool request as the default scope only
    // when the caller supplied one; the session serves exactly the
    // catalog the launcher granted.
    let session = std::sync::Arc::new(dispatch::Session::new(
        catalog,
        limits,
        group,
        stdio::Writer::new(Box::new(std::io::stdout()), limits.max_response_bytes),
    ));
    dispatch::serve(session);
    DomainResult::receipt(
        "{\"served\":true}".to_owned(),
        "mcp session ended".to_owned(),
    )
}

/// Resolve the granted root once at startup; the alias-free spelling is
/// required by the selection policy (GitHub's Windows runners export the
/// 8.3 profile alias).
fn resolve_root(root: &str) -> Result<std::path::PathBuf, DomainResult> {
    let path = std::path::Path::new(root);
    let canonical = path.canonicalize().map_err(|_| {
        DomainResult::invalid(lekalo_core::mcp::diagnostic::scope_denied_set(
            "project-root",
            "unresolved",
        ))
    })?;
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => {
            return Ok(std::path::PathBuf::from(rest));
        }
        _ => {}
    }
    Ok(canonical)
}
