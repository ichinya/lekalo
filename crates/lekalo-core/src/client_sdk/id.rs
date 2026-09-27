//! Typed identifiers of the client-SDK projection (issue #72).
//!
//! Every generated surface becomes one closed typed value before it can
//! exist in a projection. Target languages are a closed vocabulary;
//! generated identifiers carry their language so the same semantic id
//! renders deterministically per backend; consumer ids are validated
//! logical tokens so an unregistered consumer can never silently enter
//! the index. Everything refuses path-shaped, URL-shaped, or
//! credential-shaped text before it can serialize.

use std::fmt;

use crate::diagnostics::DiagnosticSet;

use super::diagnostic;

/// The closed target-language vocabulary of the first SDK generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Language {
    /// TypeScript (Vue/web consumers).
    Typescript,
    /// Go (service-to-service consumers).
    Go,
    /// PHP.
    Php,
}

impl Language {
    /// The exact wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Typescript => "typescript",
            Self::Go => "go",
            Self::Php => "php",
        }
    }

    /// The language for one wire token, or nothing. Rust is
    /// deliberately absent: its client is a later generation.
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "typescript" => Self::Typescript,
            "go" => Self::Go,
            "php" => Self::Php,
            _ => return None,
        })
    }
}

/// One validated generated identifier: `snake_case` or `camelCase`
/// over ASCII letters, digits, and interior underscores, with a
/// leading lowercase letter and a bounded length. The projection
/// never invents a name beyond this grammar; a semantic id that
/// cannot render is a refusal, never a mangled name.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TargetIdent(String);

impl TargetIdent {
    /// Validate and keep the exact identifier text.
    pub fn parse(text: &str) -> Result<Self, DiagnosticSet> {
        if !valid_ident(text) {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONTRACT_INVALID,
                "identifier-shape",
                Some(text),
            ));
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TargetIdent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Whether `text` satisfies the closed identifier grammar: ASCII
/// letters, digits, interior underscores, leading lowercase letter,
/// no trailing underscore, no doubles, bounded length.
pub(crate) fn valid_ident(text: &str) -> bool {
    if text.is_empty() || text.len() > super::version::MAX_IDENTIFIER {
        return false;
    }
    let bytes = text.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut previous_was_underscore = false;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' => previous_was_underscore = false,
            b'0'..=b'9' => {
                if index == 0 {
                    return false;
                }
                previous_was_underscore = false;
            }
            b'_' => {
                if index == 0 || previous_was_underscore || index + 1 == bytes.len() {
                    return false;
                }
                previous_was_underscore = true;
            }
            _ => return false,
        }
    }
    true
}

/// The deterministic `snake_case` spelling of one semantic id's local
/// tail (`planner.endpoint_focus_task` -> `endpoint_focus_task`).
/// The tail keeps the semantic grammar's own snake segments, so the
/// mapping is total over the accepted grammar and order-stable.
pub fn snake_of_semantic(symbol: &str) -> String {
    match symbol.split_once('.') {
        Some((_, tail)) => tail.to_owned(),
        None => symbol.to_owned(),
    }
}

/// The deterministic `camelCase` spelling of one semantic id
/// (`planner.endpoint_focus_task` -> `endpointFocusTask`).
/// The deterministic `camelCase` spelling of one semantic id. Every
/// dot segment after the module joins camel-case over its
/// underscore-separated words, so multi-segment ids stay total:
/// `planner.focus_task.input` -> `focusTaskInput`.
pub fn camel_of_semantic(symbol: &str) -> String {
    let mut words: Vec<&str> = Vec::new();
    for segment in symbol.split('.').skip(1) {
        words.extend(segment.split('_'));
    }
    camel_of_words(&words)
}

/// The deterministic `camelCase` spelling of already-split lowercase
/// words.
pub(crate) fn camel_of_words(words: &[&str]) -> String {
    let mut out = String::new();
    for (index, word) in words.iter().enumerate() {
        if word.is_empty() {
            continue;
        }
        if index == 0 {
            out.push_str(word);
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
        }
    }
    out
}

/// One validated consumer id: the closed lower-snake token grammar
/// (`web_console`, `billing`). Consumers are explicit maintained
/// declarations, never inferred from traffic.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ConsumerId(String);

impl ConsumerId {
    /// Validate and keep the exact consumer id.
    pub fn parse(text: &str) -> Result<Self, DiagnosticSet> {
        if !super::project::is_lower_snake(text) {
            return Err(diagnostic::rule_invalid(
                diagnostic::CONSUMER_INVALID,
                "consumer-id-shape",
                Some(text),
            ));
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConsumerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_are_closed() {
        assert_eq!(Language::parse("typescript"), Some(Language::Typescript));
        assert_eq!(Language::parse("go"), Some(Language::Go));
        assert_eq!(Language::parse("php"), Some(Language::Php));
        assert_eq!(Language::parse("rust"), None, "Rust is a later generation");
        assert_eq!(Language::parse("Ruby"), None);
    }

    #[test]
    fn identifiers_accept_closed_shapes() {
        assert!(TargetIdent::parse("endpoint_focus_task").is_ok());
        assert!(TargetIdent::parse("endpointFocusTask").is_ok());
        assert!(TargetIdent::parse("a").is_ok());
        assert!(TargetIdent::parse("v2plan").is_ok());
    }

    #[test]
    fn identifiers_refuse_open_or_hostile_shapes() {
        assert!(TargetIdent::parse("").is_err());
        assert!(TargetIdent::parse("_lead").is_err());
        assert!(TargetIdent::parse("Trail_").is_err());
        assert!(TargetIdent::parse("double__under").is_err());
        assert!(TargetIdent::parse("9lead").is_err());
        assert!(TargetIdent::parse("../escape").is_err());
        assert!(TargetIdent::parse("has space").is_err());
        assert!(TargetIdent::parse(&"a".repeat(200)).is_err());
    }

    #[test]
    fn semantic_spellings_are_deterministic() {
        assert_eq!(snake_of_semantic("planner.endpoint_focus_task"), "endpoint_focus_task");
        assert_eq!(camel_of_semantic("planner.endpoint_focus_task"), "endpointFocusTask");
        assert_eq!(camel_of_semantic("planner.list_tasks"), "listTasks");
        assert_eq!(camel_of_semantic("planner.count_focused"), "countFocused");
    }

    #[test]
    fn consumer_ids_are_closed_tokens() {
        assert!(ConsumerId::parse("web_console").is_ok());
        assert!(ConsumerId::parse("billing").is_ok());
        assert!(ConsumerId::parse("Web Console").is_err());
        assert!(ConsumerId::parse("").is_err());
        assert!(ConsumerId::parse("../escape").is_err());
    }
}
