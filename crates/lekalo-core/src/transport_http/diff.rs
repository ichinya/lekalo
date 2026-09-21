//! Pure semantic comparison of two same-family attachments (issue
//! #70).
//!
//! The comparison answers one question per changed path with one
//! closed class: **breaking** (a declared wire guarantee was removed
//! or narrowed — an endpoint, parameter, error entry, security
//! binding, pagination shape, or capability disappeared; a required
//! parameter or body field appeared; a parameter's serialization
//! style changed; a security scheme's contents changed in place; an
//! inherited header name was renamed), **non-breaking** (an addition
//! under the evolution policy, metadata), and **policy-change**
//! (status, rate-limit, cache, idempotency, correlation,
//! content-version, security-strengthening, capability detail, and
//! operation-id changes
//! that reshape the projection without removing a guarantee — HTTP
//! statuses are projections of the #62 identity, never the identity
//! itself, so a status change is policy, not breakage). Parameters are
//! keyed by `(name, location)`: a location move is the removal of one
//! wire binding plus the addition of another, never silent. Request
//! and response bodies compare their declared field subsets
//! member-by-member. Every endpoint member and every document-level
//! default is classified; a defensive catch-all refuses the diff when
//! the canonical bytes differ but no path was emitted, so a changed
//! member can never report a false `equal`. Invalid inputs — foreign
//! projects or mixed Model/IR pins — are the typed error set, never a
//! guessed classification. Paths are deterministic and byte-sorted.

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;
use super::types::EndpointBinding;
use super::TransportDocument;

/// The closed compatibility class of one changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum DiffClass {
    /// A wire guarantee was removed or narrowed.
    Breaking,
    /// An addition or descriptive change under the evolution policy.
    NonBreaking,
    /// A projection or policy change with the endpoint still declared.
    PolicyChange,
}

impl DiffClass {
    /// The exact wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Breaking => "breaking",
            Self::NonBreaking => "non-breaking",
            Self::PolicyChange => "policy-change",
        }
    }
}

/// One changed path with its class.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DiffPath {
    path: String,
    class: DiffClass,
}

impl DiffPath {
    /// The canonical path spelling.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The closed compatibility class.
    pub const fn class(&self) -> DiffClass {
        self.class
    }
}

/// The finished comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffResult {
    equal: bool,
    paths: Vec<DiffPath>,
}

impl DiffResult {
    /// Whether the two attachments are semantically equal.
    pub const fn equal(&self) -> bool {
        self.equal
    }

    /// The changed paths, byte-sorted.
    pub fn paths(&self) -> &[DiffPath] {
        &self.paths
    }

    /// Whether a strict `wire-consumer` profile blocks this diff: any
    /// breaking classification blocks.
    pub fn wire_consumer_blocked(&self) -> bool {
        self.paths
            .iter()
            .any(|path| path.class() == DiffClass::Breaking)
    }
}

/// Compare two same-family attachments. Pure and read-only.
pub fn compare(
    base: &TransportDocument,
    candidate: &TransportDocument,
) -> Result<DiffResult, DiagnosticSet> {
    if base.project_id().as_str() != candidate.project_id().as_str() {
        return Err(diagnostic::input_invalid("diff-project-mismatch"));
    }
    if base.model_ref().digest().as_str() != candidate.model_ref().digest().as_str()
        || base.ir_digest().as_str() != candidate.ir_digest().as_str()
    {
        return Err(diagnostic::input_invalid("diff-mixed-pins"));
    }
    if base.wire().dialect != candidate.wire().dialect {
        return Err(diagnostic::input_invalid("diff-mixed-generation"));
    }
    let mut paths: Vec<DiffPath> = Vec::new();

    // Document defaults: the inherited header names are wire-visible
    // surface (an idempotency rename loses the declared guarantee;
    // a dropped correlation header loses a declared channel).
    if base.defaults().idempotency_header.as_str()
        != candidate.defaults().idempotency_header.as_str()
    {
        push_path(
            "defaults/idempotencyHeader",
            DiffClass::Breaking,
            &mut paths,
        );
    }
    if base.defaults().correlation_headers != candidate.defaults().correlation_headers {
        let removed = base.defaults().correlation_headers.iter().any(|header| {
            !candidate
                .defaults()
                .correlation_headers
                .iter()
                .any(|other| other == header)
        });
        push_path(
            "defaults/correlationHeaders",
            if removed {
                DiffClass::Breaking
            } else {
                DiffClass::NonBreaking
            },
            &mut paths,
        );
    }

    // Security schemes: removal is breaking (an endpoint may lose its
    // declared scheme); addition is policy (a new option); an
    // in-place content change breaks (the credential mechanics the
    // endpoints declared changed under the same id).
    for scheme in base.schemes() {
        match candidate.scheme(scheme.id.as_str()) {
            None => push_path(
                &format!("securitySchemes/{}", scheme.id.as_str()),
                DiffClass::Breaking,
                &mut paths,
            ),
            Some(other) if other != scheme => push_path(
                &format!("securitySchemes/{}", scheme.id.as_str()),
                DiffClass::Breaking,
                &mut paths,
            ),
            _ => {}
        }
    }
    for scheme in candidate.schemes() {
        if base.scheme(scheme.id.as_str()).is_none() {
            push_path(
                &format!("securitySchemes/{}", scheme.id.as_str()),
                DiffClass::PolicyChange,
                &mut paths,
            );
        }
    }

    // Endpoints: removal is breaking, addition is non-breaking, and
    // every member change of a common endpoint is classified below.
    for binding in base.endpoints() {
        match candidate.endpoint(binding.endpoint.as_str()) {
            Some(other) => compare_endpoint(binding, other, &mut paths),
            None => push_path(
                &format!("endpoints/{}", binding.endpoint.as_str()),
                DiffClass::Breaking,
                &mut paths,
            ),
        }
    }
    for binding in candidate.endpoints() {
        if base.endpoint(binding.endpoint.as_str()).is_none() {
            push_path(
                &format!("endpoints/{}", binding.endpoint.as_str()),
                DiffClass::NonBreaking,
                &mut paths,
            );
        }
    }
    // Defensive custody: every classified change emitted its path, so
    // the canonical bytes may only differ when a member escaped the
    // classifier — refuse the diff instead of reporting a false
    // equal (plan §3.5: an unplaceable member refuses the diff).
    if paths.is_empty() && base.canonical_bytes()? != candidate.canonical_bytes()? {
        return Err(diagnostic::input_invalid("diff-unclassified"));
    }
    paths.sort();
    Ok(DiffResult {
        equal: paths.is_empty(),
        paths,
    })
}

/// Classify every changed member of one common endpoint.
fn compare_endpoint(
    base: &EndpointBinding,
    candidate: &EndpointBinding,
    paths: &mut Vec<DiffPath>,
) {
    let subject = base.endpoint.as_str();
    let at = |member: &str| format!("endpoints/{subject}/{member}");

    // Parameters: keyed by `(name, location)` — a location move is
    // the removal of one wire binding plus the addition of another,
    // never silent. Removal breaks; a required addition breaks (a
    // caller without the parameter no longer decodes); an optional
    // addition is non-breaking; requirement and field-binding changes
    // are policy; a serialization-style or explode change breaks
    // (the wire encoding of values changed).
    let binding_key = |param: &super::types::ParamBinding| {
        (param.name.as_str().to_owned(), param.location.as_str().to_owned())
    };
    for param in &base.params {
        match candidate
            .params
            .iter()
            .find(|other| binding_key(other) == binding_key(param))
        {
            Some(other) => {
                if other.required != param.required || other.field != param.field {
                    push_path(&at("params"), DiffClass::PolicyChange, paths);
                }
                if other.style != param.style || other.explode != param.explode {
                    push_path(&at("params"), DiffClass::Breaking, paths);
                }
            }
            None => push_path(&at("params"), DiffClass::Breaking, paths),
        }
    }
    for param in &candidate.params {
        if !base
            .params
            .iter()
            .any(|other| binding_key(other) == binding_key(param))
        {
            let class = if param.required {
                DiffClass::Breaking
            } else {
                DiffClass::NonBreaking
            };
            push_path(&at("params"), class, paths);
        }
    }

    // Body: a narrowing (whole -> explicit) breaks; a widening is
    // non-breaking; the declared field subset is compared
    // member-by-member (a removal breaks, an optional addition does
    // not, a required addition breaks, a member rebinding is policy).
    match (&base.body, &candidate.body) {
        (Some(base_body), Some(other_body)) => {
            compare_body_pair(base_body, other_body, &at("body"), paths);
        }
        (Some(_), None) => push_path(&at("body"), DiffClass::Breaking, paths),
        (None, Some(_)) => push_path(&at("body"), DiffClass::NonBreaking, paths),
        (None, None) => {}
    }

    // Success: statuses are projections (the #62 doctrine), so a
    // status change is policy; the response body and the declared
    // response headers are wire surface compared like the request
    // side (a disappearing declared header breaks its consumers).
    if base.success.status != candidate.success.status {
        push_path(&at("success"), DiffClass::PolicyChange, paths);
    }
    match (&base.success.body, &candidate.success.body) {
        (Some(base_body), Some(other_body)) => {
            compare_body_pair(base_body, other_body, &at("success"), paths);
        }
        (Some(_), None) => push_path(&at("success"), DiffClass::Breaking, paths),
        (None, Some(_)) => push_path(&at("success"), DiffClass::NonBreaking, paths),
        (None, None) => {}
    }
    let header_key = |header: &super::types::ResponseHeader| header.name.as_str().to_owned();
    for header in &base.success.headers {
        match candidate
            .success
            .headers
            .iter()
            .find(|other| header_key(other) == header_key(header))
        {
            Some(other) => {
                if other != header {
                    push_path(&at("success"), DiffClass::PolicyChange, paths);
                }
            }
            None => push_path(&at("success"), DiffClass::Breaking, paths),
        }
    }
    for header in &candidate.success.headers {
        if !base
            .success
            .headers
            .iter()
            .any(|other| header_key(other) == header_key(header))
        {
            push_path(&at("success"), DiffClass::NonBreaking, paths);
        }
    }

    // The error map: entry removal breaks (a declared identity loses
    // its projection); additions and status changes are policy.
    for entry in &base.errors {
        let still = candidate
            .errors
            .iter()
            .find(|other| other.error == entry.error);
        match still {
            Some(other) => {
                if other.status != entry.status {
                    push_path(&at("errors"), DiffClass::PolicyChange, paths);
                }
            }
            None => push_path(&at("errors"), DiffClass::Breaking, paths),
        }
    }
    if candidate.errors.len() > base.errors.len() {
        push_path(&at("errors"), DiffClass::NonBreaking, paths);
    }
    if base.error_defaults != candidate.error_defaults {
        push_path(&at("errorDefaults"), DiffClass::PolicyChange, paths);
    }

    // Security: losing the binding breaks; every other change
    // (actor, scheme set, policy ref) is policy.
    match (&base.auth, &candidate.auth) {
        (Some(_), None) => push_path(&at("security"), DiffClass::Breaking, paths),
        (Some(base_auth), Some(other_auth)) => {
            if base_auth != other_auth {
                push_path(&at("security"), DiffClass::PolicyChange, paths);
            }
        }
        (None, Some(_)) => push_path(&at("security"), DiffClass::PolicyChange, paths),
        (None, None) => {}
    }

    // Idempotency, correlation, rate limit, cache, and content
    // versioning are policy declarations.
    if base.idempotency != candidate.idempotency {
        push_path(&at("idempotency"), DiffClass::PolicyChange, paths);
    }
    if base.correlation != candidate.correlation {
        push_path(&at("correlation"), DiffClass::PolicyChange, paths);
    }
    if base.rate_limit != candidate.rate_limit {
        push_path(&at("rateLimit"), DiffClass::PolicyChange, paths);
    }
    if base.cache != candidate.cache {
        push_path(&at("cache"), DiffClass::PolicyChange, paths);
    }
    if base.api_version != candidate.api_version {
        push_path(&at("apiVersion"), DiffClass::PolicyChange, paths);
    }

    // Pagination: a shape change breaks (a cursor continuation no
    // longer decodes against an offset window); appearance is
    // policy.
    match (&base.pagination, &candidate.pagination) {
        (Some(_), None) => push_path(&at("pagination"), DiffClass::Breaking, paths),
        (Some(_), Some(_)) => {
            if base.pagination != candidate.pagination {
                push_path(&at("pagination"), DiffClass::Breaking, paths);
            }
        }
        (None, Some(_)) => push_path(&at("pagination"), DiffClass::PolicyChange, paths),
        (None, None) => {}
    }

    // Capabilities: removal breaks (a consumer relying on the
    // streaming/upload/download channel loses it); additions and
    // minimum-support or detail changes are policy (a detail switch
    // reshapes the channel mechanics without removing the declared
    // capability).
    for decl in &base.capabilities {
        let still = candidate
            .capabilities
            .iter()
            .find(|other| other.capability == decl.capability);
        match still {
            Some(other) => {
                if other.minimum_support != decl.minimum_support
                    || other.detail != decl.detail
                {
                    push_path(&at("capabilities"), DiffClass::PolicyChange, paths);
                }
            }
            None => push_path(&at("capabilities"), DiffClass::Breaking, paths),
        }
    }
    if candidate.capabilities.len() > base.capabilities.len() {
        push_path(&at("capabilities"), DiffClass::NonBreaking, paths);
    }

    // Scenario coverage and operation identity are declaration
    // data: changes are policy, never silent.
    if base.scenarios != candidate.scenarios {
        push_path(&at("scenarios"), DiffClass::PolicyChange, paths);
    }
    if base.operation_id != candidate.operation_id {
        push_path(&at("operationId"), DiffClass::PolicyChange, paths);
    }
    if base.tags != candidate.tags || base.summary != candidate.summary {
        push_path(&at("metadata"), DiffClass::NonBreaking, paths);
    }
}

/// Classify one request/response body pair: a mode narrowing (whole
/// -> explicit) breaks, a widening does not, and the declared field
/// subsets compare member-by-member — a removal breaks, an optional
/// addition does not, a required addition breaks, and a member
/// rebinding is policy.
fn compare_body_pair(
    base: &super::types::BodyBinding,
    candidate: &super::types::BodyBinding,
    path: &str,
    paths: &mut Vec<DiffPath>,
) {
    if base.mode != candidate.mode {
        let class = match (base.mode, candidate.mode) {
            (super::types::ProjectionMode::Whole, super::types::ProjectionMode::Explicit) => {
                DiffClass::Breaking
            }
            (super::types::ProjectionMode::Explicit, super::types::ProjectionMode::Whole) => {
                DiffClass::NonBreaking
            }
            // Same mode: covered by the member comparison below.
            (super::types::ProjectionMode::Whole, super::types::ProjectionMode::Whole)
            | (super::types::ProjectionMode::Explicit, super::types::ProjectionMode::Explicit) => {
                return;
            }
        };
        push_path(path, class, paths);
    }
    let member = |field: &super::types::FieldProjection| field.name.as_str().to_owned();
    for field in &base.fields {
        match candidate.fields.iter().find(|other| member(other) == member(field)) {
            Some(other) => {
                if other != field {
                    push_path(path, DiffClass::PolicyChange, paths);
                }
            }
            None => push_path(path, DiffClass::Breaking, paths),
        }
    }
    for field in &candidate.fields {
        if !base.fields.iter().any(|other| member(other) == member(field)) {
            let class = if field.required {
                DiffClass::Breaking
            } else {
                DiffClass::NonBreaking
            };
            push_path(path, class, paths);
        }
    }
}

/// Push one path (deduplicated by path+class; the same member
/// reporting twice keeps its hardest class).
fn push_path(path: &str, class: DiffClass, paths: &mut Vec<DiffPath>) {
    if let Some(existing) = paths.iter_mut().find(|entry| entry.path == path) {
        if class == DiffClass::Breaking {
            existing.class = DiffClass::Breaking;
        }
        return;
    }
    paths.push(DiffPath {
        path: path.to_owned(),
        class,
    });
}
