//! Typed identifiers of the transport-http attachment (issue #70).
//!
//! Every wire string becomes one closed typed value before it can
//! exist in an attachment. Endpoint and scenario references reuse the
//! accepted #6 semantic grammar ([`SemanticId`]); wire names (param
//! names, scheme ids, field references) are aliases bound to closed
//! lower-snake shapes; header names follow the portable RFC 9110
//! token subset; operation ids are the deterministic camel-case
//! transform of the endpoint semantic id. Everything refuses
//! path-shaped, URL-shaped, credential-shaped, or runtime-shaped
//! text before it can serialize.

use std::fmt;

use crate::scenario::id::{IdError, SemanticId};

/// A validated wire parameter, scheme, or field name (`task_id`,
/// `user_bearer`). Never a path segment, URI, or free text.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct WireName(String);

impl WireName {
    /// Validate and keep the exact wire text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if lower_snake(text).is_some() {
            Ok(Self(text.to_owned()))
        } else {
            Err(IdError::Shape)
        }
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WireName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One non-empty lower-snake wire name (`a`, `task_id`): lowercase
/// ASCII letters, digits, and interior underscores with a leading
/// letter.
pub(crate) fn lower_snake(text: &str) -> Option<()> {
    if text.is_empty() || text.len() > 64 {
        return None;
    }
    let mut previous_was_underscore = false;
    for (index, ch) in text.char_indices() {
        match ch {
            'a'..='z' => previous_was_underscore = false,
            '0'..='9' => {
                if index == 0 {
                    return None;
                }
                previous_was_underscore = false;
            }
            '_' => {
                if index == 0 || previous_was_underscore {
                    return None;
                }
                previous_was_underscore = true;
            }
            _ => return None,
        }
    }
    if previous_was_underscore {
        return None;
    }
    Some(())
}

/// A validated HTTP header field name (`Idempotency-Key`,
/// `X-Request-Id`): the portable RFC 9110 token subset — an ASCII
/// letter followed by letters, digits, and interior dashes, never a
/// leading or trailing dash.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct HeaderName(String);

impl HeaderName {
    /// Validate and keep the exact header text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if !text.is_empty()
            && text.len() <= 64
            && text.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
            && !text.ends_with('-')
            && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !text.contains("--")
        {
            Ok(Self(text.to_owned()))
        } else {
            Err(IdError::Shape)
        }
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HeaderName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A validated bounded lowercase token (tags, flow ids, capability
/// tokens, api-version names): lowercase ASCII letters, digits, and
/// interior dots, dashes, or underscores.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SafeToken(String);

impl SafeToken {
    /// Validate and keep the exact token text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if text.is_empty() || text.len() > 64 {
            return Err(IdError::Shape);
        }
        let bytes = text.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return Err(IdError::Shape);
        }
        if !bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
        }) {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SafeToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One declared operation input field reference: either a command
/// input member (`input.task_id`) or a bare query-model parameter
/// name (`project_ref`). Transport-invented fields have no spelling.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FieldRef {
    /// Whether the reference is command-input prefixed.
    input_prefixed: bool,
    /// The bare member or parameter name.
    name: WireName,
}

impl FieldRef {
    /// Parse one field reference; `input.` prefix optional.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let (input_prefixed, name) = match text.strip_prefix("input.") {
            Some(name) => (true, name),
            None => (false, text),
        };
        Ok(Self {
            input_prefixed,
            name: WireName::parse(name)?,
        })
    }

    /// Whether the reference names a command input member.
    pub const fn is_input(&self) -> bool {
        self.input_prefixed
    }

    /// The bare member or parameter name.
    pub fn name(&self) -> &WireName {
        &self.name
    }

    /// The canonical wire spelling.
    pub fn as_str(&self) -> String {
        if self.input_prefixed {
            format!("input.{}", self.name.as_str())
        } else {
            self.name.as_str().to_owned()
        }
    }
}

impl fmt::Display for FieldRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// One validated endpoint operation identity: an explicit override
/// or the deterministic default derived from the endpoint semantic
/// id (segments joined camel-case: `planner.endpoint_focus_task` →
/// `plannerEndpointFocusTask`).
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct OperationId(String);

impl OperationId {
    /// Validate one explicit operation id.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if text.is_empty() || text.len() > 192 {
            return Err(IdError::Shape);
        }
        let bytes = text.as_bytes();
        if !bytes[0].is_ascii_lowercase() {
            return Err(IdError::Shape);
        }
        if !bytes.iter().all(|b| b.is_ascii_alphanumeric()) {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The deterministic default operation id of one endpoint
    /// semantic id: every dot segment after the first is camel-cased
    /// over its underscore-separated words
    /// (`planner.endpoint_focus_task` → `plannerEndpointFocusTask`).
    pub fn derive(endpoint: &SemanticId) -> Self {
        let mut out = String::new();
        for (index, segment) in endpoint.as_str().split('.').enumerate() {
            if index == 0 {
                out.push_str(segment);
                continue;
            }
            for word in segment.split('_') {
                let mut chars = word.chars();
                if let Some(first) = chars.next() {
                    out.extend(first.to_uppercase());
                    out.push_str(chars.as_str());
                }
            }
        }
        // The semantic-id grammar guarantees this is well-formed;
        // guard anyway so a hostile id can never serialize.
        Self::parse(&out).unwrap_or_else(|_| Self(endpoint.as_str().to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_closed_wire_names() {
        assert!(WireName::parse("task_id").is_ok());
        assert!(WireName::parse("a").is_ok());
        assert!(WireName::parse("project2").is_ok());
    }

    #[test]
    fn refuses_open_or_hostile_wire_names() {
        assert!(WireName::parse("").is_err());
        assert!(WireName::parse("_lead").is_err());
        assert!(WireName::parse("Trail_").is_err());
        assert!(WireName::parse("double__under").is_err());
        assert!(WireName::parse("9lead").is_err());
        assert!(WireName::parse("Pascal").is_err());
        assert!(WireName::parse("../escape").is_err());
    }

    #[test]
    fn accepts_closed_header_names() {
        assert!(HeaderName::parse("Idempotency-Key").is_ok());
        assert!(HeaderName::parse("X-Request-Id").is_ok());
        assert!(HeaderName::parse("Etag").is_ok());
    }

    #[test]
    fn refuses_open_or_hostile_header_names() {
        assert!(HeaderName::parse("-lead").is_err());
        assert!(HeaderName::parse("trail-").is_err());
        assert!(HeaderName::parse("double--dash").is_err());
        assert!(HeaderName::parse("lower").is_err());
        assert!(HeaderName::parse("With Space").is_err());
        assert!(HeaderName::parse(&"A".repeat(65)).is_err());
    }

    #[test]
    fn field_refs_split_the_input_prefix() {
        let field = FieldRef::parse("input.task_id").expect("input ref");
        assert!(field.is_input());
        assert_eq!(field.as_str(), "input.task_id");
        let param = FieldRef::parse("project_ref").expect("bare ref");
        assert!(!param.is_input());
        assert_eq!(param.as_str(), "project_ref");
        assert!(FieldRef::parse("input.").is_err());
        assert!(FieldRef::parse("input.Bad").is_err());
    }

    #[test]
    fn operation_ids_derive_deterministically() {
        let endpoint = SemanticId::parse("planner.endpoint_focus_task").unwrap();
        assert_eq!(
            OperationId::derive(&endpoint).as_str(),
            "plannerEndpointFocusTask"
        );
        assert!(OperationId::parse("plannerEndpointFocusTask").is_ok());
        assert!(OperationId::parse("NotCamel").is_err());
        assert!(OperationId::parse("with-dash").is_err());
        assert!(OperationId::parse("with_underscore").is_err());
    }
}
