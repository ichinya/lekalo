//! Stable identities for the Scenario IR (issue #23).
//!
//! Scenario identity is explicit and stable: the inherited #5 semantic ID
//! grammar (two or three dot-separated segments, lowercase) names the
//! project, the scenario, and every semantic reference; scenario-local
//! step IDs are single lowercase segments. Array position and text hashes
//! are never identity. Every parser here is total over its input and
//! returns a fixed rejection class instead of panicking.

use super::version;

/// Why one textual identity is not a valid Scenario IR identifier.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdError {
    /// The text is empty or longer than the inherited bound.
    Length,
    /// The text violates the closed segment grammar.
    Shape,
    /// The first segment reserves a Lekalo-internal namespace.
    Reserved,
}

/// A validated inherited #5 semantic identifier: `<module>.<name>` or
/// `<module>.<kind-namespace>.<name>` with every segment matching the
/// closed lowercase grammar. The exact byte text is preserved.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct SemanticId(String);

impl SemanticId {
    /// Validate and keep the exact text of a semantic identifier
    /// (two or three segments).
    pub fn parse(text: &str) -> Result<Self, IdError> {
        segmented(text, 2, 3, 191)
    }

    /// Validate and keep the exact text of a root identifier: one
    /// segment naming the project.
    pub fn parse_root(text: &str) -> Result<Self, IdError> {
        segmented(text, 1, 1, 191)
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated namespaced identifier of the accepted effect-contract
/// shape: `namespace[/namespace...]/name` in lowercase kebab segments.
/// Names non-semantic, adapter-owned things: fixtures, actors, runners,
/// capabilities, policies, contracts, tests.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct NamespacedId(String);

impl NamespacedId {
    /// Validate and keep the exact text of a namespaced identifier:
    /// dotted kebab namespace plus one slash and the kebab name
    /// (the inherited effect-contract shape).
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let bytes = text.as_bytes();
        if text.is_empty() || bytes.len() > 64 {
            return Err(IdError::Length);
        }
        let (namespace, name) = text.split_once('/').ok_or(IdError::Shape)?;
        for segment in namespace.split('.') {
            if !kebab_segment(segment) {
                return Err(IdError::Shape);
            }
        }
        if !kebab_segment(name) {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated scenario-local step identifier: one lowercase segment of
/// at most 64 bytes. Step identity is `scenario_id + step_id`; the
/// occurrence ordinal never participates.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct StepId(String);

impl StepId {
    /// Validate and keep the exact text of a step identifier.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if text.is_empty() || text.len() > 64 {
            return Err(IdError::Length);
        }
        if !lower_segment(text) {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated field name: one lowercase segment that never carries a
/// dot, so entity scope and field scope cannot be confused.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct FieldName(String);

impl FieldName {
    /// Validate and keep the exact text of a field name.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        if text.is_empty() || text.len() > 63 {
            return Err(IdError::Length);
        }
        if !lower_segment(text) {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A validated set-like tag.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Tag(String);

impl Tag {
    /// Validate and keep the exact text of a tag.
    pub fn parse(text: &str) -> Result<Self, IdError> {
        let bytes = text.as_bytes();
        if text.is_empty() || bytes.len() > version::MAX_TAG_BYTES {
            return Err(IdError::Length);
        }
        let first = bytes[0];
        let rest = &text[1..];
        let first_ok = first.is_ascii_lowercase() || first.is_ascii_digit();
        let rest_ok = rest.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b':' | b'-')
        });
        if !first_ok || !rest_ok {
            return Err(IdError::Shape);
        }
        Ok(Self(text.to_owned()))
    }

    /// The exact validated text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One dotted lowercase identifier with two or three segments and the
/// exact inherited bounds; the first segment may not reserve a Lekalo
/// namespace.
fn segmented(
    text: &str,
    min_segments: usize,
    max_segments: usize,
    max_bytes: usize,
) -> Result<SemanticId, IdError> {
    if text.is_empty() || text.len() > max_bytes {
        return Err(IdError::Length);
    }
    let segments: Vec<&str> = text.split('.').collect();
    if segments.len() < min_segments || segments.len() > max_segments {
        return Err(IdError::Shape);
    }
    for (index, segment) in segments.iter().enumerate() {
        if !lower_segment(segment) {
            return Err(IdError::Shape);
        }
        if index == 0 && matches!(*segment, "lekalo" | "dev") {
            return Err(IdError::Reserved);
        }
    }
    Ok(SemanticId(text.to_owned()))
}

/// One closed lowercase identifier segment: `[a-z][a-z0-9_]*` with at
/// most 63 bytes (the callers own their specific length bounds).
fn lower_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() || bytes.len() > 63 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
}

/// One closed lowercase kebab segment of a namespaced identifier.
fn kebab_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.is_empty() || bytes.len() > 63 {
        return false;
    }
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_ids_accept_module_and_kind_namespaced_shapes() {
        assert_eq!(
            SemanticId::parse_root("planner").expect("root id").as_str(),
            "planner"
        );
        for text in [
            "planner.user_task_planning",
            "planner.scenario.switch_focus",
            "planner.command.focus_task",
        ] {
            let id = SemanticId::parse(text).expect("valid semantic id");
            assert_eq!(id.as_str(), text);
        }
    }

    #[test]
    fn semantic_ids_reject_reserved_invalid_and_oversized_shapes() {
        assert_eq!(SemanticId::parse(""), Err(IdError::Length));
        assert_eq!(SemanticId::parse("lekalo.planner"), Err(IdError::Reserved));
        assert_eq!(SemanticId::parse("dev.planner"), Err(IdError::Reserved));
        assert_eq!(SemanticId::parse("Planner.task"), Err(IdError::Shape));
        assert_eq!(SemanticId::parse("planner."), Err(IdError::Shape));
        assert_eq!(SemanticId::parse(".planner"), Err(IdError::Shape));
        assert_eq!(
            SemanticId::parse("planner.task.extra fourth"),
            Err(IdError::Shape)
        );
        assert_eq!(SemanticId::parse("planner.9task"), Err(IdError::Shape));
        let long = format!("planner.{}", "a".repeat(200));
        assert_eq!(SemanticId::parse(&long), Err(IdError::Length));
    }

    #[test]
    fn namespaced_ids_accept_nested_namespaces_and_reject_shapes() {
        assert!(NamespacedId::parse("core/switch-focus").is_ok());
        assert!(NamespacedId::parse("adapters.node/vitest-suite").is_ok());
        assert_eq!(
            NamespacedId::parse("core/actors/admin"),
            Err(IdError::Shape)
        );
        assert_eq!(NamespacedId::parse("switch-focus"), Err(IdError::Shape));
        assert_eq!(NamespacedId::parse("core/"), Err(IdError::Shape));
        assert_eq!(NamespacedId::parse("core/Switch"), Err(IdError::Shape));
        assert_eq!(NamespacedId::parse(""), Err(IdError::Length));
    }

    #[test]
    fn step_ids_and_field_names_are_single_lowercase_segments() {
        assert!(StepId::parse("focus_switched").is_ok());
        assert!(StepId::parse("a").is_ok());
        assert_eq!(StepId::parse("a.b"), Err(IdError::Shape));
        assert_eq!(StepId::parse(""), Err(IdError::Length));
        assert!(FieldName::parse("focused_at").is_ok());
        assert_eq!(FieldName::parse("other.entity.field"), Err(IdError::Shape));
        assert_eq!(FieldName::parse(""), Err(IdError::Length));
    }

    #[test]
    fn tags_accept_closed_charset_and_reject_the_rest() {
        assert!(Tag::parse("smoke").is_ok());
        assert!(Tag::parse("p0.auth:denied-2").is_ok());
        assert_eq!(Tag::parse("-leading"), Err(IdError::Shape));
        assert_eq!(Tag::parse("spa ce"), Err(IdError::Shape));
        assert_eq!(Tag::parse(""), Err(IdError::Length));
        let long = "a".repeat(65);
        assert_eq!(Tag::parse(&long), Err(IdError::Length));
    }
}
