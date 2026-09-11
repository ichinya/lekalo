//! Typed identifiers of the query-model attachment (issue #64).
//!
//! Every wire string becomes one closed typed value before it can
//! exist in an attachment. Query, source, policy, and scenario
//! references reuse the accepted #6 semantic grammar
//! ([`SemanticId`]); field references reuse the accepted model field
//! grammar ([`FieldName`]); parameter names are aliases bound to the
//! closed lower-snake parameter shape, and both refuse anything
//! path-shaped, URL-shaped, credential-shaped, or runtime-shaped
//! before it can serialize.

use std::fmt;

use crate::scenario::id::IdError;

/// A validated query parameter name (`due_before`, `state_set`).
/// Never a path segment, URI, or free text.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ParameterName(String);

impl ParameterName {
    /// Validate and keep the exact parameter text.
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

impl fmt::Display for ParameterName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One non-empty lower-snake segment (`a`, `due_before`): lowercase
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_closed_parameter_names() {
        assert!(ParameterName::parse("due_before").is_ok());
        assert!(ParameterName::parse("a").is_ok());
        assert!(ParameterName::parse("state2_set").is_ok());
    }

    #[test]
    fn refuses_open_or_hostile_parameter_names() {
        assert!(ParameterName::parse("").is_err());
        assert!(ParameterName::parse("_lead").is_err());
        assert!(ParameterName::parse("trail_").is_err());
        assert!(ParameterName::parse("double__under").is_err());
        assert!(ParameterName::parse("9lead").is_err());
        assert!(ParameterName::parse("Pascal").is_err());
        assert!(ParameterName::parse("has-dash").is_err());
        assert!(ParameterName::parse("../escape").is_err());
        assert!(ParameterName::parse("constructor").is_ok());
        assert!(ParameterName::parse(&"a".repeat(65)).is_err());
    }
}
