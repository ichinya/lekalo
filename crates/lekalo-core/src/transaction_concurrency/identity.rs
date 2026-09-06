//! Typed identities and references of the transaction-concurrency
//! attachment (issue #24).
//!
//! Every wire string becomes one closed typed value before it can exist
//! in an attachment: typed qualified operation and effect references
//! reuse the accepted #14/#13 identities, set-like record identifiers use
//! the accepted #23 namespaced grammar, digests and versions reuse the
//! accepted #10 typed values. Anything path-shaped, URL-shaped,
//! credential-shaped, token-shaped, or runtime-shaped is refused as
//! private data before it can serialize.

use std::fmt;

use crate::effects::{EffectOrigin, FieldName, OperationId};
use crate::scenario::id::NamespacedId;
use crate::scenario::id::StepId;

/// Why one textual reference is not a valid attachment identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RefError {
    /// The text is empty, oversized, or violates the closed grammar.
    Shape,
    /// The effect-kind key is outside the closed #14 vocabulary.
    Kind,
    /// The kind/subject/field combination is illegal under #14 rules.
    Subject,
    /// The canonical occurrence or origin part is malformed.
    Occurrence,
}

/// A validated resource reference: one namespaced bounded selector for a
/// semantic resource (`planner.resource/user_task_planning`). Never SQL,
/// a raw path, or a target-framework lock name.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ResourceRef(NamespacedId);

impl ResourceRef {
    pub(crate) fn parse(text: &str) -> Result<Self, RefError> {
        NamespacedId::parse(text)
            .map(Self)
            .map_err(|_| RefError::Shape)
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for ResourceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0.as_str())
    }
}

/// A validated error-contract reference: an opaque typed pointer into the
/// owner-supplied #62 family (`errors.planner/conflict`). Never an error
/// message, code literal, or transport status.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ErrorRef(String);

impl ErrorRef {
    pub(crate) fn parse(text: &str) -> Result<Self, RefError> {
        let bytes = text.as_bytes();
        if text.is_empty() || bytes.len() > 96 {
            return Err(RefError::Shape);
        }
        let (namespace, name) = text.split_once('/').ok_or(RefError::Shape)?;
        let mut segments = namespace.split('.');
        let first_ok = matches!(segments.next(), Some("error" | "errors"));
        if !first_ok || !segments.all(lower_kebab) || !lower_kebab(name) {
            return Err(RefError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// One lowercase kebab segment.
fn lower_kebab(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() || bytes.len() > 63 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

/// One lowercase dotted chain of kebab segments.
fn lower_chain(chain: &str) -> bool {
    !chain.is_empty() && chain.split('.').all(lower_kebab)
}

/// A typed reference naming where an opaque ETag validator token is
/// carried (`header.if-match`, `metadata.etag`, `input.version_token`).
/// Never the literal token bytes.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EtagRef(String);

impl EtagRef {
    pub(crate) fn parse(text: &str) -> Result<Self, RefError> {
        let bytes = text.as_bytes();
        if text.is_empty() || bytes.len() > 96 {
            return Err(RefError::Shape);
        }
        let (family, rest) = text.split_once('.').ok_or(RefError::Shape)?;
        if !matches!(family, "header" | "metadata" | "input") || !lower_chain(rest) {
            return Err(RefError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The deterministic canonical acquisition key of one lock requirement:
/// bounded printable lowercase ASCII. Duplicate or inconsistent order
/// keys are semantic violations, never silent aliases.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OrderKey(String);

impl OrderKey {
    pub(crate) fn parse(text: &str) -> Result<Self, RefError> {
        let bytes = text.as_bytes();
        if text.is_empty() || bytes.len() > 128 {
            return Err(RefError::Shape);
        }
        let first_ok = bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit();
        let rest_ok = bytes[1..].iter().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b':' | b'#' | b'_' | b'-')
        });
        if !first_ok || !rest_ok {
            return Err(RefError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A typed distributed-protocol capability reference supplied by the
/// protocol owners (`distributed.two-phase-commit`). The v1 owner matrix
/// approves no concrete protocol, so no group may reference one yet; the
/// grammar exists for the reviewed successor that will.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DistributedProtocol(String);

impl DistributedProtocol {
    pub(crate) fn parse(text: &str) -> Result<Self, RefError> {
        let bytes = text.as_bytes();
        if text.len() < 12 || bytes.len() > 96 {
            return Err(RefError::Shape);
        }
        let rest = text.strip_prefix("distributed.").ok_or(RefError::Shape)?;
        let mut segments = rest.split('.');
        let first = segments.next().ok_or(RefError::Shape)?;
        if !lower_kebab(first) || !segments.all(lower_kebab) || !rest.contains('.') {
            return Err(RefError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// The parsed canonical spelling of one #14 EffectId:
/// `operation|kind|subject|origin|occurrence`. The subject is the
/// bounded resource identifier (with optional exact field); kind/subject
/// legality is only decidable against a real #14 graph, never from the
/// string alone.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EffectRef {
    operation: OperationId,
    kind_key: &'static str,
    subject: String,
    origin: EffectOrigin,
    occurrence: u32,
}

impl EffectRef {
    /// Parse and validate one canonical effect reference. The kind key
    /// must be in the closed #14 vocabulary, the subject must carry the
    /// legal resource kind and field shape for that kind, and the
    /// occurrence must be a bounded ordinal.
    pub(crate) fn parse(text: &str) -> Result<Self, RefError> {
        if text.len() > 512 {
            return Err(RefError::Shape);
        }
        let parts: Vec<&str> = text.split('|').collect();
        if parts.len() != 5 {
            return Err(RefError::Shape);
        }
        let operation = OperationId::from_qualified(parts[0]).ok_or(RefError::Shape)?;
        let kind_key = effect_kind_key(parts[1]).ok_or(RefError::Kind)?;
        let subject = parse_subject(parts[2]).ok_or(RefError::Subject)?;
        let origin = parse_origin(parts[3]).ok_or(RefError::Occurrence)?;
        if parts[4].is_empty() || parts[4].len() > 10 {
            return Err(RefError::Occurrence);
        }
        let occurrence: u32 = parts[4].parse().map_err(|_| RefError::Occurrence)?;
        Ok(Self {
            operation,
            kind_key,
            subject,
            origin,
            occurrence,
        })
    }

    /// The closed kind key.
    #[cfg(test)]
    pub(crate) const fn kind_key(&self) -> &'static str {
        self.kind_key
    }

    /// The acting operation.
    pub(crate) fn operation(&self) -> &OperationId {
        &self.operation
    }

    /// Whether this effect leaves any local transaction boundary
    /// unconditionally: external calls, jobs, published outputs, and
    /// every cache side effect. Event emission stays local (transactional
    /// outbox); audit entries stay local by definition.
    pub(crate) fn is_external(&self) -> bool {
        matches!(
            self.kind_key,
            "external-call"
                | "enqueue-job"
                | "publish-output"
                | "cache-read"
                | "cache-write"
                | "cache-invalidate"
        )
    }

    /// The canonical byte text.
    pub(crate) fn as_str(&self) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            self.operation.as_str(),
            self.kind_key,
            self.subject,
            self.origin.to_canonical_string(),
            self.occurrence
        )
    }
}

/// Resolve the closed kind key.
pub(crate) fn effect_kind_key(text: &str) -> Option<&'static str> {
    crate::effects::kind::EFFECT_KIND_KEYS
        .iter()
        .copied()
        .find(|candidate| *candidate == text)
}

/// Parse `resource` or `resource#field` into a typed #14 subject. The
/// canonical EffectId spelling carries no resource-kind prefix, so the
/// subject is validated as the bounded resource identifier grammar and
/// kind/subject legality is only checked against a real #14 graph.
fn parse_subject(text: &str) -> Option<String> {
    let (resource, field) = match text.split_once('#') {
        Some((resource, field)) => (resource, Some(field)),
        None => (text, None),
    };
    if resource.is_empty() || resource.len() > 192 {
        return None;
    }
    if !resource.bytes().all(|byte| {
        byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
    }) {
        return None;
    }
    if let Some(field) = field {
        FieldName::new(field)?;
    }
    Some(text.to_owned())
}

/// Parse `effect:<declared-symbol>` or `#<ordinal>`.
fn parse_origin(text: &str) -> Option<EffectOrigin> {
    if let Some(symbol) = text.strip_prefix("effect:") {
        if crate::effects::identity::is_semantic_id(symbol) {
            return Some(EffectOrigin::Declared(symbol.to_owned()));
        }
        return None;
    }
    let ordinal = text.strip_prefix('#')?;
    if ordinal.is_empty()
        || ordinal.len() > 10
        || !ordinal.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(EffectOrigin::Occurrence(ordinal.parse().ok()?))
}

/// A scenario-local participant, invocation, schedule-node, or barrier
/// identifier: one lowercase segment (the accepted #23 step-id grammar).
pub(crate) type LocalId = StepId;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_refs_parse_canonical_spellings() {
        let text =
            "operation:planner.command.focus_task|update|canonical:planner.user_task_planning#focused_at|effect:planner.effect.touch|0";
        let parsed = EffectRef::parse(text).expect("valid effect ref");
        assert_eq!(
            parsed.operation().as_str(),
            "operation:planner.command.focus_task"
        );
        assert_eq!(parsed.kind_key(), "update");
        assert!(!parsed.is_external());
        assert_eq!(parsed.as_str(), text);
    }

    #[test]
    fn external_kinds_classify_as_external() {
        for (kind, resource) in [
            ("external-call", "external-service:planner.svc/mail"),
            ("enqueue-job", "job:planner.job/mail-delivery"),
            ("publish-output", "output:planner.output/report"),
            ("cache-read", "cache:planner.cache/task-views"),
            ("cache-write", "cache:planner.cache/task-views"),
            ("cache-invalidate", "cache:planner.cache/task-views"),
        ] {
            let text = format!("operation:planner.command.focus_task|{kind}|{resource}|effect:planner.effect.send|0");
            let parsed = EffectRef::parse(&text)
                .unwrap_or_else(|error| panic!("valid external ref {kind}: {error:?}"));
            assert!(parsed.is_external(), "{kind} must classify external");
        }
        let text = "operation:planner.command.focus_task|emit-event|event:planner.event.focused|effect:planner.effect.emit|0";
        assert!(!EffectRef::parse(text).expect("event").is_external());
    }

    #[test]
    fn effect_refs_reject_illegal_subject_and_unknown_kinds() {
        // Kind/subject legality is only decidable against a real #14
        // graph; the string grammar still rejects malformed subjects,
        // unknown kinds, and broken occurrences.
        let text = "operation:planner.command.focus_task|read|Bad Path#x|effect:planner.effect.x|0";
        assert_eq!(EffectRef::parse(text), Err(RefError::Subject));
        let text = "operation:planner.command.focus_task|schema-migration|planner.user|effect:planner.effect.x|0";
        assert_eq!(EffectRef::parse(text), Err(RefError::Kind));
        let text =
            "operation:planner.command.focus_task|update|planner.user|effect:planner.effect.x|x0";
        assert_eq!(EffectRef::parse(text), Err(RefError::Occurrence));
        let text =
            "operation:planner.command.focus_task|read|planner.user#9bad|effect:planner.effect.x|0";
        assert_eq!(EffectRef::parse(text), Err(RefError::Subject));
    }

    #[test]
    fn refs_reject_paths_urls_and_tokens() {
        assert_eq!(ResourceRef::parse("/etc/passwd"), Err(RefError::Shape));
        assert_eq!(ResourceRef::parse("http://x/y"), Err(RefError::Shape));
        assert_eq!(ResourceRef::parse("C:/temp"), Err(RefError::Shape));
        assert_eq!(ErrorRef::parse("Bearer abc"), Err(RefError::Shape));
        assert_eq!(EtagRef::parse("W/\"abc123\""), Err(RefError::Shape));
        assert_eq!(OrderKey::parse("Drop table users"), Err(RefError::Shape));
    }

    #[test]
    fn etag_order_and_protocol_refs_parse() {
        assert!(EtagRef::parse("header.if-match").is_ok());
        assert!(EtagRef::parse("metadata.etag").is_ok());
        assert!(EtagRef::parse("sql.headers").is_err());
        assert!(OrderKey::parse("planner.user_task_planning#user_id").is_ok());
        assert!(DistributedProtocol::parse("distributed.two-phase.commit").is_ok());
        assert!(DistributedProtocol::parse("distributed.2pc").is_err());
        assert!(DistributedProtocol::parse("sql.two.phase").is_err());
    }
}
