//! The PHP routes join (issue #60).
//!
//! This module is the core-side acceptance authority for the bounded
//! `dev.lekalo.php-routes-input@0.4.0` document authored under
//! `lekalo/routes/<project>.routes.json`. It validates the closed input
//! shape and canonical order, then joins every declared route against
//! exactly four owned authorities — never its own guesses:
//!
//! 1. the compiled project IR (the endpoint symbol with its method,
//!    path template, and `invokes`, and the invoked command/query
//!    definition);
//! 2. the transport-http attachment (`lekalo/transport.yaml`), which
//!    owns the whole wire surface; the join re-validates it against the
//!    compiled project with the embedded #62 error registry bound and
//!    digest-compares its canonical bytes against both the input pin
//!    and the staged evidence home;
//! 3. the staged IR evidence bytes (`.lekalo/cache/ir/<project>.json`)
//!    and the bound #58 types input and #59 operations input documents
//!    by exact digest — a route wrapper without its handler join is
//!    dead code;
//! 4. the closed per-route custody grammar (`managed` derives the
//!    generated wrapper; `checked` names the existing entrypoint the
//!    scanner evidence must join).
//!
//! Method, path, parameters, body, success/error projections, security,
//! headers, and scenario links stay single-sourced in the Model and the
//! attachment: this input only names the endpoint, its invoked
//! operation, and the custody mode. A missing context is a typed
//! finding; absence is never read as a default. Every refusal happens
//! before any generated source write.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::ir::{CompiledProject, Definition};
use crate::transport_http::{self, TransportDocument};

/// The closed identity members of the v0.4.0 input contract.
pub const INPUT_SCHEMA_VERSION: &str = "lekalo/php-routes-input/v0.4.0";
pub const INPUT_IDENTITY: &str = "dev.lekalo.php-routes-input@0.4.0";

/// The closed default namespace prefix.
pub const DEFAULT_NAMESPACE_PREFIX: &str = "Lekalo\\Generated\\Routes";

/// The staged IR evidence home the adapter (and this join) reads.
pub const IR_EVIDENCE_HOME: &str = ".lekalo/cache/ir";

/// The bound types input home of the #58 family.
pub const TYPES_INPUT_HOME: &str = "lekalo/types";

/// The bound operations input home of the #59 family.
pub const OPERATIONS_INPUT_HOME: &str = "lekalo/operations";

/// One typed join finding. `pointer` is the input-relative JSON pointer
/// of the offending member when one exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    /// The closed bounded reason token (`routes.transport-unbound`).
    pub code: String,
    /// The route semantic id, when bound to one record.
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

/// The closed per-route custody mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// The generated, regenerable thin wrapper under the closed
    /// generated root.
    Managed,
    /// No writes; the declared surface joins against the observed
    /// routes evidence.
    Checked,
}

impl Mode {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "managed" => Some(Self::Managed),
            "checked" => Some(Self::Checked),
            _ => None,
        }
    }

    /// Whether the mode requires the declared native entrypoint.
    pub fn requires_entry(self) -> bool {
        matches!(self, Self::Checked)
    }

    /// Whether the mode emits any write.
    pub fn emits(self) -> bool {
        matches!(self, Self::Managed)
    }
}

/// The declared existing controller entrypoint of a checked route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub fqn: String,
    pub method: String,
}

/// One declared scheme-to-middleware mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiddlewareBinding {
    pub scheme: String,
    pub middleware: String,
}

/// One validated route record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteRecord {
    pub id: String,
    pub operation: String,
    pub mode: Mode,
    pub entry: Option<Entry>,
}

/// The validated routes input document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutesInput {
    pub project_id: String,
    pub ir_digest: String,
    pub transport_digest: String,
    pub types_input_digest: String,
    pub operations_input_digest: String,
    pub namespace_prefix: String,
    pub middleware: Vec<MiddlewareBinding>,
    pub routes: Vec<RouteRecord>,
}

fn is_sha256_digest(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte: u8| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

fn is_semantic_id(value: &str) -> bool {
    value.split('.').all(is_lower_snake) && value.contains('.') && !value.starts_with('.')
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

fn is_camel_method(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && value.len() <= 64
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
/// (id-sorted, unique routes) is part of the shape: a misordered or
/// duplicated document is a finding, never a silent reorder.
pub fn parse_input(bytes: &[u8]) -> Result<RoutesInput, Vec<Finding>> {
    let mut findings = Vec::new();
    let document: Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(_) => {
            return Err(vec![Finding::new(
                "routes.input-shape",
                None,
                None,
                "the routes input is not valid JSON",
            )]);
        }
    };
    if !document.is_object() {
        return Err(vec![Finding::new(
            "routes.input-shape",
            None,
            None,
            "the routes input is not a JSON object",
        )]);
    }
    if !members_closed(
        document.as_object().expect("checked object"),
        &[
            "schemaVersion",
            "identity",
            "projectId",
            "irDigest",
            "transportDigest",
            "typesInputDigest",
            "operationsInputDigest",
            "policy",
            "routes",
        ],
    ) {
        return Err(vec![Finding::new(
            "routes.input-shape",
            None,
            None,
            "the routes input carries an unknown member",
        )]);
    }
    if document.get("schemaVersion").and_then(Value::as_str) != Some(INPUT_SCHEMA_VERSION)
        || document.get("identity").and_then(Value::as_str) != Some(INPUT_IDENTITY)
    {
        return Err(vec![Finding::new(
            "routes.input-identity",
            None,
            None,
            "the routes input does not carry the closed v0.4.0 identity",
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
            "routes.input-shape",
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
    let transport_digest = document
        .get("transportDigest")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let types_input_digest = document
        .get("typesInputDigest")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let operations_input_digest = document
        .get("operationsInputDigest")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    for (name, digest) in [
        ("/irDigest", &ir_digest),
        ("/transportDigest", &transport_digest),
        ("/typesInputDigest", &types_input_digest),
        ("/operationsInputDigest", &operations_input_digest),
    ] {
        if !is_sha256_digest(digest) {
            findings.push(Finding::new(
                "routes.input-shape",
                None,
                Some(name),
                "digests are canonical `sha256:<64 lowercase hex>` spellings",
            ));
        }
    }
    let mut namespace_prefix = DEFAULT_NAMESPACE_PREFIX.to_owned();
    let mut middleware = Vec::new();
    match document.get("policy") {
        None | Some(Value::Null) => {}
        Some(policy) => match policy.as_object() {
            None => findings.push(Finding::new(
                "routes.input-shape",
                None,
                Some("/policy"),
                "the routes policy carries no known member",
            )),
            Some(policy) => {
                if !members_closed(policy, &["namespacePrefix", "middleware"]) {
                    findings.push(Finding::new(
                        "routes.input-shape",
                        None,
                        Some("/policy"),
                        "the routes policy carries an unknown member",
                    ));
                }
                match policy.get("namespacePrefix") {
                    Some(prefix) => match prefix.as_str() {
                        Some(prefix) if is_php_fqn(prefix) => {
                            namespace_prefix = prefix.to_owned();
                        }
                        _ => findings.push(Finding::new(
                            "routes.input-shape",
                            None,
                            Some("/policy/namespacePrefix"),
                            "the namespace prefix must be a bounded PHP FQN spelling",
                        )),
                    },
                    None => findings.push(Finding::new(
                        "routes.input-shape",
                        None,
                        Some("/policy"),
                        "the routes policy requires namespacePrefix",
                    )),
                }
                match policy.get("middleware") {
                    None | Some(Value::Null) => {}
                    Some(Value::Array(bindings)) => {
                        if bindings.len() > 16 {
                            findings.push(Finding::new(
                                "routes.input-shape",
                                None,
                                Some("/policy/middleware"),
                                "at most 16 middleware mappings are bounded",
                            ));
                        }
                        for (index, binding) in bindings.iter().enumerate() {
                            match parse_middleware_binding(binding) {
                                Ok(binding) => middleware.push(binding),
                                Err(()) => findings.push(Finding::new(
                                    "routes.input-shape",
                                    None,
                                    Some(&format!("/policy/middleware/{index}")),
                                    "a middleware mapping names one declared scheme and one bounded middleware spelling",
                                )),
                            }
                        }
                        let mut schemes: Vec<&str> =
                            middleware.iter().map(|b| b.scheme.as_str()).collect();
                        schemes.sort_unstable();
                        schemes.dedup();
                        if schemes.len() != middleware.len() {
                            findings.push(Finding::new(
                                "routes.input-order",
                                None,
                                Some("/policy/middleware"),
                                "middleware mappings are canonical: one entry per scheme",
                            ));
                        }
                    }
                    Some(_) => findings.push(Finding::new(
                        "routes.input-shape",
                        None,
                        Some("/policy/middleware"),
                        "middleware mappings are an array",
                    )),
                }
            }
        },
    }
    let Some(routes) = document.get("routes").and_then(Value::as_array) else {
        findings.push(Finding::new(
            "routes.input-shape",
            None,
            Some("/routes"),
            "routes is a required non-empty array",
        ));
        return Err(findings);
    };
    if routes.is_empty() {
        findings.push(Finding::new(
            "routes.input-shape",
            None,
            Some("/routes"),
            "routes is a required non-empty array",
        ));
    }
    if routes.len() > 2048 {
        findings.push(Finding::new(
            "routes.input-limit",
            None,
            Some("/routes"),
            "at most 2048 route records are bounded",
        ));
    }
    let mut records = Vec::new();
    for (index, value) in routes.iter().enumerate() {
        match parse_route(value) {
            Ok(record) => records.push(record),
            Err(mut record_findings) => {
                for finding in &mut record_findings {
                    if finding.pointer.is_none() {
                        finding.pointer = Some(format!("/routes/{index}"));
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
            "routes.input-order",
            None,
            Some("/routes"),
            "route records are canonical: sorted by id, duplicates refuse",
        ));
    }
    if !findings.is_empty() {
        return Err(findings);
    }
    Ok(RoutesInput {
        project_id,
        ir_digest,
        transport_digest,
        types_input_digest,
        operations_input_digest,
        namespace_prefix,
        middleware,
        routes: records,
    })
}

/// One closed middleware mapping.
fn parse_middleware_binding(value: &Value) -> Result<MiddlewareBinding, ()> {
    let Some(object) = value.as_object() else {
        return Err(());
    };
    if !members_closed(object, &["scheme", "middleware"]) {
        return Err(());
    }
    let scheme = object.get("scheme").and_then(Value::as_str).ok_or(())?;
    if !is_lower_snake(scheme) || scheme.len() > 64 {
        return Err(());
    }
    let middleware = object.get("middleware").and_then(Value::as_str).ok_or(())?;
    let mut chars = middleware.chars();
    match chars.next() {
        Some(first) if first.is_ascii_uppercase() || first.is_ascii_lowercase() => {}
        _ => return Err(()),
    }
    if middleware.len() > 128
        || !middleware
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '-'))
    {
        return Err(());
    }
    Ok(MiddlewareBinding {
        scheme: scheme.to_owned(),
        middleware: middleware.to_owned(),
    })
}

/// One closed route record.
fn parse_route(value: &Value) -> Result<RouteRecord, Vec<Finding>> {
    let Some(object) = value.as_object() else {
        return Err(vec![Finding::new(
            "routes.input-shape",
            None,
            None,
            "a route record is a JSON object",
        )]);
    };
    if !members_closed(object, &["id", "operation", "mode", "entry"]) {
        return Err(vec![Finding::new(
            "routes.input-shape",
            None,
            None,
            "the route record carries an unknown member",
        )]);
    }
    let id = object.get("id").and_then(Value::as_str).unwrap_or_default();
    if !is_semantic_id(id) {
        return Err(vec![Finding::new(
            "routes.input-shape",
            None,
            Some("/id"),
            "the route id is the dotted endpoint symbol",
        )]);
    }
    let operation = object
        .get("operation")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_semantic_id(operation) {
        return Err(vec![Finding::new(
            "routes.input-shape",
            Some(id),
            Some("/operation"),
            "the operation is the dotted command/query symbol",
        )]);
    }
    let mode = match object
        .get("mode")
        .and_then(Value::as_str)
        .and_then(Mode::parse)
    {
        Some(mode) => mode,
        None => {
            return Err(vec![Finding::new(
                "routes.input-shape",
                Some(id),
                Some("/mode"),
                "the mode is `managed` or `checked`",
            )]);
        }
    };
    let entry = match object.get("entry") {
        None | Some(Value::Null) => None,
        Some(entry) => match parse_entry(entry) {
            Ok(entry) => Some(entry),
            Err(()) => {
                return Err(vec![Finding::new(
                    "routes.input-shape",
                    Some(id),
                    Some("/entry"),
                    "the entry is one bounded PHP FQN and one public method spelling",
                )]);
            }
        },
    };
    Ok(RouteRecord {
        id: id.to_owned(),
        operation: operation.to_owned(),
        mode,
        entry,
    })
}

/// One declared checked entrypoint.
fn parse_entry(value: &Value) -> Result<Entry, ()> {
    let Some(object) = value.as_object() else {
        return Err(());
    };
    if !members_closed(object, &["fqn", "method"]) {
        return Err(());
    }
    let fqn = object.get("fqn").and_then(Value::as_str).ok_or(())?;
    if !is_php_fqn(fqn) {
        return Err(());
    }
    let method = object.get("method").and_then(Value::as_str).ok_or(())?;
    if !is_camel_method(method) {
        return Err(());
    }
    Ok(Entry {
        fqn: fqn.to_owned(),
        method: method.to_owned(),
    })
}

/// The staged-evidence digest context: exact bytes, exact digests.
pub struct DigestContext {
    pub ir_digest: String,
    pub transport_digest: String,
    pub types_input_digest: String,
    pub operations_input_digest: String,
}

/// Read the digest context of one project root: the staged IR evidence,
/// the bound types and operations inputs, and the canonical transport
/// attachment bytes, digested exactly. A missing or unreadable transport
/// home yields `None` — the join refuses, never guesses.
pub fn digest_context(root: &Path, project_id: &str) -> Option<DigestContext> {
    use crate::digest::sha256_hex;
    let ir_bytes =
        std::fs::read(root.join(format!("{IR_EVIDENCE_HOME}/{project_id}.json"))).ok()?;
    let types_bytes =
        std::fs::read_to_string(root.join(format!("{TYPES_INPUT_HOME}/{project_id}.types.json")))
            .ok()?;
    let operations_bytes = std::fs::read_to_string(root.join(format!(
        "{OPERATIONS_INPUT_HOME}/{project_id}.operations.json"
    )))
    .ok()?;
    let attachment = transport_http::read_document(root).ok()??;
    let transport_bytes = attachment.canonical_bytes().ok()?;
    Some(DigestContext {
        ir_digest: format!("sha256:{}", sha256_hex(&ir_bytes)),
        transport_digest: format!("sha256:{}", sha256_hex(transport_bytes.as_bytes())),
        types_input_digest: format!("sha256:{}", sha256_hex(types_bytes.as_bytes())),
        operations_input_digest: format!("sha256:{}", sha256_hex(operations_bytes.as_bytes())),
    })
}

/// Join one parsed input against the compiled IR, the transport
/// attachment (re-validated with the embedded #62 error registry
/// bound), and the staged evidence digests. Returns every finding; an
/// empty set is the accepted join.
pub fn check_join(root: &Path, input: &RoutesInput, compilation: &CompiledProject) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(digests) = digest_context(root, &input.project_id) else {
        findings.push(Finding::new(
            "routes.evidence-unavailable",
            None,
            None,
            "the staged IR evidence, the transport home, or a bound types/operations input is missing",
        ));
        return findings;
    };
    if digests.ir_digest != input.ir_digest {
        findings.push(Finding::new(
            "routes.ir-digest",
            None,
            Some("/irDigest"),
            "the input names different IR evidence bytes than the staged home",
        ));
    }
    if digests.transport_digest != input.transport_digest {
        findings.push(Finding::new(
            "routes.transport-digest",
            None,
            Some("/transportDigest"),
            "the input names different transport attachment bytes than the canonical home",
        ));
    }
    if digests.types_input_digest != input.types_input_digest {
        findings.push(Finding::new(
            "routes.types-unbound",
            None,
            Some("/typesInputDigest"),
            "the input names different types-input bytes than the committed document",
        ));
    }
    if digests.operations_input_digest != input.operations_input_digest {
        findings.push(Finding::new(
            "routes.operations-unbound",
            None,
            Some("/operationsInputDigest"),
            "the input names different operations-input bytes than the committed document",
        ));
    }
    // The wire authority: the attachment re-validated against the
    // compiled project with the embedded error registry bound, then
    // digest-joined.
    let attachment = match transport_http::read_document(root) {
        Err(_) => {
            findings.push(Finding::new(
                "routes.transport-unreadable",
                None,
                None,
                "the transport home is present but unreadable",
            ));
            return findings;
        }
        Ok(None) => {
            findings.push(Finding::new(
                "routes.transport-unbound",
                None,
                None,
                "the project declares no transport home; no route can join the wire",
            ));
            return findings;
        }
        Ok(Some(attachment)) => attachment,
    };
    if attachment.project_id().as_str() != input.project_id {
        findings.push(Finding::new(
            "routes.transport-project-mismatch",
            None,
            None,
            "the transport attachment binds a different project id",
        ));
    }
    let registry = match crate::error_contract::ErrorRegistry::embedded() {
        Ok(registry) => registry,
        Err(_) => {
            findings.push(Finding::new(
                "routes.registry-invalid",
                None,
                None,
                "the embedded error registry failed to validate",
            ));
            return findings;
        }
    };
    let context = transport_http::ValidationContext::new(compilation).with_errors(registry);
    if let Err(diagnostics) = transport_http::validate(&attachment, &context) {
        let reasons = diagnostics.reason_ids().join(",");
        findings.push(Finding::new(
            "routes.transport-invalid",
            None,
            None,
            format!(
                "the transport attachment does not validate against the compiled project: {reasons}"
            ),
        ));
        return findings;
    }
    if attachment.ir_digest().as_str() != digests.ir_digest {
        findings.push(Finding::new(
            "routes.transport-ir-mismatch",
            None,
            None,
            "the transport attachment binds different IR bytes than the staged evidence",
        ));
    }
    // The middleware schemes must all be declared transport schemes.
    for binding in &input.middleware {
        if attachment.scheme(&binding.scheme).is_none() {
            findings.push(Finding::new(
                "routes.scheme-unresolved",
                None,
                Some("/policy/middleware"),
                format!(
                    "the middleware mapping names `{}`, which the attachment never declares",
                    binding.scheme
                ),
            ));
        }
    }
    let ir = IrContext::of(compilation);
    for record in &input.routes {
        check_record(record, &ir, &attachment, &mut findings);
    }
    findings
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

/// One route record's full semantic join.
fn check_record(
    record: &RouteRecord,
    ir: &IrContext<'_>,
    attachment: &TransportDocument,
    findings: &mut Vec<Finding>,
) {
    let reference = Some(record.id.as_str());
    let Some(Definition::Endpoint(endpoint)) = ir.definitions.get(record.id.as_str()).copied()
    else {
        findings.push(Finding::new(
            "routes.endpoint-unresolved",
            reference,
            None,
            "the route id is not a compiled IR endpoint definition",
        ));
        return;
    };
    if endpoint.invokes.as_str() != record.operation {
        findings.push(Finding::new(
            "routes.invokes-mismatch",
            reference,
            Some("/operation"),
            format!(
                "the endpoint invokes `{}`, the record declares `{}`",
                endpoint.invokes.as_str(),
                record.operation
            ),
        ));
    }
    match ir.definitions.get(record.operation.as_str()).copied() {
        Some(Definition::Command(_)) | Some(Definition::Query(_)) => {}
        _ => findings.push(Finding::new(
            "routes.operation-unresolved",
            reference,
            Some("/operation"),
            "the invoked operation is not a compiled command or query definition",
        )),
    }
    // Mode/entry pairing.
    if record.mode.requires_entry() != record.entry.is_some() {
        findings.push(Finding::new(
            "routes.entry-required",
            reference,
            Some("/entry"),
            if record.mode.requires_entry() {
                "checked records declare their existing controller entrypoint"
            } else {
                "managed records never declare a foreign entrypoint"
            },
        ));
    }
    // The wire binding: the attachment must declare this endpoint.
    if attachment.endpoint(record.id.as_str()).is_none() {
        findings.push(Finding::new(
            "routes.transport-unbound",
            reference,
            None,
            "the transport attachment binds no wire surface for this endpoint",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_constants_are_the_closed_v040_family() {
        assert_eq!(INPUT_SCHEMA_VERSION, "lekalo/php-routes-input/v0.4.0");
        assert_eq!(INPUT_IDENTITY, "dev.lekalo.php-routes-input@0.4.0");
        assert_eq!(DEFAULT_NAMESPACE_PREFIX, "Lekalo\\Generated\\Routes");
    }

    #[test]
    fn mode_pairing_holds() {
        assert!(Mode::Checked.requires_entry());
        assert!(!Mode::Managed.requires_entry());
        assert!(Mode::Managed.emits());
        assert!(!Mode::Checked.emits());
        assert_eq!(Mode::parse("managed"), Some(Mode::Managed));
        assert_eq!(Mode::parse("checked"), Some(Mode::Checked));
        assert_eq!(Mode::parse("scaffold-once"), None);
    }

    #[test]
    fn grammars_refuse_the_foreign_spellings() {
        assert!(is_semantic_id("planner.endpoint_focus_task"));
        assert!(!is_semantic_id("planner"));
        assert!(!is_semantic_id("Planner.endpoint"));
        assert!(is_php_fqn(
            "Lekalo\\Generated\\Routes\\Planner\\TodayController"
        ));
        assert!(!is_php_fqn("relative\\Name"));
        assert!(!is_php_fqn("Lekalo\\generated\\routes"));
        assert!(is_camel_method("focusTask"));
        assert!(!is_camel_method("FocusTask"));
        assert!(is_sha256_digest(&format!("sha256:{}", "a".repeat(64))));
        assert!(!is_sha256_digest("sha256:abc"));
    }

    #[test]
    fn a_minimal_input_parses_into_the_closed_shape() {
        let input = serde_json::json!({
            "schemaVersion": INPUT_SCHEMA_VERSION,
            "identity": INPUT_IDENTITY,
            "projectId": "planner",
            "irDigest": format!("sha256:{}", "a".repeat(64)),
            "transportDigest": format!("sha256:{}", "b".repeat(64)),
            "typesInputDigest": format!("sha256:{}", "c".repeat(64)),
            "operationsInputDigest": format!("sha256:{}", "d".repeat(64)),
            "routes": [
                {"id": "planner.endpoint_focus", "operation": "planner.focus_task", "mode": "checked",
                 "entry": {"fqn": "App\\Http\\Controllers\\TaskFocusController", "method": "focus"}},
                {"id": "planner.endpoint_today", "operation": "planner.today", "mode": "managed"},
            ],
        });
        let parsed = parse_input(serde_json::to_vec(&input).unwrap().as_slice())
            .expect("the minimal input parses");
        assert_eq!(parsed.project_id, "planner");
        assert_eq!(parsed.namespace_prefix, DEFAULT_NAMESPACE_PREFIX);
        assert_eq!(parsed.routes.len(), 2);
        assert_eq!(parsed.routes[0].mode, Mode::Checked);
        assert_eq!(parsed.routes[0].entry.as_ref().unwrap().method, "focus");
        assert_eq!(parsed.routes[1].mode, Mode::Managed);
    }

    #[test]
    fn unknown_members_and_broken_order_refuse() {
        let zeros = |c: char| format!("sha256:{}", c.to_string().repeat(64));
        let base = serde_json::json!({
            "schemaVersion": INPUT_SCHEMA_VERSION,
            "identity": INPUT_IDENTITY,
            "projectId": "planner",
            "irDigest": zeros('a'),
            "transportDigest": zeros('b'),
            "typesInputDigest": zeros('c'),
            "operationsInputDigest": zeros('d'),
            "routes": [{"id": "planner.endpoint_today", "operation": "planner.today", "mode": "managed"}],
        });
        let mut unknown = base.clone();
        unknown["custody"] = serde_json::json!("managed");
        assert!(parse_input(serde_json::to_vec(&unknown).unwrap().as_slice()).is_err());
        let mut misordered = base.clone();
        misordered["routes"] = serde_json::json!([
            {"id": "planner.endpoint_today", "operation": "planner.today", "mode": "managed"},
            {"id": "planner.endpoint_focus", "operation": "planner.focus_task", "mode": "checked",
             "entry": {"fqn": "App\\Http\\Controllers\\C", "method": "focus"}},
        ]);
        let refused = parse_input(serde_json::to_vec(&misordered).unwrap().as_slice()).unwrap_err();
        assert!(refused
            .iter()
            .any(|finding| finding.code == "routes.input-order"));
        let mut duplicated = base.clone();
        duplicated["routes"] = serde_json::json!([
            {"id": "planner.endpoint_today", "operation": "planner.today", "mode": "managed"},
            {"id": "planner.endpoint_today", "operation": "planner.today", "mode": "managed"},
        ]);
        let refused = parse_input(serde_json::to_vec(&duplicated).unwrap().as_slice()).unwrap_err();
        assert!(refused
            .iter()
            .any(|finding| finding.code == "routes.input-order"));
        let mut foreign_identity = base;
        foreign_identity["schemaVersion"] = serde_json::json!("lekalo/php-routes-input/v0.4.1");
        let refused =
            parse_input(serde_json::to_vec(&foreign_identity).unwrap().as_slice()).unwrap_err();
        assert!(refused
            .iter()
            .any(|finding| finding.code == "routes.input-identity"));
    }
}
