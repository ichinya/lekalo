//! The PHP operations join (issue #59).
//!
//! This module is the core-side acceptance authority for the bounded
//! `dev.lekalo.php-operations-input@0.4.0` document authored under
//! `lekalo/operations/<project>.operations.json`. It validates the closed
//! input shape and canonical order, then joins every declared operation
//! against exactly four owned authorities — never its own guesses:
//!
//! 1. the compiled project IR (operation/policy/effect/event/entity
//!    definitions by semantic id);
//! 2. the embedded #62 error registry (the declared errors of an
//!    operation are the registry binding set, exact equality);
//! 3. the staged IR evidence bytes (`.lekalo/cache/ir/<project>.json`)
//!    and the bound #58 types input (`lekalo/types/<project>.types.json`)
//!    by exact digest;
//! 4. the closed managed recipe grammar (typed operands, full field
//!    coverage, no query writes).
//!
//! A missing context is a typed finding; absence is never read as "no
//! policy", "pure query", or "no transaction". Every refusal happens
//! before any generated source write.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::error_contract::registry::ErrorRegistry;
use crate::ir::{CompiledProject, Definition, TypeRef};

/// The closed identity members of the v0.4.0 input contract.
pub const INPUT_SCHEMA_VERSION: &str = "lekalo/php-operations-input/v0.4.0";
pub const INPUT_IDENTITY: &str = "dev.lekalo.php-operations-input@0.4.0";

/// The closed default namespace prefix.
pub const DEFAULT_NAMESPACE_PREFIX: &str = "Lekalo\\Generated\\Operations";

/// The staged IR evidence home the adapter (and this join) reads.
pub const IR_EVIDENCE_HOME: &str = ".lekalo/cache/ir";

/// The bound types input home of the #58 family.
pub const TYPES_INPUT_HOME: &str = "lekalo/types";

/// One typed join finding. `pointer` is the input-relative JSON pointer
/// of the offending member when one exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    /// The closed bounded reason token (`operations.query-write`).
    pub code: String,
    /// The operation semantic id, when bound to one record.
    pub semantic_id: Option<String>,
    /// The input-relative JSON pointer, when known.
    pub pointer: Option<String>,
    /// The bounded human detail.
    pub detail: String,
}

impl Finding {
    fn new(
        code: &str,
        semantic_id: Option<&str>,
        pointer: Option<&str>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code: code.to_owned(),
            semantic_id: semantic_id.map(str::to_owned),
            pointer: pointer.map(str::to_owned),
            detail: detail.into(),
        }
    }
}

/// The closed implementation mode of one operation record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Managed,
    ScaffoldOnce,
    Checked,
    Custom,
}

impl Mode {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "managed" => Some(Self::Managed),
            "scaffold-once" => Some(Self::ScaffoldOnce),
            "checked" => Some(Self::Checked),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    /// Whether the mode requires the declared native entrypoint.
    pub fn requires_entry(self) -> bool {
        matches!(self, Self::Checked | Self::Custom)
    }

    /// Whether the mode emits any write.
    pub fn emits(self) -> bool {
        matches!(self, Self::Managed | Self::ScaffoldOnce)
    }
}

/// The closed transaction binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionMode {
    Required,
    Forbidden,
}

/// One closed typed operand of a managed recipe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operand {
    /// The value of one command input field.
    FromInput(String),
    /// The value of one field of the read entity.
    FromEntity(String),
    /// One declared enum case of the target type (the IR value spelling).
    EnumCase { enum_id: String, value: String },
    /// A scalar literal (string, boolean, or integer).
    Literal(Value),
}

/// The closed managed recipe vocabulary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Recipe {
    /// One typed port call carries the business body.
    PortDelegation {
        port: String,
        method: String,
        result: Option<String>,
    },
    /// The bounded single-entity update.
    SingleEntityUpdate {
        entity: String,
        key: String,
        assignments: Vec<(String, Operand)>,
        kept: Vec<String>,
        preconditions: Vec<Precondition>,
        missing_error: String,
        emissions: Vec<(String, BTreeMap<String, Operand>)>,
    },
}

/// One typed equality precondition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Precondition {
    pub field: String,
    pub equals: Operand,
    pub error: String,
}

/// The declared native entrypoint of a checked/custom record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub fqn: String,
    pub method: String,
    pub path: String,
}

/// One validated operation record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationRecord {
    pub id: String,
    pub kind: OperationKind,
    pub mode: Mode,
    pub entry: Option<Entry>,
    pub recipe: Option<Recipe>,
    pub errors: Vec<String>,
    pub policy: Option<String>,
    pub transaction: TransactionMode,
}

/// The closed operation kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    Command,
    Query,
}

impl OperationKind {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "command" => Some(Self::Command),
            "query" => Some(Self::Query),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Command => "command",
            Self::Query => "query",
        }
    }
}

/// The validated operations input document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationsInput {
    pub project_id: String,
    pub ir_digest: String,
    pub types_input_digest: String,
    pub namespace_prefix: String,
    pub operations: Vec<OperationRecord>,
}

fn is_sha256_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_semantic_id(value: &str) -> bool {
    let Some((module, name)) = value.split_once('.') else {
        return false;
    };
    is_lower_snake(module) && is_lower_snake(name)
}

fn is_lower_snake(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn is_field_name(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_pascal_name(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() => {}
        _ => return false,
    }
    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_camel_method(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_php_fqn(value: &str) -> bool {
    let mut segments = value.split('\\');
    let Some(first) = segments.next() else {
        return false;
    };
    let segment_ok = |segment: &str| {
        let mut chars = segment.chars();
        match chars.next() {
            Some(c) if c.is_ascii_uppercase() || c == '_' => {}
            _ => return false,
        }
        segment.len() <= 128
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    if !segment_ok(first) {
        return false;
    }
    let count = 1 + segments.count();
    (2..=8).contains(&count) && value.split('\\').all(segment_ok)
}

fn is_logical_path(value: &str) -> bool {
    !value.starts_with('/')
        && !value.contains("..")
        && !value.contains('\\')
        && value.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '/' | '-')
        })
}

/// The closed member set of one JSON object (additionalProperties:
/// false): an unknown member is an authoring error, never a silently
/// ignored hint.
fn members_closed(object: &serde_json::Map<String, Value>, allowed: &[&str]) -> bool {
    object
        .keys()
        .all(|member| allowed.contains(&member.as_str()))
}

/// Parse and shape-validate the input document. The canonical order
/// (id-sorted, unique operations) is part of the shape: a misordered or
/// duplicated document is a finding, never a silent reorder.
pub fn parse_input(bytes: &[u8]) -> Result<OperationsInput, Vec<Finding>> {
    let mut findings = Vec::new();
    let document: Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => {
            return Err(vec![Finding::new(
                "operations.input-shape",
                None,
                None,
                "the operations input is not valid JSON",
            )]);
        }
    };
    if !document.is_object() {
        return Err(vec![Finding::new(
            "operations.input-shape",
            None,
            None,
            "the operations input is not a JSON object",
        )]);
    }
    if !members_closed(
        document.as_object().expect("checked object"),
        &[
            "schemaVersion",
            "identity",
            "projectId",
            "irDigest",
            "typesInputDigest",
            "policy",
            "operations",
        ],
    ) {
        return Err(vec![Finding::new(
            "operations.input-shape",
            None,
            None,
            "the operations input carries an unknown member",
        )]);
    }
    if document.get("schemaVersion").and_then(Value::as_str) != Some(INPUT_SCHEMA_VERSION)
        || document.get("identity").and_then(Value::as_str) != Some(INPUT_IDENTITY)
    {
        return Err(vec![Finding::new(
            "operations.input-identity",
            None,
            None,
            "the operations input does not carry the closed v0.4.0 identity",
        )]);
    }
    let project_id = document
        .get("projectId")
        .and_then(Value::as_str)
        .filter(|id| is_lower_snake(id))
        .unwrap_or_default()
        .to_owned();
    if project_id.is_empty() {
        findings.push(Finding::new(
            "operations.input-shape",
            None,
            Some("/projectId"),
            "projectId must be a lowercase snake token",
        ));
    }
    let ir_digest = document
        .get("irDigest")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let types_input_digest = document
        .get("typesInputDigest")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    for (name, digest) in [
        ("/irDigest", &ir_digest),
        ("/typesInputDigest", &types_input_digest),
    ] {
        if !is_sha256_digest(digest) {
            findings.push(Finding::new(
                "operations.input-shape",
                None,
                Some(name),
                "digests are canonical `sha256:<64 lowercase hex>` spellings",
            ));
        }
    }
    let mut namespace_prefix = DEFAULT_NAMESPACE_PREFIX.to_owned();
    match document.get("policy") {
        None | Some(Value::Null) => {}
        Some(policy) => match policy.get("namespacePrefix") {
            None => {
                findings.push(Finding::new(
                    "operations.input-shape",
                    None,
                    Some("/policy"),
                    "the operations policy carries no known member",
                ));
            }
            Some(prefix) => match prefix.as_str() {
                Some(prefix) if is_php_fqn(prefix) => {
                    namespace_prefix = prefix.to_owned();
                }
                _ => findings.push(Finding::new(
                    "operations.input-shape",
                    None,
                    Some("/policy/namespacePrefix"),
                    "the namespace prefix must be a bounded PHP FQN spelling",
                )),
            },
        },
    }
    let Some(operations) = document.get("operations").and_then(Value::as_array) else {
        findings.push(Finding::new(
            "operations.input-shape",
            None,
            Some("/operations"),
            "operations is a required non-empty array",
        ));
        return Err(findings);
    };
    if operations.is_empty() {
        findings.push(Finding::new(
            "operations.input-shape",
            None,
            Some("/operations"),
            "operations is a required non-empty array",
        ));
    }
    let mut records = Vec::new();
    for (index, value) in operations.iter().enumerate() {
        match parse_operation(value) {
            Ok(record) => records.push(record),
            Err(mut record_findings) => {
                for finding in &mut record_findings {
                    if finding.pointer.is_none() {
                        finding.pointer = Some(format!("/operations/{index}"));
                    }
                }
                findings.append(&mut record_findings);
            }
        }
    }
    let sorted: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
    let mut unique = sorted.clone();
    unique.sort();
    unique.dedup();
    if sorted != unique {
        findings.push(Finding::new(
            "operations.input-order",
            None,
            Some("/operations"),
            "operation records are canonical: sorted by id, duplicates refuse",
        ));
    }
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(OperationsInput {
        project_id,
        ir_digest,
        types_input_digest,
        namespace_prefix,
        operations: records,
    })
}

/// Parse one operation record against the closed shape.
fn parse_operation(value: &Value) -> Result<OperationRecord, Vec<Finding>> {
    let mut findings = Vec::new();
    if !value.is_object() {
        return Err(vec![Finding::new(
            "operations.input-shape",
            None,
            None,
            "an operation record is a JSON object",
        )]);
    };
    if !members_closed(
        value.as_object().expect("checked object"),
        &[
            "id",
            "kind",
            "mode",
            "entry",
            "recipe",
            "errors",
            "policy",
            "transaction",
        ],
    ) {
        return Err(vec![Finding::new(
            "operations.input-shape",
            None,
            None,
            "an operation record carries an unknown member",
        )]);
    }
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| is_semantic_id(id))
        .unwrap_or_default()
        .to_owned();
    if id.is_empty() {
        findings.push(Finding::new(
            "operations.input-shape",
            None,
            Some("/id"),
            "the operation id must be a `module.name` semantic id",
        ));
    }
    let reference = if id.is_empty() {
        None
    } else {
        Some(id.as_str())
    };
    let kind = match value.get("kind").and_then(Value::as_str) {
        Some(kind) => match OperationKind::parse(kind) {
            Some(kind) => Some(kind),
            None => {
                findings.push(Finding::new(
                    "operations.input-shape",
                    reference,
                    Some("/kind"),
                    "the operation kind is command or query",
                ));
                None
            }
        },
        None => {
            findings.push(Finding::new(
                "operations.input-shape",
                reference,
                Some("/kind"),
                "the operation kind is required",
            ));
            None
        }
    };
    let mode = match value.get("mode").and_then(Value::as_str) {
        Some(mode) => match Mode::parse(mode) {
            Some(mode) => Some(mode),
            None => {
                findings.push(Finding::new(
                    "operations.input-shape",
                    reference,
                    Some("/mode"),
                    "the mode is one of managed, scaffold-once, checked, custom",
                ));
                None
            }
        },
        None => {
            findings.push(Finding::new(
                "operations.input-shape",
                reference,
                Some("/mode"),
                "the implementation mode is required",
            ));
            None
        }
    };
    let entry = match value.get("entry") {
        None | Some(Value::Null) => None,
        Some(entry) => match parse_entry(entry) {
            Ok(entry) => Some(entry),
            Err(mut entry_findings) => {
                for finding in &mut entry_findings {
                    if finding.semantic_id.is_none() {
                        finding.semantic_id = reference.map(str::to_owned);
                    }
                }
                findings.append(&mut entry_findings);
                None
            }
        },
    };
    let recipe = match value.get("recipe") {
        None | Some(Value::Null) => None,
        Some(recipe) => match parse_recipe(recipe) {
            Ok(recipe) => Some(recipe),
            Err(mut recipe_findings) => {
                for finding in &mut recipe_findings {
                    if finding.semantic_id.is_none() {
                        finding.semantic_id = reference.map(str::to_owned);
                    }
                }
                findings.append(&mut recipe_findings);
                None
            }
        },
    };
    let mut errors: Vec<String> = Vec::new();
    match value.get("errors") {
        None | Some(Value::Null) => {}
        Some(items) => match items.as_array() {
            Some(items) => {
                for (index, item) in items.iter().enumerate() {
                    match item.as_str().filter(|id| is_semantic_id(id)) {
                        Some(id) => errors.push(id.to_owned()),
                        None => findings.push(Finding::new(
                            "operations.input-shape",
                            reference,
                            Some(&format!("/errors/{index}")),
                            "each declared error is a semantic id",
                        )),
                    }
                }
                let mut sorted = errors.clone();
                sorted.sort();
                sorted.dedup();
                if sorted != errors {
                    findings.push(Finding::new(
                        "operations.input-order",
                        reference,
                        Some("/errors"),
                        "declared errors are canonical: sorted, unique",
                    ));
                }
            }
            None => findings.push(Finding::new(
                "operations.input-shape",
                reference,
                Some("/errors"),
                "errors is an array of semantic ids",
            )),
        },
    }
    let policy = match value.get("policy") {
        None | Some(Value::Null) => None,
        Some(policy) => match policy
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| is_semantic_id(id))
        {
            Some(id) => Some(id.to_owned()),
            None => {
                findings.push(Finding::new(
                    "operations.input-shape",
                    reference,
                    Some("/policy/id"),
                    "the policy binding names one policy semantic id",
                ));
                None
            }
        },
    };
    let transaction = match value.get("transaction") {
        None | Some(Value::Null) => TransactionMode::Forbidden,
        Some(transaction) => match transaction.get("mode").and_then(Value::as_str) {
            Some("required") => TransactionMode::Required,
            Some("forbidden") => TransactionMode::Forbidden,
            _ => {
                findings.push(Finding::new(
                    "operations.transaction-unsupported",
                    reference,
                    Some("/transaction/mode"),
                    "v0.4.0 closes the transaction mode to required or forbidden",
                ));
                TransactionMode::Forbidden
            }
        },
    };
    let (Some(id), Some(kind), Some(mode)) = (Some(id), kind, mode) else {
        return Err(findings);
    };
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(OperationRecord {
        id,
        kind,
        mode,
        entry,
        recipe,
        errors,
        policy,
        transaction,
    })
}

/// Parse one declared native entrypoint.
fn parse_entry(value: &Value) -> Result<Entry, Vec<Finding>> {
    let fqn = value
        .get("fqn")
        .and_then(Value::as_str)
        .filter(|fqn| is_php_fqn(fqn))
        .unwrap_or_default()
        .to_owned();
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| is_logical_path(path) && path.ends_with(".php"))
        .unwrap_or_default()
        .to_owned();
    let mut findings = Vec::new();
    if fqn.is_empty() {
        findings.push(Finding::new(
            "operations.input-shape",
            None,
            Some("/entry/fqn"),
            "the entrypoint FQN must be a bounded PHP FQN",
        ));
    }
    if method != "handle" {
        findings.push(Finding::new(
            "operations.input-shape",
            None,
            Some("/entry/method"),
            "v0.4.0 closes the public business entrypoint to `handle`",
        ));
    }
    if path.is_empty() {
        findings.push(Finding::new(
            "operations.input-shape",
            None,
            Some("/entry/path"),
            "the entrypoint path must be a lowercase logical .php path",
        ));
    }
    if findings.is_empty() {
        return Ok(Entry { fqn, method, path });
    }
    Err(findings)
}

/// Parse one recipe against the closed grammar. Only the grammar is
/// decided here; the semantic join happens against the IR.
fn parse_recipe(value: &Value) -> Result<Recipe, Vec<Finding>> {
    let Some(object) = value.as_object() else {
        return Err(vec![Finding::new(
            "operations.input-shape",
            None,
            Some("/recipe"),
            "a recipe is a JSON object",
        )]);
    };
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let closed = match kind {
        "port-delegation" => members_closed(object, &["kind", "port", "method", "result"]),
        "single-entity-update" => members_closed(
            object,
            &[
                "kind",
                "entity",
                "key",
                "assignments",
                "kept",
                "preconditions",
                "missingBehavior",
                "emit",
            ],
        ),
        _ => true,
    };
    if !closed {
        return Err(vec![Finding::new(
            "operations.input-shape",
            None,
            Some("/recipe"),
            "the recipe carries an unknown member",
        )]);
    }
    if kind == "port-delegation" {
        let port = value
            .get("port")
            .and_then(Value::as_str)
            .filter(|port| is_pascal_name(port))
            .unwrap_or_default()
            .to_owned();
        let method = value
            .get("method")
            .and_then(Value::as_str)
            .filter(|method| is_camel_method(method))
            .unwrap_or_default()
            .to_owned();
        let result = value
            .get("result")
            .and_then(Value::as_str)
            .filter(|id| is_semantic_id(id))
            .map(str::to_owned);
        let mut findings = Vec::new();
        if port.is_empty() {
            findings.push(Finding::new(
                "operations.input-shape",
                None,
                Some("/recipe/port"),
                "the delegation port is a PascalCase interface stem",
            ));
        }
        if method.is_empty() {
            findings.push(Finding::new(
                "operations.input-shape",
                None,
                Some("/recipe/method"),
                "the delegation method is a camelCase identifier",
            ));
        }
        if findings.is_empty() {
            return Ok(Recipe::PortDelegation {
                port,
                method,
                result,
            });
        }
        return Err(findings);
    }
    if kind == "single-entity-update" {
        let mut findings = Vec::new();
        let entity = value
            .get("entity")
            .and_then(Value::as_str)
            .filter(|id| is_semantic_id(id))
            .unwrap_or_default()
            .to_owned();
        let key = value
            .get("key")
            .and_then(Value::as_str)
            .filter(|name| is_field_name(name))
            .unwrap_or_default()
            .to_owned();
        if entity.is_empty() {
            findings.push(Finding::new(
                "operations.input-shape",
                None,
                Some("/recipe/entity"),
                "the updated entity must be a semantic id",
            ));
        }
        if key.is_empty() {
            findings.push(Finding::new(
                "operations.input-shape",
                None,
                Some("/recipe/key"),
                "the key must be a command input field name",
            ));
        }
        let assignments = parse_operand_pairs(
            value.get("assignments"),
            "/recipe/assignments",
            &mut findings,
        );
        // The closed shape requires assignments and kept members; kept
        // may be empty (single-field entities) but never absent.
        if !value
            .as_object()
            .expect("checked object")
            .contains_key("kept")
        {
            findings.push(Finding::new(
                "operations.input-shape",
                None,
                Some("/recipe/kept"),
                "the kept member is required (possibly empty)",
            ));
        }
        let kept = match value.get("kept") {
            None | Some(Value::Null) => Vec::new(),
            Some(kept) => kept
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .filter(|name| is_field_name(name))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        };
        let mut preconditions = Vec::new();
        if let Some(items) = value.get("preconditions").and_then(Value::as_array) {
            for (index, item) in items.iter().enumerate() {
                let pointer = format!("/recipe/preconditions/{index}");
                let field = item
                    .get("field")
                    .and_then(Value::as_str)
                    .filter(|name| is_field_name(name))
                    .unwrap_or_default()
                    .to_owned();
                let error = item
                    .get("error")
                    .and_then(Value::as_str)
                    .filter(|id| is_semantic_id(id))
                    .unwrap_or_default()
                    .to_owned();
                let equals = item.get("equals");
                if field.is_empty() || error.is_empty() {
                    findings.push(Finding::new(
                        "operations.input-shape",
                        None,
                        Some(&pointer),
                        "a precondition names one entity field and one declared error",
                    ));
                }
                match equals.and_then(parse_operand_ref) {
                    Some(operand) => {
                        if !field.is_empty() && !error.is_empty() {
                            preconditions.push(Precondition {
                                field,
                                equals: operand,
                                error,
                            });
                        }
                    }
                    None => findings.push(Finding::new(
                        "operations.input-shape",
                        None,
                        Some(&format!("{pointer}/equals")),
                        "a precondition operand is one closed typed operand",
                    )),
                }
            }
        }
        let missing_error = value
            .get("missingBehavior")
            .and_then(|behavior| behavior.get("error"))
            .and_then(Value::as_str)
            .filter(|id| is_semantic_id(id))
            .unwrap_or_default()
            .to_owned();
        if missing_error.is_empty() {
            findings.push(Finding::new(
                "operations.input-shape",
                None,
                Some("/recipe/missingBehavior"),
                "the missing-record behavior names one declared error",
            ));
        }
        let mut emissions = Vec::new();
        if let Some(items) = value.get("emit").and_then(Value::as_array) {
            for (index, item) in items.iter().enumerate() {
                let pointer = format!("/recipe/emit/{index}");
                let event = item
                    .get("event")
                    .and_then(Value::as_str)
                    .filter(|id| is_semantic_id(id))
                    .unwrap_or_default()
                    .to_owned();
                let payload = item.get("payload");
                if event.is_empty() {
                    findings.push(Finding::new(
                        "operations.input-shape",
                        None,
                        Some(&pointer),
                        "an emission names one event semantic id",
                    ));
                    continue;
                }
                let Some(payload) = payload.and_then(Value::as_object) else {
                    findings.push(Finding::new(
                        "operations.input-shape",
                        None,
                        Some(&format!("{pointer}/payload")),
                        "an emission payload is a field-to-operand object",
                    ));
                    continue;
                };
                let mut fields = BTreeMap::new();
                for (field, operand) in payload {
                    if !is_field_name(field) {
                        findings.push(Finding::new(
                            "operations.input-shape",
                            None,
                            Some(&format!("{pointer}/payload/{field}")),
                            "payload field names are camelCase identifiers",
                        ));
                        continue;
                    }
                    match parse_operand(Some(operand)) {
                        Some(operand) => {
                            fields.insert(field.clone(), operand);
                        }
                        None => findings.push(Finding::new(
                            "operations.input-shape",
                            None,
                            Some(&format!("{pointer}/payload/{field}")),
                            "a payload operand is one closed typed operand",
                        )),
                    }
                }
                emissions.push((event, fields));
            }
        }
        if findings.is_empty() {
            return Ok(Recipe::SingleEntityUpdate {
                entity,
                key,
                assignments,
                kept,
                preconditions,
                missing_error,
                emissions,
            });
        }
        return Err(findings);
    }
    Err(vec![Finding::new(
        "operations.recipe-unsupported",
        None,
        Some("/recipe/kind"),
        "the recipe kind is not part of the closed v0.4.0 vocabulary",
    )])
}

/// Parse an array of `{field, value}` assignment pairs.
fn parse_operand_pairs(
    value: Option<&Value>,
    pointer: &str,
    findings: &mut Vec<Finding>,
) -> Vec<(String, Operand)> {
    let mut pairs = Vec::new();
    let Some(items) = value.and_then(Value::as_array) else {
        findings.push(Finding::new(
            "operations.input-shape",
            None,
            Some(pointer),
            "assignments is a required array of {field, value} pairs",
        ));
        return pairs;
    };
    for (index, item) in items.iter().enumerate() {
        let field = item
            .get("field")
            .and_then(Value::as_str)
            .filter(|name| is_field_name(name))
            .unwrap_or_default()
            .to_owned();
        match item.get("value").and_then(parse_operand_ref) {
            Some(operand) if !field.is_empty() => pairs.push((field, operand)),
            _ => findings.push(Finding::new(
                "operations.input-shape",
                None,
                Some(&format!("{pointer}/{index}")),
                "an assignment pairs one entity field with one closed typed operand",
            )),
        }
    }
    pairs
}

/// The borrowed-reference form of [`parse_operand`].
fn parse_operand_ref(value: &Value) -> Option<Operand> {
    parse_operand(Some(value))
}

/// Parse one closed typed operand.
fn parse_operand(value: Option<&Value>) -> Option<Operand> {
    let value = value?;
    let object = value.as_object()?;
    if let Some(field) = object.get("fromInput").and_then(Value::as_str) {
        return is_field_name(field).then(|| Operand::FromInput(field.to_owned()));
    }
    if let Some(field) = object.get("fromEntity").and_then(Value::as_str) {
        return is_field_name(field).then(|| Operand::FromEntity(field.to_owned()));
    }
    if let Some(enum_case) = object.get("enumCase") {
        let enum_id = enum_case.get("type").and_then(Value::as_str)?;
        let case = enum_case.get("value").and_then(Value::as_str)?;
        return (is_semantic_id(enum_id)
            && !case.is_empty()
            && case.len() <= 64
            && case
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'))
        .then(|| Operand::EnumCase {
            enum_id: enum_id.to_owned(),
            value: case.to_owned(),
        });
    }
    if let Some(literal) = object.get("literal") {
        return match literal {
            Value::String(text) if text.len() <= 256 => Some(Operand::Literal(literal.clone())),
            Value::Bool(_) => Some(Operand::Literal(literal.clone())),
            Value::Number(number) if number.is_i64() => Some(Operand::Literal(literal.clone())),
            _ => None,
        };
    }
    None
}

/// One IR lookup context of one join, fully typed: the compiled IR has
/// no JSON escape hatch, so the join never re-decodes source text.
struct IrContext<'a> {
    definitions: BTreeMap<&'a str, &'a Definition>,
}

impl<'a> IrContext<'a> {
    fn of(compilation: &'a CompiledProject) -> Self {
        let mut definitions = BTreeMap::new();
        for definition in &compilation.definitions {
            definitions.insert(definition.id().as_str(), definition);
        }
        Self { definitions }
    }
}

/// The staged-evidence digest context: exact bytes, exact digests.
pub struct DigestContext {
    pub ir_digest: String,
    pub types_input_digest: String,
}

/// Read the digest context of one project root: the staged IR evidence
/// and the bound types input bytes, digested exactly.
pub fn digest_context(root: &Path, project_id: &str) -> Option<DigestContext> {
    use crate::digest::sha256_hex;
    let ir_bytes =
        std::fs::read(root.join(format!("{IR_EVIDENCE_HOME}/{project_id}.json"))).ok()?;
    let types_bytes =
        std::fs::read_to_string(root.join(format!("{TYPES_INPUT_HOME}/{project_id}.types.json")))
            .ok()?;
    Some(DigestContext {
        ir_digest: format!("sha256:{}", sha256_hex(&ir_bytes)),
        types_input_digest: format!("sha256:{}", sha256_hex(types_bytes.as_bytes())),
    })
}

/// Join one parsed input against the compiled IR, the embedded #62
/// error registry, and the staged evidence digests. Returns every
/// finding; an empty set is the accepted join.
pub fn check_join(
    root: &Path,
    input: &OperationsInput,
    compilation: &CompiledProject,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(digests) = digest_context(root, &input.project_id) else {
        findings.push(Finding::new(
            "operations.evidence-unavailable",
            None,
            None,
            "the staged IR evidence or the bound types input is missing",
        ));
        return findings;
    };
    if digests.ir_digest != input.ir_digest {
        findings.push(Finding::new(
            "operations.ir-digest",
            None,
            Some("/irDigest"),
            "the input names different IR evidence bytes than the staged home",
        ));
    }
    if digests.types_input_digest != input.types_input_digest {
        findings.push(Finding::new(
            "operations.types-unbound",
            None,
            Some("/typesInputDigest"),
            "the input names different types-input bytes than the committed document",
        ));
    }
    let registry = match ErrorRegistry::embedded() {
        Ok(registry) => registry,
        Err(_) => {
            findings.push(Finding::new(
                "operations.registry-invalid",
                None,
                None,
                "the embedded error registry failed to validate",
            ));
            return findings;
        }
    };
    let context = IrContext::of(compilation);
    for record in &input.operations {
        check_record(record, &context, registry, &mut findings);
        check_enum_cases(record, &context, &mut findings);
    }
    findings
}

/// Every enum-case operand must name one compiled enum definition and
/// one of its declared values, verbatim.
fn check_enum_cases(
    record: &OperationRecord,
    context: &IrContext<'_>,
    findings: &mut Vec<Finding>,
) {
    let Some(Recipe::SingleEntityUpdate {
        assignments,
        preconditions,
        emissions,
        ..
    }) = &record.recipe
    else {
        return;
    };
    let mut operands: Vec<&Operand> = assignments.iter().map(|(_, operand)| operand).collect();
    operands.extend(preconditions.iter().map(|item| &item.equals));
    for (_, payload) in emissions {
        operands.extend(payload.values());
    }
    for operand in operands {
        let Operand::EnumCase { enum_id, value } = operand else {
            continue;
        };
        match context.definitions.get(enum_id.as_str()) {
            Some(Definition::Enum(enum_definition)) => {
                let declared = enum_definition
                    .values
                    .iter()
                    .any(|candidate| candidate.value.as_str() == value);
                if !declared {
                    findings.push(Finding::new(
                        "operations.type-mismatch",
                        Some(record.id.as_str()),
                        Some("/recipe"),
                        format!("the enum case value `{value}` is not declared by `{enum_id}`"),
                    ));
                }
            }
            _ => findings.push(Finding::new(
                "operations.type-unresolved",
                Some(record.id.as_str()),
                Some("/recipe"),
                format!("the enum case type `{enum_id}` is not a compiled enum"),
            )),
        }
    }
}

/// The `operations.` finding prefix keeps every reason bounded and
/// attributable to this join.
fn ir_definition<'a>(context: &'a IrContext<'_>, id: &str) -> Option<&'a Definition> {
    context.definitions.get(id).copied()
}

/// The canonical spelling of one closed type reference: `planner.x`,
/// `planner.x?`, or `list<planner.x>`.
fn type_spelling(reference: &TypeRef) -> String {
    match reference {
        TypeRef::Ref(symbol) => symbol.as_str().to_owned(),
        TypeRef::List(inner) => format!("list<{}>", type_spelling(inner)),
        TypeRef::Optional(inner) => format!("{}?", type_spelling(inner)),
    }
}

/// One operation record's full semantic join.
fn check_record(
    record: &OperationRecord,
    context: &IrContext<'_>,
    registry: &ErrorRegistry,
    findings: &mut Vec<Finding>,
) {
    let reference = Some(record.id.as_str());
    let Some(definition) = ir_definition(context, &record.id) else {
        findings.push(Finding::new(
            "operations.operation-unresolved",
            reference,
            None,
            "the operation id is not a compiled IR definition",
        ));
        return;
    };
    if definition.kind().as_str() != record.kind.as_str() {
        findings.push(Finding::new(
            "operations.kind-mismatch",
            reference,
            None,
            format!(
                "the IR declares kind `{}`, the record declares `{}`",
                definition.kind().as_str(),
                record.kind.as_str()
            ),
        ));
    }
    // Mode/recipe pairing.
    if record.mode.requires_entry() != record.entry.is_some() {
        findings.push(Finding::new(
            "operations.entry-required",
            reference,
            Some("/entry"),
            if record.mode.requires_entry() {
                "checked and custom records declare their native entrypoint"
            } else {
                "managed and scaffold-once records never declare a foreign entrypoint"
            },
        ));
    }
    // The recipe drives the emitted signature: required for managed,
    // accepted for scaffold-once (the body stays the explicit failure),
    // foreign to checked and custom.
    if (record.mode == Mode::Managed || record.mode == Mode::ScaffoldOnce)
        && record.recipe.is_none()
    {
        findings.push(Finding::new(
            "operations.recipe-required",
            reference,
            Some("/recipe"),
            "a managed or scaffold-once record names one closed recipe",
        ));
    }
    if record.recipe.is_some() && !matches!(record.mode, Mode::Managed | Mode::ScaffoldOnce) {
        findings.push(Finding::new(
            "operations.recipe-required",
            reference,
            Some("/recipe"),
            "recipes belong to managed and scaffold-once records only",
        ));
    }
    // Errors: exact embedded-registry binding equality.
    let binding = registry
        .bindings()
        .iter()
        .find(|binding| binding.operation().as_str() == record.id);
    match binding {
        None => findings.push(Finding::new(
            "operations.registry-binding",
            reference,
            Some("/errors"),
            "the embedded error registry binds no contract for this operation",
        )),
        Some(binding) => {
            if binding.kind().as_str() != record.kind.as_str() {
                findings.push(Finding::new(
                    "operations.registry-binding",
                    reference,
                    Some("/errors"),
                    "the registry binds a different operation kind",
                ));
            }
            let mut declared = record.errors.clone();
            declared.sort();
            let mut bound: Vec<String> = binding
                .errors()
                .members()
                .iter()
                .map(|member| member.id().as_str().to_owned())
                .collect();
            bound.sort();
            if declared != bound {
                findings.push(Finding::new(
                    "operations.registry-binding",
                    reference,
                    Some("/errors"),
                    "the declared errors must equal the #62 registry binding set exactly",
                ));
            }
        }
    }
    // Policy: an IR policy definition whose applies_to names this record.
    if let Some(policy_id) = &record.policy {
        match ir_definition(context, policy_id) {
            None => findings.push(Finding::new(
                "operations.policy-unresolved",
                reference,
                Some("/policy/id"),
                "the policy id is not a compiled IR definition",
            )),
            Some(Definition::Policy(policy)) => {
                if !policy.applies_to.iter().any(|id| id.as_str() == record.id) {
                    findings.push(Finding::new(
                        "operations.policy-unresolved",
                        reference,
                        Some("/policy/id"),
                        "the policy definition does not apply to this operation",
                    ));
                }
            }
            Some(_) => findings.push(Finding::new(
                "operations.policy-unresolved",
                reference,
                Some("/policy/id"),
                "the policy id names a non-policy definition",
            )),
        }
    }
    // Transaction: queries never open one, and a write recipe requires
    // exactly one bound transaction (the emitted body runs inside the
    // TransactionPort run).
    if record.kind == OperationKind::Query && record.transaction == TransactionMode::Required {
        findings.push(Finding::new(
            "operations.transaction-unsupported",
            reference,
            Some("/transaction"),
            "a query never opens a transaction",
        ));
    }
    if let Some(Recipe::SingleEntityUpdate { .. }) = &record.recipe {
        if record.transaction != TransactionMode::Required {
            findings.push(Finding::new(
                "operations.transaction-required",
                reference,
                Some("/transaction"),
                "a write recipe requires the required transaction binding: the body runs inside the TransactionPort",
            ));
        }
    }
    // The recipe join.
    match &record.recipe {
        None => {}
        Some(Recipe::PortDelegation {
            port: _,
            method: _,
            result,
        }) => {
            if record.kind == OperationKind::Query && result.is_some() {
                findings.push(Finding::new(
                    "operations.query-write",
                    reference,
                    Some("/recipe/result"),
                    "a query derives its result from the IR returns; a declared recipe result is redundant",
                ));
            }
            if let Some(result) = result {
                match ir_definition(context, result) {
                    None => findings.push(Finding::new(
                        "operations.type-unresolved",
                        reference,
                        Some("/recipe/result"),
                        "the declared result is not a compiled IR definition",
                    )),
                    Some(definition) => {
                        if !matches!(
                            definition.kind().as_str(),
                            "scalar" | "enum" | "value-object" | "entity"
                        ) {
                            findings.push(Finding::new(
                                "operations.type-unresolved",
                                reference,
                                Some("/recipe/result"),
                                "the declared result is not a mappable type definition",
                            ));
                        }
                    }
                }
            }
        }
        Some(Recipe::SingleEntityUpdate { .. }) => {
            if record.kind == OperationKind::Query {
                findings.push(Finding::new(
                    "operations.query-write",
                    reference,
                    Some("/recipe"),
                    "a query can never carry a write recipe; reads stay reads",
                ));
                return;
            }
            check_update_recipe(record, context, findings);
        }
    }
}

/// The single-entity-update join: entity/key typing, full field
/// coverage, typed operands, declared preconditions and emissions.
fn check_update_recipe(
    record: &OperationRecord,
    context: &IrContext<'_>,
    findings: &mut Vec<Finding>,
) {
    let Recipe::SingleEntityUpdate {
        entity,
        key,
        assignments,
        kept,
        preconditions,
        missing_error,
        emissions,
    } = record.recipe.as_ref().expect("update recipe")
    else {
        return;
    };
    let Definition::Command(command) = ir_definition(context, &record.id).expect("checked above")
    else {
        return;
    };
    let input_types: BTreeMap<String, String> = command
        .input
        .iter()
        .map(|field| (field.name.as_str().to_owned(), type_spelling(&field.r#type)))
        .collect();
    let Some(Definition::Entity(entity_definition)) = ir_definition(context, entity) else {
        findings.push(Finding::new(
            "operations.entity-unresolved",
            Some(record.id.as_str()),
            Some("/recipe/entity"),
            "the updated definition is not an entity",
        ));
        return;
    };
    let entity_types: BTreeMap<String, String> = entity_definition
        .fields
        .iter()
        .map(|field| (field.name.as_str().to_owned(), type_spelling(&field.r#type)))
        .collect();
    if entity_definition.identity.len() != 1 {
        findings.push(Finding::new(
            "operations.recipe-unsupported",
            Some(record.id.as_str()),
            Some("/recipe/entity"),
            "v0.4.0 updates only single-identity entities",
        ));
        return;
    }
    let identity_field = entity_definition.identity[0].as_str();
    // The key operand types.
    let key_input_type = input_types.get(key).map(String::as_str);
    let identity_type = entity_types.get(identity_field).map(String::as_str);
    match (key_input_type, identity_type) {
        (Some(input_type), Some(entity_type)) if input_type == entity_type => {}
        _ => findings.push(Finding::new(
            "operations.type-mismatch",
            Some(record.id.as_str()),
            Some("/recipe/key"),
            "the key input field type must equal the entity identity field type",
        )),
    }
    // Coverage: every non-identity entity field exactly once, either
    // assigned or kept; the identity field is always carried over.
    let mut covered: BTreeMap<&str, &str> = BTreeMap::new();
    for (field, _) in assignments {
        if let Some(previous) = covered.insert(field, "assignment") {
            findings.push(Finding::new(
                "operations.recipe-coverage",
                Some(record.id.as_str()),
                Some("/recipe/assignments"),
                format!("the field `{field}` is covered twice ({previous} and assignment)"),
            ));
        }
    }
    for field in kept {
        if let Some(previous) = covered.insert(field, "kept") {
            findings.push(Finding::new(
                "operations.recipe-coverage",
                Some(record.id.as_str()),
                Some("/recipe/kept"),
                format!("the field `{field}` is covered twice ({previous} and kept)"),
            ));
        }
    }
    for field in entity_types.keys() {
        if field == identity_field {
            continue;
        }
        if !covered.contains_key(field.as_str()) {
            findings.push(Finding::new(
                "operations.recipe-coverage",
                Some(record.id.as_str()),
                Some("/recipe"),
                format!("the entity field `{field}` is neither assigned nor kept"),
            ));
        }
    }
    // Operand typing.
    for (field, operand) in assignments {
        let Some(target) = entity_types.get(field) else {
            findings.push(Finding::new(
                "operations.recipe-coverage",
                Some(record.id.as_str()),
                Some("/recipe/assignments"),
                format!("the assigned field `{field}` is not an entity field"),
            ));
            continue;
        };
        check_operand(
            operand,
            target,
            &input_types,
            &entity_types,
            Some(record.id.as_str()),
            findings,
        );
    }
    for precondition in preconditions {
        match entity_types.get(&precondition.field) {
            None => findings.push(Finding::new(
                "operations.recipe-coverage",
                Some(record.id.as_str()),
                Some("/recipe/preconditions"),
                format!(
                    "the precondition field `{}` is not an entity field",
                    precondition.field
                ),
            )),
            Some(target) => {
                check_operand(
                    &precondition.equals,
                    target,
                    &input_types,
                    &entity_types,
                    Some(record.id.as_str()),
                    findings,
                );
            }
        }
        if !record.errors.contains(&precondition.error) {
            findings.push(Finding::new(
                "operations.registry-binding",
                Some(record.id.as_str()),
                Some("/recipe/preconditions"),
                format!(
                    "the precondition error `{}` is not a declared error of this operation",
                    precondition.error
                ),
            ));
        }
    }
    if !record.errors.contains(missing_error) {
        findings.push(Finding::new(
            "operations.registry-binding",
            Some(record.id.as_str()),
            Some("/recipe/missingBehavior"),
            "the missing-record error is not a declared error of this operation",
        ));
    }
    // Emissions: the event must be emitted by one of the command's
    // effects, and every payload field must be covered by a typed
    // operand.
    let mut emitted_events: Vec<&str> = Vec::new();
    for effect_id in &command.effects {
        if let Some(Definition::Effect(effect)) = ir_definition(context, effect_id.as_str()) {
            emitted_events.extend(effect.emits.iter().map(|event| event.as_str()));
        }
    }
    for (event, payload) in emissions {
        if !emitted_events.contains(&event.as_str()) {
            findings.push(Finding::new(
                "operations.effect-unresolved",
                Some(record.id.as_str()),
                Some("/recipe/emit"),
                format!("the event `{event}` is not emitted by any effect of this command"),
            ));
        }
        let Some(Definition::Event(event_definition)) = ir_definition(context, event) else {
            findings.push(Finding::new(
                "operations.type-unresolved",
                Some(record.id.as_str()),
                Some("/recipe/emit"),
                format!("the definition `{event}` is not an event"),
            ));
            continue;
        };
        let event_fields: Vec<(String, &TypeRef)> = event_definition
            .payload
            .iter()
            .map(|field| (field.name.as_str().to_owned(), &field.r#type))
            .collect();
        for (name, reference) in &event_fields {
            let Some(operand) = payload.get(name) else {
                findings.push(Finding::new(
                    "operations.recipe-coverage",
                    Some(record.id.as_str()),
                    Some("/recipe/emit"),
                    format!("the event field `{name}` has no operand"),
                ));
                continue;
            };
            check_operand(
                operand,
                &type_spelling(reference),
                &input_types,
                &entity_types,
                Some(record.id.as_str()),
                findings,
            );
        }
        for name in payload.keys() {
            if !event_fields.iter().any(|(field, _)| field == name) {
                findings.push(Finding::new(
                    "operations.recipe-coverage",
                    Some(record.id.as_str()),
                    Some("/recipe/emit"),
                    format!("the payload operand `{name}` is not an event field"),
                ));
            }
        }
    }
}

/// One operand's exact type reconciliation against one target type
/// spelling. `input_types` and `entity_types` are the lookup tables the
/// `fromInput`/`fromEntity` forms resolve against.
fn check_operand(
    operand: &Operand,
    target: &str,
    input_types: &BTreeMap<String, String>,
    entity_types: &BTreeMap<String, String>,
    semantic_id: Option<&str>,
    findings: &mut Vec<Finding>,
) {
    let mismatch = |detail: String, findings: &mut Vec<Finding>| {
        findings.push(Finding::new(
            "operations.type-mismatch",
            semantic_id,
            Some("/recipe"),
            detail,
        ));
    };
    match operand {
        Operand::FromInput(field) => match input_types.get(field) {
            Some(spelling) if spelling == target => {}
            Some(spelling) => mismatch(
                format!(
                    "the input field `{field}` carries `{spelling}`, the target needs `{target}`"
                ),
                findings,
            ),
            None => mismatch(format!("`{field}` is not a command input field"), findings),
        },
        Operand::FromEntity(field) => match entity_types.get(field) {
            Some(spelling) if spelling == target => {}
            Some(spelling) => mismatch(
                format!(
                    "the entity field `{field}` carries `{spelling}`, the target needs `{target}`"
                ),
                findings,
            ),
            None => mismatch(format!("`{field}` is not an entity field"), findings),
        },
        Operand::EnumCase { enum_id, .. } => {
            if target != enum_id {
                mismatch(
                    format!("the enum case targets `{enum_id}`, the field carries `{target}`"),
                    findings,
                );
            }
        }
        Operand::Literal(literal) => {
            // A literal must carry a plain scalar spelling; v0.4.0 keeps
            // literals honest: a literal alone cannot prove uuid, date,
            // or uri shape, so those bases are unsupported elsewhere and
            // a literal never matches a definition reference.
            let spelled = match literal {
                Value::String(_) => "#string".to_owned(),
                Value::Bool(_) => "#boolean".to_owned(),
                Value::Number(number) if number.is_i64() => "#int".to_owned(),
                _ => String::new(),
            };
            if spelled != target {
                mismatch(
                    format!("a literal operand cannot carry the definition target `{target}`"),
                    findings,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_vocabulary_helpers() {
        assert!(is_semantic_id("planner.focus_task"));
        assert!(!is_semantic_id("planner"));
        assert!(!is_semantic_id("Planner.focus_task"));
        assert!(is_php_fqn(
            "Lekalo\\Generated\\Operations\\Planner\\FocusTaskHandler"
        ));
        assert!(!is_php_fqn("FocusTaskHandler"));
        assert!(is_logical_path("planner/focus_task/handler.php"));
        assert!(!is_logical_path("Planner/FocusTask.php"));
        assert!(!is_logical_path("../escape.php"));
        assert!(is_sha256_digest(
            "sha256:0000000000000000000000000000000000000000000000000000000000000000"
        ));
        assert!(!is_sha256_digest(
            "sha256:000000000000000000000000000000000000000000000000000000000000000Z"
        ));
    }

    #[test]
    fn mode_helpers() {
        assert!(Mode::Checked.requires_entry());
        assert!(Mode::Custom.requires_entry());
        assert!(!Mode::Managed.requires_entry());
        assert!(Mode::Managed.emits());
        assert!(Mode::ScaffoldOnce.emits());
        assert!(!Mode::Checked.emits());
        assert!(!Mode::Custom.emits());
    }

    #[test]
    fn operand_parsing_is_closed() {
        let value: Value = serde_json::from_str(r#"{"fromInput": "task_id"}"#).expect("operand");
        assert_eq!(
            parse_operand(Some(&value)),
            Some(Operand::FromInput("task_id".to_owned()))
        );
        let value: Value = serde_json::from_str(r#"{"now": true}"#).expect("operand");
        assert_eq!(parse_operand(Some(&value)), None, "clock operands refuse");
        let value: Value = serde_json::from_str(
            r#"{"enumCase": {"type": "planner.task_state", "value": "focused"}}"#,
        )
        .expect("operand");
        assert!(parse_operand(Some(&value)).is_some());
    }

    #[test]
    fn input_identity_refuses_a_foreign_contract() {
        let bytes = br#"{"schemaVersion":"lekalo/php-operations-input/v9.9.9","identity":"x"}"#;
        let findings = parse_input(bytes).expect_err("foreign identity");
        assert_eq!(findings[0].code, "operations.input-identity");
    }

    #[test]
    fn input_order_is_canonical() {
        let bytes = br#"{
            "schemaVersion": "lekalo/php-operations-input/v0.4.0",
            "identity": "dev.lekalo.php-operations-input@0.4.0",
            "projectId": "planner",
            "irDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "typesInputDigest": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "operations": [
                {"id": "planner.zed", "kind": "query", "mode": "checked",
                 "entry": {"fqn": "App\\Zed\\ZedHandler", "method": "handle", "path": "app/zed.php"}},
                {"id": "planner.abc", "kind": "query", "mode": "checked",
                 "entry": {"fqn": "App\\Abc\\AbcHandler", "method": "handle", "path": "app/abc.php"}}
            ]
        }"#;
        let findings = parse_input(bytes).expect_err("misordered");
        assert!(findings
            .iter()
            .any(|finding| finding.code == "operations.input-order"));
    }
}
