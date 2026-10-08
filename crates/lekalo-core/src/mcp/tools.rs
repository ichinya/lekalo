//! The exact issue #33 tool catalog and the closed request grammar of the
//! seven read tools.
//!
//! The catalog is a fixed, deterministic table: five resource families and
//! seven P1 tool names, no write/generate/collect/install/scan/report
//! capability anywhere in the dispatch surface. Request parsing is pure:
//! structural violations (missing, unknown, wrong-typed, out-of-range
//! members) are JSON-RPC -32602 material; cross-field semantic invariants
//! are the typed `mcp.input-invalid` refusal. Values are never echoed.

use serde_json::Value;

use super::policy;
use super::version::TIMEOUT_MS_CEILING;

/// The exact seven read tool names, in catalog order.
pub const TOOL_NAMES: [&str; 7] = [
    "lekalo_inspect",
    "lekalo_impact",
    "lekalo_context",
    "lekalo_graph",
    "lekalo_validate",
    "lekalo_target_status",
    "lekalo_trace_export",
];

/// One descriptor of the fixed catalog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ToolDescriptor {
    /// The exact wire name (`lekalo_inspect`).
    pub name: &'static str,
    /// The stable human title.
    pub title: &'static str,
    /// The tool slug inside the contract identities (`inspect`).
    pub slug: &'static str,
}

/// The fixed, deterministic tool catalog; no dynamic registration exists.
pub const CATALOG: [ToolDescriptor; 7] = [
    ToolDescriptor {
        name: "lekalo_inspect",
        title: "Lekalo inspect",
        slug: "inspect",
    },
    ToolDescriptor {
        name: "lekalo_impact",
        title: "Lekalo impact",
        slug: "impact",
    },
    ToolDescriptor {
        name: "lekalo_context",
        title: "Lekalo context",
        slug: "context",
    },
    ToolDescriptor {
        name: "lekalo_graph",
        title: "Lekalo graph",
        slug: "graph",
    },
    ToolDescriptor {
        name: "lekalo_validate",
        title: "Lekalo validate",
        slug: "validate",
    },
    ToolDescriptor {
        name: "lekalo_target_status",
        title: "Lekalo target status",
        slug: "target-status",
    },
    ToolDescriptor {
        name: "lekalo_trace_export",
        title: "Lekalo trace export",
        slug: "trace-export",
    },
];

/// Look up one descriptor by exact tool name.
pub fn descriptor(name: &str) -> Option<ToolDescriptor> {
    CATALOG.iter().copied().find(|tool| tool.name == name)
}

/// The optional inspect projections.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum Include {
    Bindings,
    Scenarios,
}

impl Include {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "bindings" => Some(Self::Bindings),
            "scenarios" => Some(Self::Scenarios),
            _ => None,
        }
    }

    /// The CLI `--include` spelling, for typed reuse of the core seam.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bindings => "bindings",
            Self::Scenarios => "scenarios",
        }
    }
}

/// Why one request failed the structural part of its grammar
/// (JSON-RPC -32602; the field name is closed, the value never echoes).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParamsError {
    /// The closed name of the offending member.
    pub field: &'static str,
}

/// Why one request failed a cross-field semantic invariant
/// (typed `mcp.input-invalid`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SemanticError {
    /// The closed name of the offending member.
    pub field: &'static str,
    /// The closed machine reason.
    pub reason: &'static str,
}

/// The common members of every tool request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Common {
    pub project_id: String,
    pub group_id: Option<String>,
    pub timeout_ms: Option<u64>,
}

/// A fully validated request of one of the seven read tools. Operands are
/// semantic ids and enumerated selectors only: no root path, executable
/// name, argv, environment variable, export destination, or write flag
/// can be expressed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolRequest {
    Inspect {
        common: Common,
        symbol: String,
        include: Vec<Include>,
    },
    Impact {
        common: Common,
        mode: ImpactMode,
        symbol: Option<String>,
        base_ref: Option<String>,
        head_ref: Option<String>,
        worktree: bool,
        depth: Option<u16>,
        module: Option<String>,
        target: Option<String>,
        relations: Vec<String>,
        profile: Option<String>,
    },
    Context {
        common: Common,
        mode: ContextMode,
        symbol: Option<String>,
        changed_symbols: Vec<String>,
        /// The exact changed-symbol count the caller declared (and the
        /// dispatcher re-verified); the seam re-checks it at dispatch so
        /// a comma-split downstream cannot widen the evaluated set.
        changed_symbol_count: u64,
        budget: u64,
        spans: bool,
    },
    Graph {
        common: Common,
        mode: GraphMode,
        symbol: Option<String>,
        from: Option<String>,
        to: Option<String>,
        transitive: bool,
    },
    Validate {
        common: Common,
        module: Option<String>,
        strict: bool,
    },
    TargetStatus {
        common: Common,
        target_id: Option<String>,
        profile_id: Option<String>,
    },
    TraceExport {
        common: Common,
        manifest_ref: String,
    },
}

/// The impact operand mode: symbol and changed modes need distinct
/// validated operands; a Git selector is not a semantic-id list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImpactMode {
    Symbol,
    Changed,
}

/// The context operand mode: changed ids are a typed handoff, not a Git
/// mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextMode {
    Symbol,
    Changed,
}

/// The graph query mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphMode {
    Show,
    Callers,
    Path,
    Export,
}

impl ToolRequest {
    /// The common members of this request.
    pub fn common(&self) -> &Common {
        match self {
            Self::Inspect { common, .. }
            | Self::Impact { common, .. }
            | Self::Context { common, .. }
            | Self::Graph { common, .. }
            | Self::Validate { common, .. }
            | Self::TargetStatus { common, .. }
            | Self::TraceExport { common, .. } => common,
        }
    }

    /// Apply the trusted launcher group as the disclosure ceiling. A
    /// client-supplied `groupId` is admitted only when it names exactly
    /// the launch group; any other spelling is a widening refusal, never
    /// a silent substitution (the closure then runs against the effective
    /// group, which is always the launch group or nothing when unset).
    pub fn apply_launch_group(&mut self, launch_group: Option<&str>) -> Result<(), SemanticError> {
        let Some(launch_group) = launch_group else {
            return Ok(());
        };
        let common = match self {
            Self::Inspect { common, .. }
            | Self::Impact { common, .. }
            | Self::Context { common, .. }
            | Self::Graph { common, .. }
            | Self::Validate { common, .. }
            | Self::TargetStatus { common, .. }
            | Self::TraceExport { common, .. } => common,
        };
        match &common.group_id {
            None => {
                common.group_id = Some(launch_group.to_owned());
                Ok(())
            }
            Some(requested) if requested == launch_group => Ok(()),
            Some(_) => Err(SemanticError {
                field: "groupId",
                reason: "widen-past-launch-group",
            }),
        }
    }

    /// The exact tool name of this request.
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::Inspect { .. } => "lekalo_inspect",
            Self::Impact { .. } => "lekalo_impact",
            Self::Context { .. } => "lekalo_context",
            Self::Graph { .. } => "lekalo_graph",
            Self::Validate { .. } => "lekalo_validate",
            Self::TargetStatus { .. } => "lekalo_target_status",
            Self::TraceExport { .. } => "lekalo_trace_export",
        }
    }
}

/// The closed member vocabulary of each tool beyond the common three.
fn known_members(tool: &str) -> &'static [&'static str] {
    match tool {
        "lekalo_inspect" => &["symbol", "include"],
        "lekalo_impact" => &[
            "mode",
            "symbol",
            "baseRef",
            "headRef",
            "worktree",
            "depth",
            "module",
            "target",
            "relations",
            "profile",
        ],
        "lekalo_context" => &[
            "mode",
            "symbol",
            "changedSymbols",
            "budget",
            "spans",
            "changedSymbolCount",
        ],
        "lekalo_graph" => &["mode", "symbol", "from", "to", "transitive"],
        "lekalo_validate" => &["module", "strict"],
        "lekalo_target_status" => &["targetId", "profileId"],
        "lekalo_trace_export" => &["manifestRef"],
        _ => &[],
    }
}

/// Parse and validate the `arguments` object of one known tool.
///
/// Structural violations return [`ParamsError`]; cross-field semantic
/// violations return [`SemanticError`]. Symbol/selector values are
/// grammar-checked as bounded semantic ids; no operand can carry a path,
/// argv, or environment spelling past this boundary.
pub fn parse_request(tool: &str, arguments: &Value) -> Result<ToolRequest, RequestError> {
    let Value::Object(arguments) = arguments else {
        return Err(RequestError::Params(ParamsError { field: "arguments" }));
    };
    // Closed member vocabulary: every key must be one of the common
    // three or the tool's own members; a smuggled write/destination
    // parameter is a structural refusal before any semantic work.
    for key in arguments.keys() {
        if !matches!(key.as_str(), "projectId" | "groupId" | "timeoutMs")
            && !known_members(tool).contains(&key.as_str())
        {
            return Err(RequestError::Params(ParamsError { field: "arguments" }));
        }
    }
    let project_id = arguments
        .get("projectId")
        .and_then(Value::as_str)
        .ok_or(RequestError::Params(ParamsError { field: "projectId" }))?
        .to_owned();
    let group_id = match arguments.get("groupId") {
        None => None,
        Some(Value::String(group)) => Some(group.clone()),
        Some(_) => return Err(RequestError::Params(ParamsError { field: "groupId" })),
    };
    let timeout_ms = match arguments.get("timeoutMs") {
        None => None,
        Some(value) => Some(
            value
                .as_u64()
                .ok_or(RequestError::Params(ParamsError { field: "timeoutMs" }))?,
        ),
    };
    if let Err(reason) = policy::admit_id(&project_id) {
        let _ = reason;
        return Err(RequestError::Params(ParamsError { field: "projectId" }));
    }
    if let Some(group) = &group_id {
        if policy::admit_id(group).is_err() {
            return Err(RequestError::Params(ParamsError { field: "groupId" }));
        }
    }
    if let Some(timeout) = timeout_ms {
        if timeout == 0 || timeout > TIMEOUT_MS_CEILING {
            return Err(RequestError::Params(ParamsError { field: "timeoutMs" }));
        }
    }
    let common = Common {
        project_id,
        group_id,
        timeout_ms,
    };
    let field = |name: &'static str| RequestError::Params(ParamsError { field: name });
    let semantic = |field: &'static str, reason: &'static str| {
        RequestError::Semantic(SemanticError { field, reason })
    };
    let string_field = |arguments: &serde_json::Map<String, Value>, name: &str| -> Option<String> {
        arguments
            .get(name)
            .map(|value| value.as_str().map(str::to_owned))
            .unwrap_or(None)
    };
    let bool_field = |arguments: &serde_json::Map<String, Value>, name: &str| -> Option<bool> {
        arguments
            .get(name)
            .map(|value| value.as_bool())
            .unwrap_or(None)
    };
    match tool {
        "lekalo_inspect" => {
            let symbol = string_field(arguments, "symbol").ok_or(field("symbol"))?;
            if symbol.is_empty() || symbol.len() > 192 {
                return Err(field("symbol"));
            }
            // A comma is the CLI's changed-list separator, never part of a
            // semantic id: refusing it here keeps one operand from
            // smuggle-splitting into a set the closure never checked.
            if symbol.contains(',') {
                return Err(field("symbol"));
            }
            let include = match arguments.get("include") {
                None => Vec::new(),
                Some(Value::Array(items)) => {
                    let mut parsed = Vec::new();
                    for item in items {
                        let Value::String(text) = item else {
                            return Err(field("include"));
                        };
                        let include = Include::parse(text).ok_or(field("include"))?;
                        if parsed.contains(&include) {
                            return Err(field("include"));
                        }
                        parsed.push(include);
                    }
                    parsed
                }
                Some(_) => return Err(field("include")),
            };
            Ok(ToolRequest::Inspect {
                common,
                symbol,
                include,
            })
        }
        "lekalo_impact" => {
            let mode = string_field(arguments, "mode").ok_or(field("mode"))?;
            let mode = match mode.as_str() {
                "symbol" => ImpactMode::Symbol,
                "changed" => ImpactMode::Changed,
                _ => return Err(field("mode")),
            };
            let symbol = match arguments.get("symbol") {
                None => None,
                Some(Value::String(symbol)) => {
                    if symbol.is_empty() || symbol.len() > 192 || symbol.contains(',') {
                        return Err(field("symbol"));
                    }
                    Some(symbol.clone())
                }
                Some(_) => return Err(field("symbol")),
            };
            let base_ref = match arguments.get("baseRef") {
                None => None,
                Some(Value::String(text)) => {
                    if text.is_empty() || text.len() > 128 {
                        return Err(field("baseRef"));
                    }
                    Some(text.clone())
                }
                Some(_) => return Err(field("baseRef")),
            };
            let head_ref = match arguments.get("headRef") {
                None => None,
                Some(Value::String(text)) => {
                    if text.is_empty() || text.len() > 128 {
                        return Err(field("headRef"));
                    }
                    Some(text.clone())
                }
                Some(_) => return Err(field("headRef")),
            };
            let worktree = bool_field(arguments, "worktree").unwrap_or(false);
            let depth = match arguments.get("depth") {
                None => None,
                Some(value) => {
                    let depth = value.as_u64().ok_or(field("depth"))?;
                    if depth == 0 || depth > 256 {
                        return Err(field("depth"));
                    }
                    Some(depth as u16)
                }
            };
            let module = match arguments.get("module") {
                None => None,
                Some(Value::String(module)) => {
                    if policy::admit_module_id(module).is_err() {
                        return Err(field("module"));
                    }
                    Some(module.clone())
                }
                Some(_) => return Err(field("module")),
            };
            let relations = match arguments.get("relations") {
                None => Vec::new(),
                Some(Value::Array(items)) => {
                    let mut parsed = Vec::new();
                    for item in items {
                        let Value::String(relation) = item else {
                            return Err(field("relations"));
                        };
                        if relation.is_empty() || relation.len() > 32 {
                            return Err(field("relations"));
                        }
                        if parsed.contains(relation) {
                            return Err(field("relations"));
                        }
                        parsed.push(relation.clone());
                    }
                    parsed
                }
                Some(_) => return Err(field("relations")),
            };
            let profile = match arguments.get("profile") {
                None => None,
                Some(Value::String(profile)) => {
                    if profile.is_empty() || profile.len() > 32 {
                        return Err(field("profile"));
                    }
                    Some(profile.clone())
                }
                Some(_) => return Err(field("profile")),
            };
            let target = match arguments.get("target") {
                None => None,
                Some(Value::String(target)) => {
                    if policy::admit_module_id(target).is_err() {
                        return Err(field("target"));
                    }
                    Some(target.clone())
                }
                Some(_) => return Err(field("target")),
            };
            // Cross-field semantics: symbol mode requires `symbol` and
            // forbids Git operands; changed mode requires `baseRef` and
            // exactly one of `headRef`/`worktree`.
            match mode {
                ImpactMode::Symbol => {
                    if base_ref.is_some() || head_ref.is_some() || worktree {
                        return Err(semantic("mode", "selector-requires-changed"));
                    }
                    if symbol.is_none() {
                        return Err(semantic("symbol", "required-for-symbol-mode"));
                    }
                }
                ImpactMode::Changed => {
                    if symbol.is_some() {
                        return Err(semantic("mode", "symbol-forbidden-in-changed"));
                    }
                    if base_ref.is_none() {
                        return Err(semantic("baseRef", "required-for-changed"));
                    }
                    if head_ref.is_some() == worktree {
                        return Err(semantic("mode", "exactly-one-head-or-worktree"));
                    }
                }
            }
            Ok(ToolRequest::Impact {
                common,
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
            })
        }
        "lekalo_context" => {
            let mode = string_field(arguments, "mode").ok_or(field("mode"))?;
            let mode = match mode.as_str() {
                "symbol" => ContextMode::Symbol,
                "changed" => ContextMode::Changed,
                _ => return Err(field("mode")),
            };
            let budget = arguments
                .get("budget")
                .and_then(|value| value.as_u64())
                .ok_or(field("budget"))?;
            if budget == 0 {
                return Err(field("budget"));
            }
            let symbol = match arguments.get("symbol") {
                None => None,
                Some(Value::String(symbol)) => {
                    if symbol.is_empty() || symbol.len() > 192 {
                        return Err(field("symbol"));
                    }
                    Some(symbol.clone())
                }
                Some(_) => return Err(field("symbol")),
            };
            let changed_symbols = match arguments.get("changedSymbols") {
                None => Vec::new(),
                Some(Value::Array(items)) => {
                    if items.is_empty() || items.len() > 64 {
                        return Err(field("changedSymbols"));
                    }
                    let mut parsed = Vec::new();
                    for item in items {
                        let Value::String(symbol) = item else {
                            return Err(field("changedSymbols"));
                        };
                        // A comma is the CLI's list separator; a member
                        // carrying one would re-split downstream into a
                        // symbol set the closure never checked.
                        if symbol.is_empty()
                            || symbol.len() > 192
                            || symbol.contains(',')
                            || parsed.contains(symbol)
                        {
                            return Err(field("changedSymbols"));
                        }
                        parsed.push(symbol.clone());
                    }
                    parsed
                }
                Some(_) => return Err(field("changedSymbols")),
            };
            let spans = bool_field(arguments, "spans").unwrap_or(false);
            let changed_symbol_count = match arguments.get("changedSymbolCount") {
                None => changed_symbols.len() as u64,
                Some(value) => {
                    let count = value.as_u64().ok_or(field("changedSymbolCount"))?;
                    if count != changed_symbols.len() as u64 {
                        return Err(field("changedSymbolCount"));
                    }
                    count
                }
            };
            let _ = changed_symbol_count;
            let changed_symbols_len = changed_symbols.len() as u64;
            match mode {
                ContextMode::Symbol if symbol.is_none() => {
                    return Err(semantic("symbol", "required-for-symbol-mode"))
                }
                ContextMode::Symbol if !changed_symbols.is_empty() => {
                    return Err(semantic("mode", "changed-symbols-forbid-symbol-mode"))
                }
                ContextMode::Changed if symbol.is_some() => {
                    return Err(semantic("mode", "symbol-forbidden-in-changed"))
                }
                ContextMode::Changed if changed_symbols.is_empty() => {
                    return Err(semantic("changedSymbols", "required-for-changed"))
                }
                _ => {}
            }
            Ok(ToolRequest::Context {
                common,
                mode,
                symbol,
                changed_symbols,
                changed_symbol_count: changed_symbols_len,
                budget,
                spans,
            })
        }
        "lekalo_graph" => {
            let mode = string_field(arguments, "mode").ok_or(field("mode"))?;
            let mode = match mode.as_str() {
                "show" => GraphMode::Show,
                "callers" => GraphMode::Callers,
                "path" => GraphMode::Path,
                "export" => GraphMode::Export,
                _ => return Err(field("mode")),
            };
            let symbol = match arguments.get("symbol") {
                None => None,
                Some(Value::String(symbol)) => {
                    if symbol.is_empty() || symbol.len() > 192 {
                        return Err(field("symbol"));
                    }
                    Some(symbol.clone())
                }
                Some(_) => return Err(field("symbol")),
            };
            let from = match arguments.get("from") {
                None => None,
                Some(Value::String(from)) => {
                    if from.is_empty() || from.len() > 192 {
                        return Err(field("from"));
                    }
                    Some(from.clone())
                }
                Some(_) => return Err(field("from")),
            };
            let to = match arguments.get("to") {
                None => None,
                Some(Value::String(to)) => {
                    if to.is_empty() || to.len() > 192 {
                        return Err(field("to"));
                    }
                    Some(to.clone())
                }
                Some(_) => return Err(field("to")),
            };
            let transitive = bool_field(arguments, "transitive").unwrap_or(false);
            match mode {
                GraphMode::Show | GraphMode::Callers if symbol.is_none() => {
                    return Err(semantic("symbol", "required-for-mode"))
                }
                GraphMode::Path if from.is_none() || to.is_none() => {
                    return Err(semantic("from", "path-requires-from-and-to"))
                }
                _ => {}
            }
            Ok(ToolRequest::Graph {
                common,
                mode,
                symbol,
                from,
                to,
                transitive,
            })
        }
        "lekalo_validate" => {
            let module = match arguments.get("module") {
                None => None,
                Some(Value::String(module)) => {
                    if policy::admit_module_id(module).is_err() {
                        return Err(field("module"));
                    }
                    Some(module.clone())
                }
                Some(_) => return Err(field("module")),
            };
            let strict = bool_field(arguments, "strict").unwrap_or(false);
            Ok(ToolRequest::Validate {
                common,
                module,
                strict,
            })
        }
        "lekalo_target_status" => {
            let id_field =
                |arguments: &serde_json::Map<String, Value>, name: &str| -> Option<String> {
                    match arguments.get(name) {
                        None => None,
                        Some(Value::String(text)) => {
                            if policy::admit_id(text).is_err() {
                                return None;
                            }
                            Some(text.clone())
                        }
                        Some(_) => None,
                    }
                };
            let target_id = id_field(arguments, "targetId");
            let profile_id = id_field(arguments, "profileId");
            if arguments.contains_key("targetId") && target_id.is_none() {
                return Err(field("targetId"));
            }
            if arguments.contains_key("profileId") && profile_id.is_none() {
                return Err(field("profileId"));
            }
            Ok(ToolRequest::TargetStatus {
                common,
                target_id,
                profile_id,
            })
        }
        "lekalo_trace_export" => {
            let manifest_ref =
                string_field(arguments, "manifestRef").ok_or(field("manifestRef"))?;
            if policy::admit_id(&manifest_ref).is_err() {
                return Err(field("manifestRef"));
            }
            Ok(ToolRequest::TraceExport {
                common,
                manifest_ref,
            })
        }
        _ => Err(RequestError::Params(ParamsError { field: "tool" })),
    }
}

/// The two failure classes of request admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestError {
    /// JSON-RPC -32602: structural parameter violation.
    Params(ParamsError),
    /// Typed `mcp.input-invalid`: semantic invariant violation.
    Semantic(SemanticError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn catalog_is_the_exact_seven_read_tools() {
        assert_eq!(TOOL_NAMES.len(), 7);
        assert_eq!(CATALOG.len(), 7);
        for (index, tool) in CATALOG.iter().enumerate() {
            assert_eq!(tool.name, TOOL_NAMES[index]);
            assert_eq!(descriptor(tool.name), Some(*tool));
        }
        assert!(descriptor("lekalo_generate").is_none());
        assert!(descriptor("lekalo_write").is_none());
        assert!(descriptor("lekalo_migrate").is_none());
        assert!(descriptor("lekalo_scan").is_none());
        assert!(descriptor("lekalo_inspect").is_some());
    }

    #[test]
    fn inspect_parses_the_closed_grammar() {
        let request = parse_request(
            "lekalo_inspect",
            &json!({"projectId": "planner", "symbol": "planner.focus_task", "include": ["bindings", "scenarios"]}),
        )
        .expect("request");
        let ToolRequest::Inspect {
            common,
            symbol,
            include,
        } = request
        else {
            panic!("variant")
        };
        assert_eq!(common.project_id, "planner");
        assert_eq!(common.group_id, None);
        assert_eq!(symbol, "planner.focus_task");
        assert_eq!(include, vec![Include::Bindings, Include::Scenarios]);
    }

    #[test]
    fn structural_violations_are_params_errors() {
        assert_eq!(
            parse_request("lekalo_inspect", &json!({"projectId": "planner"})),
            Err(RequestError::Params(ParamsError { field: "symbol" }))
        );
        assert_eq!(
            parse_request(
                "lekalo_inspect",
                &json!({"projectId": "planner", "symbol": "x", "extra": 1})
            ),
            Err(RequestError::Params(ParamsError { field: "arguments" }))
        );
        assert_eq!(
            parse_request(
                "lekalo_inspect",
                &json!({"projectId": "planner", "symbol": "x", "include": ["nope"]})
            ),
            Err(RequestError::Params(ParamsError { field: "include" }))
        );
        assert_eq!(
            parse_request(
                "lekalo_inspect",
                &json!({"projectId": "../etc", "symbol": "x"})
            ),
            Err(RequestError::Params(ParamsError { field: "projectId" }))
        );
        assert_eq!(
            parse_request("lekalo_inspect", &json!([1, 2])),
            Err(RequestError::Params(ParamsError { field: "arguments" }))
        );
    }

    #[test]
    fn impact_modes_need_distinct_validated_operands() {
        let symbol_request = parse_request(
            "lekalo_impact",
            &json!({"projectId": "p", "mode": "symbol", "symbol": "planner.focus_task"}),
        )
        .expect("symbol request");
        assert!(matches!(
            symbol_request,
            ToolRequest::Impact {
                mode: ImpactMode::Symbol,
                ..
            }
        ));

        let changed_request = parse_request(
            "lekalo_impact",
            &json!({"projectId": "p", "mode": "changed", "baseRef": "main", "worktree": true}),
        )
        .expect("changed request");
        assert!(matches!(
            changed_request,
            ToolRequest::Impact {
                mode: ImpactMode::Changed,
                ..
            }
        ));

        // Symbol mode must not carry Git operands.
        assert!(matches!(
            parse_request(
                "lekalo_impact",
                &json!({"projectId": "p", "mode": "symbol", "symbol": "s", "baseRef": "main"})
            ),
            Err(RequestError::Semantic(SemanticError {
                reason: "selector-requires-changed",
                ..
            }))
        ));
        // Changed mode requires baseRef and exactly one head/worktree.
        assert!(matches!(
            parse_request(
                "lekalo_impact",
                &json!({"projectId": "p", "mode": "changed", "symbol": "s"})
            ),
            Err(RequestError::Semantic(SemanticError {
                reason: "symbol-forbidden-in-changed",
                ..
            }))
        ));
        assert!(matches!(
            parse_request(
                "lekalo_impact",
                &json!({"projectId": "p", "mode": "changed"})
            ),
            Err(RequestError::Semantic(SemanticError {
                reason: "required-for-changed",
                ..
            }))
        ));
        assert!(matches!(
            parse_request(
                "lekalo_impact",
                &json!({"projectId": "p", "mode": "changed", "baseRef": "b", "headRef": "h", "worktree": true})
            ),
            Err(RequestError::Semantic(SemanticError {
                reason: "exactly-one-head-or-worktree",
                ..
            }))
        ));
    }

    #[test]
    fn context_needs_exactly_one_operand_shape() {
        let request = parse_request(
            "lekalo_context",
            &json!({"projectId": "p", "mode": "changed", "changedSymbols": ["a", "b"], "budget": 12000}),
        )
        .expect("changed request");
        let ToolRequest::Context {
            changed_symbols,
            budget,
            ..
        } = request
        else {
            panic!("variant")
        };
        assert_eq!(changed_symbols, vec!["a", "b"]);
        assert_eq!(budget, 12_000);
        assert!(matches!(
            parse_request(
                "lekalo_context",
                &json!({"projectId": "p", "mode": "symbol", "budget": 100})
            ),
            Err(RequestError::Semantic(SemanticError {
                reason: "required-for-symbol-mode",
                ..
            }))
        ));
        assert!(matches!(
            parse_request(
                "lekalo_context",
                &json!({"projectId": "p", "mode": "symbol", "symbol": "a", "changedSymbols": ["b"], "budget": 100})
            ),
            Err(RequestError::Semantic(SemanticError {
                reason: "changed-symbols-forbid-symbol-mode",
                ..
            }))
        ));
        assert!(matches!(
            parse_request(
                "lekalo_context",
                &json!({"projectId": "p", "mode": "changed", "changedSymbols": [], "budget": 100})
            ),
            Err(RequestError::Params(ParamsError {
                field: "changedSymbols"
            }))
        ));
    }

    #[test]
    fn graph_and_trace_operands_are_semantic_ids_only() {
        assert!(matches!(
            parse_request(
                "lekalo_graph",
                &json!({"projectId": "p", "mode": "path", "from": "a"})
            ),
            Err(RequestError::Semantic(SemanticError {
                reason: "path-requires-from-and-to",
                ..
            }))
        ));
        let export = parse_request("lekalo_graph", &json!({"projectId": "p", "mode": "export"}))
            .expect("export");
        assert!(matches!(
            export,
            ToolRequest::Graph {
                mode: GraphMode::Export,
                ..
            }
        ));
        assert!(matches!(
            parse_request(
                "lekalo_trace_export",
                &json!({"projectId": "p", "manifestRef": "/abs/path"})
            ),
            Err(RequestError::Params(ParamsError {
                field: "manifestRef"
            }))
        ));
    }

    #[test]
    fn no_write_capability_can_be_expressed() {
        // Unknown tools are refused by the dispatcher before parsing; the
        // parser itself has no arm for any non-catalog name.
        assert!(matches!(
            parse_request("lekalo_generate", &json!({"projectId": "p"})),
            Err(RequestError::Params(ParamsError { field: "tool" }))
        ));
        // No write/export/collect flag exists on any arm.
        for (tool, arguments) in [
            ("lekalo_inspect", json!({"projectId": "p", "symbol": "s"})),
            (
                "lekalo_impact",
                json!({"projectId": "p", "mode": "symbol", "symbol": "s"}),
            ),
            (
                "lekalo_context",
                json!({"projectId": "p", "mode": "symbol", "symbol": "s", "budget": 1}),
            ),
            ("lekalo_graph", json!({"projectId": "p", "mode": "export"})),
            ("lekalo_validate", json!({"projectId": "p"})),
            ("lekalo_target_status", json!({"projectId": "p"})),
            (
                "lekalo_trace_export",
                json!({"projectId": "p", "manifestRef": "m"}),
            ),
        ] {
            let request = parse_request(tool, &arguments).expect(tool);
            assert!(request.tool_name().starts_with("lekalo_"));
        }
    }
}
