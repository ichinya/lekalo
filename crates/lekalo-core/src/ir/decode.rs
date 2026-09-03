//! Closed typed decoding of the loader aggregate into IR (issue #8).
//!
//! Every definition node is decoded against the exact key set, grammar,
//! cardinality, and closed-enum surface of the accepted Model contracts for
//! the active source version. Unknown keys fail closed; no key is ever
//! ignored, and no target-specific or speculative node exists. Decoding is
//! pure: it reads the spanned trees it is given and records a source-map
//! entry for every definition, field, reference, and registry position it
//! accepts.
//!
//! Source-map coverage: one entry per definition (carrying its semantic
//! ID), one per accepted mapping key, one per array member, and one per
//! type-expression node (nested wrappers at suffixed pointers). Type,
//! `returns`, `entity`, and `invokes` positions record their value node
//! instead of the bare key so each position yields the more precise span.

use super::diagnostic::{
    bounded_field_echo, IrFailure, DUPLICATE_MEMBER, KIND_PLACEMENT, KIND_UNKNOWN, MISSING_FIELD,
    UNKNOWN_FIELD, VALUE_INVALID,
};
use super::grammar::{
    is_endpoint_path, is_field_name, is_project_id, is_requirement_id, is_segment, is_symbol_id,
    is_target_name,
};
use super::{
    CommandDef, Common, Compilation, CompiledProject, Decision, Definition, EffectDef,
    EffectOperation, EndpointDef, EndpointPath, EntityDef, EnumDef, EnumValue, EnumValueName,
    EventDef, Field, FieldName, HttpMethod, IdRegistry, Module, ModuleId, PolicyDef, Portability,
    Project, ProjectId, QueryDef, RenameHistoryEntry, RequirementId, ScalarBase, ScalarDef,
    ScenarioDef, SourceMap, SymbolId, TargetBindingDef, TargetName, Text, Tombstone, TypeRef,
    ValueObjectDef, Visibility, MAX_TYPE_LEVELS,
};
use crate::loader::error::{Diagnostic, Span};
use crate::loader::frontends::{MapEntry, Node, Scalar, Value};
use crate::loader::{
    ModelVersion, NormalizedDefinition, NormalizedModel, Position, SourceMapEntry,
};

/// The maximum Unicode scalars of one bounded human-readable text value.
const MAX_TEXT_CHARS: usize = 2000;

/// Decode one whole model; any diagnostic fails the entire compilation.
pub(super) fn compile(model: &NormalizedModel) -> Result<Compilation, IrFailure> {
    let mut cx = Context {
        version: model.model_version,
        path: String::new(),
        diagnostics: Vec::new(),
        entries: Vec::new(),
    };

    let project = match &model.project {
        Some(definition) => decode_project(definition, "/project", &mut cx),
        None => None,
    };

    let mut modules = Vec::with_capacity(model.modules.len());
    for (index, normalized) in model.modules.iter().enumerate() {
        if IrFailure::is_full(&cx.diagnostics) {
            break;
        }
        if let Some(module) = decode_module(normalized, &format!("/modules/{index}"), &mut cx) {
            modules.push(module);
        }
    }

    let mut definitions = Vec::with_capacity(model.definitions.len());
    for (index, normalized) in model.definitions.iter().enumerate() {
        if IrFailure::is_full(&cx.diagnostics) {
            break;
        }
        if let Some(definition) =
            decode_symbol(normalized, &format!("/definitions/{index}"), &mut cx)
        {
            definitions.push(definition);
        }
    }

    if !cx.diagnostics.is_empty() {
        return Err(IrFailure::new(cx.diagnostics));
    }

    modules.sort_by(|left, right| {
        left.id
            .as_str()
            .as_bytes()
            .cmp(right.id.as_str().as_bytes())
    });
    definitions.sort_by(|left, right| {
        left.id()
            .as_str()
            .as_bytes()
            .cmp(right.id().as_str().as_bytes())
    });

    Ok(Compilation {
        project: CompiledProject {
            model_version: model.model_version,
            project,
            modules,
            definitions,
        },
        source_map: SourceMap::new(cx.entries),
    })
}

/// The document home of one definition, derived from its logical path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Home {
    Project,
    Module,
    Entities,
    Commands,
    Queries,
    Policies,
    Events,
    Scenarios,
    Bindings,
}

fn home_of(path: &str) -> Option<Home> {
    if path == "lekalo/project.yaml" {
        return Some(Home::Project);
    }
    if path.starts_with("lekalo/modules/") && path.ends_with("/module.yaml") {
        return Some(Home::Module);
    }
    match path.rsplit('/').next().unwrap_or_default() {
        "entities.yaml" => Some(Home::Entities),
        "commands.yaml" => Some(Home::Commands),
        "queries.yaml" => Some(Home::Queries),
        "policies.yaml" => Some(Home::Policies),
        "events.yaml" => Some(Home::Events),
        "scenarios.yaml" => Some(Home::Scenarios),
        "bindings.yaml" => Some(Home::Bindings),
        _ => None,
    }
}

fn home_accepts(home: Home, kind: &str) -> bool {
    match home {
        Home::Project => kind == "project",
        Home::Module => kind == "module",
        Home::Entities => matches!(kind, "scalar" | "enum" | "value-object" | "entity"),
        Home::Commands => matches!(kind, "command" | "effect"),
        Home::Queries => kind == "query",
        Home::Policies => kind == "policy",
        Home::Events => kind == "event",
        Home::Scenarios => kind == "scenario",
        Home::Bindings => matches!(kind, "endpoint" | "target-binding"),
    }
}

/// Escape one dynamic token for JSON Pointer (RFC 6901).
fn pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// Keys whose source-map entry is recorded by their dedicated value decoder.
const VALUE_DECODED_KEYS: &[&str] = &["type", "returns", "entity", "invokes"];

/// Decode state threaded through every decoder.
struct Context {
    version: ModelVersion,
    path: String,
    diagnostics: Vec<Diagnostic>,
    entries: Vec<SourceMapEntry>,
}

impl Context {
    fn push(&mut self, code: &str, span: Span, data: serde_json::Value) {
        if IrFailure::is_full(&self.diagnostics) {
            return;
        }
        let mut diagnostic = Diagnostic::new(code).with_span(span).with_data(data);
        if !self.path.is_empty() {
            diagnostic = diagnostic.with_path(self.path.clone());
        }
        self.diagnostics.push(diagnostic);
    }

    fn record(&mut self, pointer: &str, span: Span, semantic_id: Option<&str>) {
        self.entries.push(SourceMapEntry {
            path: self.path.clone(),
            semantic_id: semantic_id.map(|id| id.to_owned()),
            pointer: pointer.to_owned(),
            start: Position {
                byte: span.start.byte,
                line: span.start.line,
                column: span.start.column,
            },
            end: Position {
                byte: span.end.byte,
                line: span.end.line,
                column: span.end.column,
            },
        });
    }

    /// Record every mapping key of `node` that this decoder accepts and
    /// return them in document order; unknown keys fail closed.
    fn scan<'a>(
        &mut self,
        node: &'a Node,
        base_pointer: &str,
        allowed: &[&str],
    ) -> Option<Vec<(&'a str, &'a MapEntry)>> {
        let entries = match &node.value {
            Value::Map(entries) => entries,
            _ => {
                self.push(
                    VALUE_INVALID,
                    node.span,
                    serde_json::json!({ "detail": "not-a-mapping" }),
                );
                return None;
            }
        };
        let mut found = Vec::with_capacity(entries.len());
        for entry in entries {
            if !allowed.contains(&entry.key.as_str()) {
                self.push(
                    UNKNOWN_FIELD,
                    entry.key_span,
                    serde_json::json!({ "field": bounded_field_echo(&entry.key) }),
                );
                continue;
            }
            if !VALUE_DECODED_KEYS.contains(&entry.key.as_str()) {
                self.record(
                    &format!("{base_pointer}/{}", pointer_token(&entry.key)),
                    entry.key_span,
                    None,
                );
            }
            found.push((entry.key.as_str(), entry));
        }
        Some(found)
    }

    /// Look up one accepted key.
    fn find<'a>(&self, found: &'a [(&'a str, &'a MapEntry)], key: &str) -> Option<&'a MapEntry> {
        for (name, entry) in found {
            if *name == key {
                return Some(entry);
            }
        }
        None
    }

    /// Require one accepted key, recording `ir.missing-field` otherwise.
    fn require<'a>(
        &mut self,
        found: &'a [(&'a str, &'a MapEntry)],
        key: &str,
    ) -> Option<&'a MapEntry> {
        match self.find(found, key) {
            Some(entry) => Some(entry),
            None => {
                self.push(
                    MISSING_FIELD,
                    Span::EMPTY_EOF,
                    serde_json::json!({ "field": bounded_field_echo(key) }),
                );
                None
            }
        }
    }

    /// Decode a bounded human-readable text value.
    fn text(&mut self, node: &Node, detail: &'static str) -> Option<Text> {
        let value = self.string(node, detail)?;
        if value.is_empty() || value.chars().count() > MAX_TEXT_CHARS {
            self.push(
                VALUE_INVALID,
                node.span,
                serde_json::json!({ "detail": "text-length" }),
            );
            return None;
        }
        Some(Text(value))
    }

    /// Decode a string value; any other node is a typed failure.
    fn string(&mut self, node: &Node, detail: &'static str) -> Option<String> {
        match &node.value {
            Value::Scalar(Scalar::Str(text)) => Some(text.clone()),
            _ => {
                self.push(
                    VALUE_INVALID,
                    node.span,
                    serde_json::json!({ "detail": detail }),
                );
                None
            }
        }
    }

    /// Decode a positive integer value.
    fn positive_integer(&mut self, node: &Node, detail: &'static str) -> Option<u64> {
        match &node.value {
            Value::Scalar(Scalar::Int(value)) if *value >= 1 => match u64::try_from(*value) {
                Ok(value) => Some(value),
                Err(_) => {
                    self.push(
                        VALUE_INVALID,
                        node.span,
                        serde_json::json!({ "detail": detail }),
                    );
                    None
                }
            },
            _ => {
                self.push(
                    VALUE_INVALID,
                    node.span,
                    serde_json::json!({ "detail": detail }),
                );
                None
            }
        }
    }

    /// Decode the exact-literal `true` (the only accepted spelling).
    fn optional_true(&mut self, node: &Node, key: &'static str) -> bool {
        match &node.value {
            Value::Scalar(Scalar::Bool(true)) => true,
            _ => {
                self.push(
                    VALUE_INVALID,
                    node.span,
                    serde_json::json!({ "detail": "const-true", "field": key }),
                );
                false
            }
        }
    }

    /// Decode one closed-enum string literal.
    fn literal<T, const N: usize>(
        &mut self,
        node: &Node,
        detail: &'static str,
        accepted: [(&'static str, T); N],
    ) -> Option<T> {
        let text = self.string(node, detail)?;
        for (candidate, value) in accepted {
            if text == candidate {
                return Some(value);
            }
        }
        self.push(
            VALUE_INVALID,
            node.span,
            serde_json::json!({ "detail": detail, "value": bounded_field_echo(&text) }),
        );
        None
    }

    /// Decode one array of validated strings, recording every member.
    /// Set-like surfaces reject duplicates.
    fn string_array(
        &mut self,
        node: &Node,
        base_pointer: &str,
        detail: &'static str,
        valid: impl Fn(&Context, &str) -> bool,
        set_like: bool,
        field: &'static str,
    ) -> Option<Vec<String>> {
        let items = match &node.value {
            Value::Seq(items) => items,
            _ => {
                self.push(
                    VALUE_INVALID,
                    node.span,
                    serde_json::json!({ "detail": detail }),
                );
                return None;
            }
        };
        let mut values = Vec::with_capacity(items.len());
        let mut ok = true;
        for (position, item) in items.iter().enumerate() {
            self.record(&format!("{base_pointer}/{position}"), item.span, None);
            let Some(text) = self.string(item, detail) else {
                ok = false;
                continue;
            };
            if !valid(self, &text) {
                self.push(
                    VALUE_INVALID,
                    item.span,
                    serde_json::json!({
                        "detail": format!("{detail}-member"),
                        "value": bounded_field_echo(&text),
                    }),
                );
                ok = false;
                continue;
            }
            values.push(text);
        }
        if set_like {
            let mut unique = values.clone();
            unique.sort();
            unique.dedup();
            if unique.len() != values.len() {
                if let Some(duplicate) = unique
                    .iter()
                    .find(|value| values.iter().filter(|other| *other == *value).count() > 1)
                {
                    self.push(
                        DUPLICATE_MEMBER,
                        node.span,
                        serde_json::json!({
                            "field": field,
                            "value": bounded_field_echo(duplicate),
                        }),
                    );
                }
                ok = false;
            }
        }
        if ok {
            Some(values)
        } else {
            None
        }
    }
}

fn valid_symbol(version: ModelVersion) -> impl Fn(&Context, &str) -> bool {
    move |_context, text| is_symbol_id(version, text)
}

fn valid_module_id(version: ModelVersion) -> impl Fn(&Context, &str) -> bool {
    move |_context, text| version.module_id_valid(text)
}

/// The allowed key set shared by every definition plus `extra` kind keys.
fn common_keys(extra: &[&'static str], version: ModelVersion) -> Vec<&'static str> {
    let mut keys: Vec<&'static str> = vec![
        "id",
        "kind",
        "version",
        "description",
        "derived_from",
        "visibility",
        "portability",
    ];
    if version == ModelVersion::V1_0_0 {
        keys.push("renamed_from");
    }
    keys.extend_from_slice(extra);
    keys
}

/// Decode the shared common fields from an already-scanned key list.
fn decode_common(found: &[(&str, &MapEntry)], pointer: &str, cx: &mut Context) -> Option<Common> {
    let version_entry = cx.require(found, "version")?;
    let version = cx.positive_integer(&version_entry.value, "version-integer")?;
    let description = match cx.find(found, "description") {
        Some(entry) => Some(cx.text(&entry.value, "description-type")?),
        None => None,
    };

    let derived_from = match cx.find(found, "derived_from") {
        Some(entry) => cx.string_array(
            &entry.value,
            &format!("{pointer}/derived_from"),
            "derived-from-array",
            |_context, text| is_requirement_id(text),
            true,
            "derived_from",
        )?,
        None => Vec::new(),
    };
    let visibility = match cx.find(found, "visibility") {
        Some(entry) => Some(cx.literal(
            &entry.value,
            "visibility-unknown",
            [
                ("module", Visibility::Module),
                ("project", Visibility::Project),
            ],
        )?),
        None => None,
    };

    let portability = match cx.find(found, "portability") {
        Some(entry) => Some(cx.literal(
            &entry.value,
            "portability-unknown",
            [
                ("portable", Portability::Portable),
                ("target-specific", Portability::TargetSpecific),
            ],
        )?),
        None => None,
    };

    let renamed_from = match cx.find(found, "renamed_from") {
        Some(entry) => cx.string_array(
            &entry.value,
            &format!("{pointer}/renamed_from"),
            "renamed-from-array",
            valid_symbol(cx.version),
            true,
            "renamed_from",
        )?,
        None => Vec::new(),
    };

    Some(Common {
        version,
        description,
        derived_from: derived_from.into_iter().map(RequirementId).collect(),
        visibility,
        portability,
        renamed_from: renamed_from.into_iter().map(SymbolId).collect(),
    })
}

/// Decode the definition `id` value against the version grammar.
fn decode_id(entry: &MapEntry, cx: &mut Context, project: bool, module: bool) -> Option<String> {
    let text = cx.string(&entry.value, "id-type")?;
    let valid = match (project, module) {
        (true, _) => is_project_id(cx.version, &text),
        (_, true) => cx.version.module_id_valid(&text),
        _ => is_symbol_id(cx.version, &text),
    };
    if valid {
        Some(text)
    } else {
        cx.push(
            VALUE_INVALID,
            entry.value.span,
            serde_json::json!({ "detail": "id-syntax", "value": bounded_field_echo(&text) }),
        );
        None
    }
}

/// Require the exact `kind` literal of a project or module document.
fn require_exact_kind(node: &Node, expected: &str, cx: &mut Context) -> Option<()> {
    let entry = node.get("kind")?;
    match entry.as_str() {
        Some(kind) if kind == expected => Some(()),
        Some(other) => {
            cx.push(
                KIND_PLACEMENT,
                entry.span,
                serde_json::json!({
                    "detail": "kind-placement",
                    "kind": bounded_field_echo(other),
                    "expected": expected,
                }),
            );
            None
        }
        None => {
            cx.push(
                VALUE_INVALID,
                entry.span,
                serde_json::json!({ "detail": "kind-type" }),
            );
            None
        }
    }
}

fn decode_project(
    definition: &NormalizedDefinition,
    pointer: &str,
    cx: &mut Context,
) -> Option<Project> {
    cx.path = definition.path.clone();
    cx.record(pointer, definition.span, Some(&definition.id));
    require_exact_kind(&definition.node, "project", cx)?;
    let keys = common_keys(&["id_registry"], cx.version);
    let found = cx.scan(&definition.node, pointer, &keys)?;
    let id = decode_id(cx.require(&found, "id")?, cx, true, false)?;
    let common = decode_common(&found, pointer, cx)?;
    let id_registry = match cx.find(&found, "id_registry") {
        Some(entry) => Some(decode_id_registry(
            &entry.value,
            &format!("{pointer}/id_registry"),
            cx,
        )?),
        None => None,
    };
    Some(Project {
        id: ProjectId(id),
        common,
        id_registry,
    })
}

fn decode_id_registry(node: &Node, pointer: &str, cx: &mut Context) -> Option<IdRegistry> {
    let found = cx.scan(node, pointer, &["rename_history", "tombstones"])?;
    if cx.find(&found, "rename_history").is_none() && cx.find(&found, "tombstones").is_none() {
        cx.push(
            VALUE_INVALID,
            node.span,
            serde_json::json!({ "detail": "registry-empty" }),
        );
        return None;
    }
    let rename_history = match cx.find(&found, "rename_history") {
        Some(entry) => {
            decode_rename_history(&entry.value, &format!("{pointer}/rename_history"), cx)?
        }
        None => Vec::new(),
    };
    let tombstones = match cx.find(&found, "tombstones") {
        Some(entry) => decode_tombstones(&entry.value, &format!("{pointer}/tombstones"), cx)?,
        None => Vec::new(),
    };
    Some(IdRegistry {
        rename_history,
        tombstones,
    })
}

fn decode_rename_history(
    node: &Node,
    pointer: &str,
    cx: &mut Context,
) -> Option<Vec<RenameHistoryEntry>> {
    let items = match &node.value {
        Value::Seq(items) if !items.is_empty() => items,
        _ => {
            cx.push(
                VALUE_INVALID,
                node.span,
                serde_json::json!({ "detail": "rename-history-array" }),
            );
            return None;
        }
    };
    let mut entries = Vec::with_capacity(items.len());
    let mut ok = true;
    for (position, item) in items.iter().enumerate() {
        let base = format!("{pointer}/{position}");
        cx.record(&base, item.span, None);
        let Some(found) = cx.scan(
            item,
            &base,
            &["from", "to", "definition_version", "same_identity", "note"],
        ) else {
            ok = false;
            continue;
        };
        let (Some(from), Some(to), Some(version)) = (
            cx.find(&found, "from"),
            cx.find(&found, "to"),
            cx.find(&found, "definition_version"),
        ) else {
            for key in ["from", "to", "definition_version"] {
                if cx.find(&found, key).is_none() {
                    cx.push(
                        MISSING_FIELD,
                        item.span,
                        serde_json::json!({ "field": key }),
                    );
                }
            }
            ok = false;
            continue;
        };
        let Some(from) = cx.string(&from.value, "rename-from-type") else {
            ok = false;
            continue;
        };
        let Some(to) = cx.string(&to.value, "rename-to-type") else {
            ok = false;
            continue;
        };
        let Some(version) = cx.positive_integer(&version.value, "definition-version-integer")
        else {
            ok = false;
            continue;
        };
        if !is_symbol_id(cx.version, &from) || !is_symbol_id(cx.version, &to) {
            cx.push(
                VALUE_INVALID,
                item.span,
                serde_json::json!({ "detail": "rename-id-syntax" }),
            );
            ok = false;
            continue;
        }
        let same_identity = match cx.find(&found, "same_identity") {
            Some(entry) => cx.optional_true(&entry.value, "same_identity"),
            None => false,
        };
        if !same_identity && cx.find(&found, "same_identity").is_some() {
            ok = false;
            continue;
        }
        let note = match cx.find(&found, "note") {
            Some(entry) => Some(cx.text(&entry.value, "note-type")?),
            None => None,
        };
        entries.push(RenameHistoryEntry {
            from: SymbolId(from),
            to: SymbolId(to),
            definition_version: version,
            same_identity,
            note,
        });
    }
    if ok {
        Some(entries)
    } else {
        None
    }
}

fn decode_tombstones(node: &Node, pointer: &str, cx: &mut Context) -> Option<Vec<Tombstone>> {
    let items = match &node.value {
        Value::Seq(items) if !items.is_empty() => items,
        _ => {
            cx.push(
                VALUE_INVALID,
                node.span,
                serde_json::json!({ "detail": "tombstones-array" }),
            );
            return None;
        }
    };
    let mut entries = Vec::with_capacity(items.len());
    let mut ok = true;
    for (position, item) in items.iter().enumerate() {
        let base = format!("{pointer}/{position}");
        cx.record(&base, item.span, None);
        let found = cx.scan(item, &base, &["id", "reason", "replaced_by", "since"])?;
        let (Some(id), Some(reason), Some(since)) = (
            cx.find(&found, "id"),
            cx.find(&found, "reason"),
            cx.find(&found, "since"),
        ) else {
            for key in ["id", "reason", "since"] {
                if cx.find(&found, key).is_none() {
                    cx.push(
                        MISSING_FIELD,
                        item.span,
                        serde_json::json!({ "field": key }),
                    );
                }
            }
            ok = false;
            continue;
        };
        let Some(id) = decode_id(id, cx, false, false) else {
            ok = false;
            continue;
        };
        let Some(reason) = cx.string(&reason.value, "tombstone-reason-type") else {
            ok = false;
            continue;
        };
        let Some(since) = cx.positive_integer(&since.value, "since-integer") else {
            ok = false;
            continue;
        };
        match reason.as_str() {
            "replaced" => {
                let Some(replaced_by) = cx.find(&found, "replaced_by") else {
                    cx.push(
                        MISSING_FIELD,
                        item.span,
                        serde_json::json!({ "field": "replaced_by" }),
                    );
                    ok = false;
                    continue;
                };
                let Some(replaced_by) = decode_id(replaced_by, cx, false, false) else {
                    ok = false;
                    continue;
                };
                entries.push(Tombstone::Replaced {
                    id: SymbolId(id),
                    replaced_by: SymbolId(replaced_by),
                    since,
                });
            }
            "deleted" => {
                if cx.find(&found, "replaced_by").is_some() {
                    cx.push(
                        VALUE_INVALID,
                        item.span,
                        serde_json::json!({ "detail": "tombstone-replaced-by-forbidden" }),
                    );
                    ok = false;
                    continue;
                }
                entries.push(Tombstone::Deleted {
                    id: SymbolId(id),
                    since,
                });
            }
            other => {
                cx.push(
                    VALUE_INVALID,
                    item.span,
                    serde_json::json!({
                        "detail": "tombstone-reason-unknown",
                        "value": bounded_field_echo(other),
                    }),
                );
                ok = false;
            }
        }
    }
    if ok {
        Some(entries)
    } else {
        None
    }
}

fn decode_module(
    definition: &NormalizedDefinition,
    pointer: &str,
    cx: &mut Context,
) -> Option<Module> {
    cx.path = definition.path.clone();
    cx.record(pointer, definition.span, Some(&definition.id));
    require_exact_kind(&definition.node, "module", cx)?;
    let keys = common_keys(&["imports"], cx.version);
    let found = cx.scan(&definition.node, pointer, &keys)?;
    let id = decode_id(cx.require(&found, "id")?, cx, false, true)?;
    let common = decode_common(&found, pointer, cx)?;
    let imports = match cx.find(&found, "imports") {
        Some(entry) => cx.string_array(
            &entry.value,
            &format!("{pointer}/imports"),
            "imports-array",
            valid_module_id(cx.version),
            true,
            "imports",
        )?,
        None => Vec::new(),
    };
    Some(Module {
        id: ModuleId(id),
        common,
        imports: imports.into_iter().map(ModuleId).collect(),
    })
}

/// Decode one type expression: `ref`, `list`, or `optional`, at most
/// [`MAX_TYPE_LEVELS`] levels deep including the leaf. Every expression
/// node is recorded at its own pointer.
fn decode_type(node: &Node, pointer: &str, cx: &mut Context, level: usize) -> Option<TypeRef> {
    cx.record(pointer, node.span, None);
    let entries = match &node.value {
        Value::Map(entries) if entries.len() == 1 => entries,
        _ => {
            cx.push(
                VALUE_INVALID,
                node.span,
                serde_json::json!({ "detail": "type-syntax" }),
            );
            return None;
        }
    };
    let entry = &entries[0];
    match entry.key.as_str() {
        "ref" => {
            let text = cx.string(&entry.value, "type-syntax")?;
            if !is_symbol_id(cx.version, &text) {
                cx.push(
                    VALUE_INVALID,
                    entry.value.span,
                    serde_json::json!({
                        "detail": "reference-syntax",
                        "value": bounded_field_echo(&text),
                    }),
                );
                return None;
            }
            Some(TypeRef::Ref(SymbolId(text)))
        }
        "list" | "optional" => {
            if level + 2 > MAX_TYPE_LEVELS {
                cx.push(
                    VALUE_INVALID,
                    node.span,
                    serde_json::json!({ "detail": "type-depth", "maxLevels": MAX_TYPE_LEVELS }),
                );
                return None;
            }
            let inner = decode_type(
                &entry.value,
                &format!("{pointer}/{}", entry.key),
                cx,
                level + 1,
            )?;
            Some(match entry.key.as_str() {
                "list" => TypeRef::List(Box::new(inner)),
                _ => TypeRef::Optional(Box::new(inner)),
            })
        }
        other => {
            cx.push(
                VALUE_INVALID,
                node.span,
                serde_json::json!({
                    "detail": "type-wrapper-unknown",
                    "wrapper": bounded_field_echo(other),
                }),
            );
            None
        }
    }
}

/// Decode a field list (`fields`, `input`, `payload`).
fn decode_fields(node: &Node, pointer: &str, cx: &mut Context) -> Option<Vec<Field>> {
    let items = match &node.value {
        Value::Seq(items) if !items.is_empty() => items,
        _ => {
            cx.push(
                VALUE_INVALID,
                node.span,
                serde_json::json!({ "detail": "fields-array" }),
            );
            return None;
        }
    };
    let mut fields = Vec::with_capacity(items.len());
    let mut ok = true;
    for (position, item) in items.iter().enumerate() {
        let base = format!("{pointer}/{position}");
        cx.record(&base, item.span, None);
        let Some(found) = cx.scan(item, &base, &["name", "type", "required", "description"]) else {
            ok = false;
            continue;
        };
        let (Some(name), Some(type_node)) = (cx.find(&found, "name"), cx.find(&found, "type"))
        else {
            for key in ["name", "type"] {
                if cx.find(&found, key).is_none() {
                    cx.push(
                        MISSING_FIELD,
                        item.span,
                        serde_json::json!({ "field": key }),
                    );
                }
            }
            ok = false;
            continue;
        };
        let Some(name) = cx.string(&name.value, "field-name-type") else {
            ok = false;
            continue;
        };
        if !is_field_name(&name) {
            cx.push(
                VALUE_INVALID,
                item.span,
                serde_json::json!({
                    "detail": "field-name-syntax",
                    "value": bounded_field_echo(&name),
                }),
            );
            ok = false;
            continue;
        }
        let Some(field_type) = decode_type(&type_node.value, &format!("{base}/type"), cx, 0) else {
            ok = false;
            continue;
        };
        let required = match cx.find(&found, "required") {
            Some(entry) => cx.optional_true(&entry.value, "required"),
            None => false,
        };
        if !required && cx.find(&found, "required").is_some() {
            ok = false;
            continue;
        }
        let description = match cx.find(&found, "description") {
            Some(entry) => Some(cx.text(&entry.value, "description-type")?),
            None => None,
        };
        fields.push(Field {
            name: FieldName(name),
            r#type: field_type,
            required,
            description,
        });
    }
    if ok {
        Some(fields)
    } else {
        None
    }
}

/// Decode a reference array (`effects`, `reads`, `applies_to`, `emits`,
/// `covers`) with the given minimum cardinality.
fn decode_references(
    node: &Node,
    pointer: &str,
    cx: &mut Context,
    field: &'static str,
    minimum: usize,
) -> Option<Vec<SymbolId>> {
    let values = cx.string_array(
        node,
        pointer,
        "references-array",
        valid_symbol(cx.version),
        true,
        field,
    )?;
    if values.len() < minimum {
        cx.push(
            VALUE_INVALID,
            node.span,
            serde_json::json!({ "detail": "cardinality", "field": field, "minimum": minimum }),
        );
        return None;
    }
    Some(values.into_iter().map(SymbolId).collect())
}

/// Decode one fully qualified reference value.
fn decode_reference(node: &Node, pointer: &str, cx: &mut Context) -> Option<SymbolId> {
    cx.record(pointer, node.span, None);
    let text = cx.string(node, "reference-type")?;
    if !is_symbol_id(cx.version, &text) {
        cx.push(
            VALUE_INVALID,
            node.span,
            serde_json::json!({
                "detail": "reference-syntax",
                "value": bounded_field_echo(&text),
            }),
        );
        return None;
    }
    Some(SymbolId(text))
}

/// The closed kind literal of one symbol definition, with placement
/// enforced against its document home.
fn classify_kind(
    node: &Node,
    definition: &NormalizedDefinition,
    cx: &mut Context,
) -> Option<&'static str> {
    let kind_node = match node.get("kind") {
        Some(entry) => entry,
        None => {
            cx.push(
                MISSING_FIELD,
                definition.span,
                serde_json::json!({ "field": "kind" }),
            );
            return None;
        }
    };
    let kind = match kind_node.as_str() {
        Some(kind) => kind,
        None => {
            cx.push(
                VALUE_INVALID,
                kind_node.span,
                serde_json::json!({ "detail": "kind-type" }),
            );
            return None;
        }
    };
    let known: &str = match kind {
        "scalar" => "scalar",
        "enum" => "enum",
        "value-object" => "value-object",
        "entity" => "entity",
        "command" => "command",
        "query" => "query",
        "policy" => "policy",
        "event" => "event",
        "effect" => "effect",
        "endpoint" => "endpoint",
        "scenario" => "scenario",
        "target-binding" => "target-binding",
        "project" | "module" => {
            cx.push(
                KIND_PLACEMENT,
                kind_node.span,
                serde_json::json!({
                    "detail": "kind-placement",
                    "kind": kind,
                    "home": definition.path,
                }),
            );
            return None;
        }
        other => {
            cx.push(
                KIND_UNKNOWN,
                kind_node.span,
                serde_json::json!({ "kind": bounded_field_echo(other) }),
            );
            return None;
        }
    };
    match home_of(&definition.path) {
        Some(home) if home_accepts(home, known) => Some(known),
        _ => {
            cx.push(
                KIND_PLACEMENT,
                kind_node.span,
                serde_json::json!({
                    "detail": "kind-placement",
                    "kind": known,
                    "home": definition.path,
                }),
            );
            None
        }
    }
}

fn decode_symbol(
    definition: &NormalizedDefinition,
    pointer: &str,
    cx: &mut Context,
) -> Option<Definition> {
    cx.path = definition.path.clone();
    cx.record(pointer, definition.span, Some(&definition.id));
    let kind = classify_kind(&definition.node, definition, cx)?;
    let extra: &[&'static str] = match kind {
        "scalar" => &["base"],
        "enum" => &["values"],
        "value-object" => &["fields"],
        "entity" => &["fields", "identity"],
        "command" => &["input", "effects"],
        "query" => &["reads", "returns"],
        "policy" => &["applies_to", "decision"],
        "event" => &["payload"],
        "effect" => &["operation", "entity", "emits"],
        "endpoint" => &["invokes", "method", "path"],
        "scenario" => &["summary", "covers"],
        "target-binding" => &["target"],
        _ => unreachable!("closed kind set"),
    };
    let keys = common_keys(extra, cx.version);
    let found = cx.scan(&definition.node, pointer, &keys)?;
    let id = decode_id(cx.require(&found, "id")?, cx, false, false)?;
    let common = decode_common(&found, pointer, cx)?;
    let child = |key: &str| format!("{pointer}/{key}");
    match kind {
        "scalar" => {
            let base = cx.require(&found, "base")?;
            let base = cx.literal(
                &base.value,
                "base-unknown",
                [
                    ("string", ScalarBase::String),
                    ("number", ScalarBase::Number),
                    ("boolean", ScalarBase::Boolean),
                    ("date", ScalarBase::Date),
                    ("datetime", ScalarBase::Datetime),
                    ("uuid", ScalarBase::Uuid),
                    ("uri", ScalarBase::Uri),
                ],
            )?;
            Some(Definition::Scalar(ScalarDef {
                id: SymbolId(id),
                common,
                base,
            }))
        }
        "enum" => {
            let entry = cx.require(&found, "values")?;
            let values = decode_enum_values(&entry.value, &child("values"), cx)?;
            Some(Definition::Enum(EnumDef {
                id: SymbolId(id),
                common,
                values,
            }))
        }
        "value-object" => {
            let fields = decode_fields(&cx.require(&found, "fields")?.value, &child("fields"), cx)?;
            Some(Definition::ValueObject(ValueObjectDef {
                id: SymbolId(id),
                common,
                fields,
            }))
        }
        "entity" => {
            let fields = decode_fields(&cx.require(&found, "fields")?.value, &child("fields"), cx)?;
            let identity_entry = cx.require(&found, "identity")?;
            let identity = cx.string_array(
                &identity_entry.value,
                &child("identity"),
                "identity-array",
                |_context, text| is_field_name(text),
                true,
                "identity",
            )?;
            if identity.is_empty() {
                cx.push(
                    VALUE_INVALID,
                    identity_entry.value.span,
                    serde_json::json!({
                        "detail": "cardinality",
                        "field": "identity",
                        "minimum": 1,
                    }),
                );
                return None;
            }
            Some(Definition::Entity(EntityDef {
                id: SymbolId(id),
                common,
                fields,
                identity: identity.into_iter().map(FieldName).collect(),
            }))
        }
        "command" => {
            let input = match cx.find(&found, "input") {
                Some(entry) => decode_fields(&entry.value, &child("input"), cx)?,
                None => Vec::new(),
            };
            let effects = match cx.find(&found, "effects") {
                Some(entry) => {
                    decode_references(&entry.value, &child("effects"), cx, "effects", 0)?
                }
                None => Vec::new(),
            };
            Some(Definition::Command(CommandDef {
                id: SymbolId(id),
                common,
                input,
                effects,
            }))
        }
        "query" => {
            let reads = decode_references(
                &cx.require(&found, "reads")?.value,
                &child("reads"),
                cx,
                "reads",
                1,
            )?;
            let returns = match cx.find(&found, "returns") {
                Some(entry) => Some(decode_type(&entry.value, &child("returns"), cx, 0)?),
                None => None,
            };
            Some(Definition::Query(QueryDef {
                id: SymbolId(id),
                common,
                reads,
                returns,
            }))
        }
        "policy" => {
            let applies_to = decode_references(
                &cx.require(&found, "applies_to")?.value,
                &child("applies_to"),
                cx,
                "applies_to",
                1,
            )?;
            let decision = cx.require(&found, "decision")?;
            let decision = cx.literal(
                &decision.value,
                "decision-unknown",
                [("allow", Decision::Allow), ("deny", Decision::Deny)],
            )?;
            Some(Definition::Policy(PolicyDef {
                id: SymbolId(id),
                common,
                applies_to,
                decision,
            }))
        }
        "event" => {
            let payload = match cx.find(&found, "payload") {
                Some(entry) => decode_fields(&entry.value, &child("payload"), cx)?,
                None => Vec::new(),
            };
            Some(Definition::Event(EventDef {
                id: SymbolId(id),
                common,
                payload,
            }))
        }
        "effect" => {
            let operation = cx.require(&found, "operation")?;
            let operation = cx.literal(
                &operation.value,
                "operation-unknown",
                [
                    ("create", EffectOperation::Create),
                    ("update", EffectOperation::Update),
                    ("delete", EffectOperation::Delete),
                ],
            )?;
            let entity =
                decode_reference(&cx.require(&found, "entity")?.value, &child("entity"), cx)?;
            let emits = match cx.find(&found, "emits") {
                Some(entry) => decode_references(&entry.value, &child("emits"), cx, "emits", 0)?,
                None => Vec::new(),
            };
            Some(Definition::Effect(EffectDef {
                id: SymbolId(id),
                common,
                operation,
                entity,
                emits,
            }))
        }
        "endpoint" => {
            let invokes =
                decode_reference(&cx.require(&found, "invokes")?.value, &child("invokes"), cx)?;
            let method = cx.require(&found, "method")?;
            let method = cx.literal(
                &method.value,
                "method-unknown",
                [
                    ("GET", HttpMethod::Get),
                    ("POST", HttpMethod::Post),
                    ("PUT", HttpMethod::Put),
                    ("PATCH", HttpMethod::Patch),
                    ("DELETE", HttpMethod::Delete),
                ],
            )?;
            let path_entry = cx.require(&found, "path")?;
            let path = cx.string(&path_entry.value, "path-type")?;
            if !is_endpoint_path(&path) {
                cx.push(
                    VALUE_INVALID,
                    path_entry.value.span,
                    serde_json::json!({
                        "detail": "path-syntax",
                        "value": bounded_field_echo(&path),
                    }),
                );
                return None;
            }
            Some(Definition::Endpoint(EndpointDef {
                id: SymbolId(id),
                common,
                invokes,
                method,
                path: EndpointPath(path),
            }))
        }
        "scenario" => {
            let summary_entry = cx.require(&found, "summary")?;
            let summary = cx.text(&summary_entry.value, "summary-type")?;
            let covers = match cx.find(&found, "covers") {
                Some(entry) => decode_references(&entry.value, &child("covers"), cx, "covers", 0)?,
                None => Vec::new(),
            };
            Some(Definition::Scenario(ScenarioDef {
                id: SymbolId(id),
                common,
                summary,
                covers,
            }))
        }
        "target-binding" => {
            let target_entry = cx.require(&found, "target")?;
            let target = cx.string(&target_entry.value, "target-type")?;
            if !is_target_name(&target) {
                cx.push(
                    VALUE_INVALID,
                    target_entry.value.span,
                    serde_json::json!({
                        "detail": "target-syntax",
                        "value": bounded_field_echo(&target),
                    }),
                );
                return None;
            }
            Some(Definition::TargetBinding(TargetBindingDef {
                id: SymbolId(id),
                common,
                target: TargetName(target),
            }))
        }
        _ => unreachable!("closed kind set"),
    }
}

/// Decode the closed enum member list.
fn decode_enum_values(node: &Node, pointer: &str, cx: &mut Context) -> Option<Vec<EnumValue>> {
    let items = match &node.value {
        Value::Seq(items) if !items.is_empty() => items,
        _ => {
            cx.push(
                VALUE_INVALID,
                node.span,
                serde_json::json!({ "detail": "values-array" }),
            );
            return None;
        }
    };
    let mut values = Vec::with_capacity(items.len());
    let mut ok = true;
    for (position, item) in items.iter().enumerate() {
        let base = format!("{pointer}/{position}");
        cx.record(&base, item.span, None);
        let Some(found) = cx.scan(item, &base, &["value", "description"]) else {
            ok = false;
            continue;
        };
        let value_entry = cx.require(&found, "value")?;
        let Some(value) = cx.string(&value_entry.value, "enum-value-type") else {
            ok = false;
            continue;
        };
        if !is_field_name(&value) {
            cx.push(
                VALUE_INVALID,
                item.span,
                serde_json::json!({
                    "detail": "enum-value-syntax",
                    "value": bounded_field_echo(&value),
                }),
            );
            ok = false;
            continue;
        }
        let description = match cx.find(&found, "description") {
            Some(entry) => Some(cx.text(&entry.value, "description-type")?),
            None => None,
        };
        values.push(EnumValue {
            value: EnumValueName(value),
            description,
        });
    }
    if ok {
        Some(values)
    } else {
        None
    }
}

#[allow(dead_code)]
fn unused_segment_guard(text: &str) -> bool {
    is_segment(text)
}
