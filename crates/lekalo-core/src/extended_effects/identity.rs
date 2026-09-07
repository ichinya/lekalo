//! Typed identities and references of the extended-effects attachment
//! (issue #26).
//!
//! Every wire string becomes one closed typed value before it can exist
//! in an attachment: typed effect references reuse the accepted #14
//! EffectId spellings with the per-family closed kind binding, provider
//! capabilities reuse the accepted #14 provider-contract grammar, error
//! references stay opaque typed pointers into the #62 family, and
//! record identifiers use the accepted #23 namespaced grammar. Anything
//! path-shaped, URL-shaped, credential-shaped, token-shaped, or
//! runtime-shaped is refused as private data before it can serialize.

use std::fmt;

use crate::effects::{EffectOrigin, FieldName, OperationId};

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

/// A validated provider capability contract in the accepted #14
/// provider grammar: namespaced id plus exact SemVer tail
/// (`vendor.mail/send@1.0.0`). Never a URL, hostname, credential, or
/// runtime endpoint.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProviderContract(String);

impl ProviderContract {
    pub(crate) fn parse(text: &str) -> Result<Self, RefError> {
        let (contract, version) = text.split_once('@').ok_or(RefError::Shape)?;
        let (namespace, name) = contract.split_once('/').ok_or(RefError::Shape)?;
        for segment in namespace.split('.') {
            if !lower_kebab(segment) {
                return Err(RefError::Shape);
            }
        }
        if !lower_kebab(name) {
            return Err(RefError::Shape);
        }
        // The exact canonical SemVer tail: no leading zeros, three
        // numeric parts, nothing else.
        let numeric = |part: &str| {
            if part.is_empty() || part.len() > 10 {
                return false;
            }
            if part == "0" {
                return true;
            }
            !part.starts_with('0') && part.bytes().all(|byte| byte.is_ascii_digit())
        };
        let mut parts = version.split('.');
        let valid = parts.next().map(numeric).unwrap_or(false)
            && parts.next().map(numeric).unwrap_or(false)
            && parts.next().map(numeric).unwrap_or(false)
            && parts.next().is_none();
        if !valid {
            return Err(RefError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderContract {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A validated error-contract reference: an opaque typed pointer into
/// the owner-supplied #62 family (`errors.vendor/timeout`). Never an
/// error message, code literal, transport status, or catch-all.
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
    /// legal resource identifier and field shape, and the occurrence
    /// must be a bounded ordinal.
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
    pub(crate) const fn kind_key(&self) -> &'static str {
        self.kind_key
    }

    /// The acting operation.
    pub(crate) fn operation(&self) -> &OperationId {
        &self.operation
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_refs_parse_canonical_spellings_and_classify_kinds() {
        let text =
            "operation:planner.focus_task|emit-event|planner.task_focused|effect:planner.create_task|0";
        let parsed = EffectRef::parse(text).expect("valid effect ref");
        assert_eq!(parsed.kind_key(), "emit-event");
        assert_eq!(parsed.as_str(), text);
        for kind in [
            "enqueue-job",
            "external-call",
            "cache-read",
            "cache-write",
            "cache-invalidate",
            "publish-output",
        ] {
            let text = format!(
                "operation:planner.notify_focus|{kind}|planner.subject|effect:planner.effect|0"
            );
            assert!(EffectRef::parse(&text).is_ok(), "{kind} must parse");
        }
    }

    #[test]
    fn effect_refs_reject_illegal_shapes() {
        let text =
            "operation:planner.focus_task|schema-migration|planner.user|effect:planner.effect.x|0";
        assert_eq!(EffectRef::parse(text), Err(RefError::Kind));
        let text = "operation:planner.focus_task|read|Bad Path#x|effect:planner.effect.x|0";
        assert_eq!(EffectRef::parse(text), Err(RefError::Subject));
        let text = "operation:planner.focus_task|read|planner.user|effect:planner.effect.x|x0";
        assert_eq!(EffectRef::parse(text), Err(RefError::Occurrence));
        let text = "operation:planner.focus_task|read|planner.user|effect:planner.effect.x";
        assert_eq!(EffectRef::parse(text), Err(RefError::Shape));
    }

    #[test]
    fn provider_and_error_refs_reject_paths_urls_and_tokens() {
        assert!(ProviderContract::parse("vendor.mail/send@1.0.0").is_ok());
        assert!(ProviderContract::parse("vendor.mail/send@01.0.0").is_err());
        assert!(ProviderContract::parse("vendor.mail/send@1.0").is_err());
        assert!(ProviderContract::parse("https://vendor/maill@1.0.0").is_err());
        assert!(ProviderContract::parse("vendor.mail/send@1.0.0-extra").is_err());
        assert_eq!(
            ErrorRef::parse("errors.vendor/timeout"),
            Ok(ErrorRef("errors.vendor/timeout".to_owned()))
        );
        assert_eq!(ErrorRef::parse("errors.vendor/*"), Err(RefError::Shape));
        assert_eq!(ErrorRef::parse("Bearer abc"), Err(RefError::Shape));
        assert_eq!(ErrorRef::parse("timeout"), Err(RefError::Shape));
    }
}
