//! Typed identifiers of the invariant-transition attachment (issue
//! #63).
//!
//! Every wire string becomes one closed typed value before it can
//! exist in an attachment. Record identifiers reuse the accepted #6
//! semantic grammar ([`SemanticId`]); requirement, policy, profile,
//! and test references reuse the accepted namespaced grammar
//! ([`NamespacedId`]); only the state identifier and the #66
//! expression reference are new here, and both are closed, bounded,
//! and refuse anything path-shaped, URL-shaped, credential-shaped, or
//! runtime-shaped before it can serialize.

use std::fmt;

use crate::scenario::id::IdError;

/// A validated state identifier inside one state space (`todo`,
/// `doing`, `done`). Never a path segment, URI, or free text.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StateRef(String);

impl StateRef {
    /// Validate and keep the exact state text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let bytes = text.as_bytes();
        if text.is_empty() || bytes.len() > 64 {
            return Err(IdError::Length);
        }
        let first = bytes[0];
        let rest_ok = bytes[1..]
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'));
        if first.is_ascii_lowercase() && rest_ok {
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

impl fmt::Display for StateRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A validated reference into the future typed-expression family #66
/// (`expr.planner/overdue-check`). Pure declaration data: resolvability
/// stays with the expression owner, and no execution claim is made.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ExpressionRef(String);

impl ExpressionRef {
    /// Validate and keep the exact expression reference text.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let (namespace, name) = text.split_once('/').ok_or(IdError::Shape)?;
        let mut segments = namespace.split('.');
        let first_ok = matches!(segments.next(), Some("expr"));
        // At least one owned sub-segment after the `expr` root: a bare
        // `expr/name` reference is refused.
        let second_ok = match segments.next() {
            Some(segment) => lower_kebab(segment),
            None => false,
        };
        if !first_ok || !second_ok || !segments.all(lower_kebab) || !lower_kebab(name) {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ExpressionRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One lower-kebab segment.
pub(crate) fn lower_kebab(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_refs_accept_closed_grammar() {
        assert!(StateRef::parse("todo").is_ok());
        assert!(StateRef::parse("in_review-2").is_ok());
        assert!(StateRef::parse("").is_err());
        assert!(StateRef::parse("Todo").is_err());
        assert!(StateRef::parse("1todo").is_err());
        assert!(StateRef::parse("../escape").is_err());
        assert!(StateRef::parse(&"a".repeat(65)).is_err());
    }

    #[test]
    fn expression_refs_accept_only_the_expr_namespace() {
        assert!(ExpressionRef::parse("expr.planner/overdue").is_ok());
        assert!(ExpressionRef::parse("errors.planner/overdue").is_err());
        assert!(ExpressionRef::parse("expr/overdue").is_err());
        assert!(ExpressionRef::parse("https://host/expr").is_err());
    }
}
