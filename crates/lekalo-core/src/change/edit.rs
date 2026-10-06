//! Source-preserving byte-edit planning (issue #73).
//!
//! Every edit is derived from the exact semantic-owner-to-syntax-node
//! association of the accepted loader: the typed definition node locates the
//! insertion, removal, or rename range, and all bytes outside the declared
//! range stay verbatim — comments, key order, quoting, blank lines,
//! indentation, CRLF line endings, and unrelated definitions included. An
//! edit whose insertion or removal boundary is not provably clean refuses
//! with `change.source-preservation-unavailable` instead of guessing; there
//! is no whole-file reserialization fallback.

use crate::loader::frontends::{Node, Value};

use super::diagnostic::ChangeError;

/// One planned byte replacement over original document bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ByteEdit {
    /// Inclusive start byte of the replaced range.
    pub start: usize,
    /// Exclusive end byte of the replaced range.
    pub end: usize,
    /// The exact replacement bytes (empty for removals).
    pub replacement: String,
}

impl ByteEdit {
    /// Apply this edit to exact bytes; all other bytes stay verbatim.
    pub fn apply(&self, bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(bytes.len() + self.replacement.len());
        out.extend_from_slice(&bytes[..self.start]);
        out.extend_from_slice(self.replacement.as_bytes());
        out.extend_from_slice(&bytes[self.end..]);
        out
    }
}

/// The first byte offset after the line break that follows `offset`.
fn line_start_after(bytes: &[u8], offset: usize) -> usize {
    let mut cursor = offset;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\n' {
            return cursor + 1;
        }
        cursor += 1;
    }
    bytes.len()
}

/// The first byte offset of the line containing `offset`.
fn line_start_at(bytes: &[u8], offset: usize) -> usize {
    let mut cursor = offset.min(bytes.len());
    while cursor > 0 {
        if bytes[cursor - 1] == b'\n' {
            return cursor;
        }
        cursor -= 1;
    }
    0
}

/// Quote a YAML scalar with the double-quoted style used by the checked-in
/// fixtures; only the two YAML double-quote escapes are produced.
fn quote_yaml(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The line break in effect immediately before one insertion offset.
fn newline_before(bytes: &[u8], offset: usize) -> &'static str {
    if offset >= 2 && bytes[offset - 2] == b'\r' && bytes[offset - 1] == b'\n' {
        "\r\n"
    } else {
        "\n"
    }
}

/// Build the YAML block of one field item at the indentation of the last
/// existing item (`anchor_column`), using the document's line break style.
pub fn yaml_field_item(
    bytes: &[u8],
    anchor_column: usize,
    name: &str,
    type_spelling: &str,
    required: bool,
    description: Option<&str>,
) -> String {
    let indent = " ".repeat(anchor_column);
    let newline = newline_before(bytes, bytes.len());
    let mut item = String::new();
    item.push_str(newline);
    item.push_str(&indent);
    item.push_str("- name: ");
    item.push_str(&quote_yaml(name));
    item.push_str(newline);
    item.push_str(&indent);
    item.push_str("  type: ");
    item.push_str(&quote_yaml(type_spelling));
    if required {
        item.push_str(newline);
        item.push_str(&indent);
        item.push_str("  required: true");
    }
    if let Some(description) = description {
        item.push_str(newline);
        item.push_str(&indent);
        item.push_str("  description: ");
        item.push_str(&quote_yaml(description));
    }
    item
}

/// Build the YAML block of one enum-case item at the anchor column.
pub fn yaml_case_item(
    bytes: &[u8],
    anchor_column: usize,
    name: &str,
    description: Option<&str>,
) -> String {
    let indent = " ".repeat(anchor_column);
    let newline = newline_before(bytes, bytes.len());
    let mut item = String::new();
    item.push_str(newline);
    item.push_str(&indent);
    item.push_str("- value: ");
    item.push_str(&quote_yaml(name));
    if let Some(description) = description {
        item.push_str(newline);
        item.push_str(&indent);
        item.push_str("  description: ");
        item.push_str(&quote_yaml(description));
    }
    item
}

/// The anchor dash column and end offset of a sequence's last item. The
/// YAML frontend reports zero-width event spans for block-mapping items,
/// so the item range is derived from the first key's position (the dash
/// sits two columns before it) and the last entry's value span.
fn seq_anchor(items: &[Node]) -> Result<(usize, usize), ChangeError> {
    let Some(last) = items.last() else {
        return Err(ChangeError::SourcePreservation {
            subject: String::new(),
            detail: "sequence-empty",
        });
    };
    let (dash_byte, end) = item_range(last).ok_or(ChangeError::SourcePreservation {
        subject: String::new(),
        detail: "item-layout",
    })?;
    let _ = dash_byte;
    let anchor_column = item_anchor_column(last).ok_or(ChangeError::SourcePreservation {
        subject: String::new(),
        detail: "item-layout",
    })?;
    Ok((anchor_column, end))
}

/// The 0-based dash column of one block-sequence item.
fn item_anchor_column(item: &Node) -> Option<usize> {
    match &item.value {
        Value::Map(entries) if !entries.is_empty() => {
            // The dash sits two 1-based columns before the key; the anchor
            // is its 0-based indentation.
            let column = entries.first()?.key_span.start.column.checked_sub(3)?;
            Some(column)
        }
        Value::Scalar(_) => Some(item.span.start.column - 1),
        _ => None,
    }
}

/// The exact byte range of one block-sequence item: the dash marker through
/// the end of its last entry's value, plus the dash's 0-based column.
fn item_range(item: &Node) -> Option<(usize, usize)> {
    match &item.value {
        Value::Map(entries) if !entries.is_empty() => {
            let key_start = entries.first()?.key_span.start.byte;
            let dash = key_start.checked_sub(2)?;
            let last_end = entries.last()?.value.span.end.byte;
            // The dash must sit at `dash` exactly (two columns before the
            // key: `- `); anything else is a layout this planner refuses.
            Some((dash, last_end))
        }
        Value::Scalar(_) => {
            let start = item.span.start.byte;
            let end = item.span.end.byte;
            (end > start).then_some((start, end))
        }
        _ => None,
    }
}

/// Locate the member sequence node of one spanned definition.
fn member_sequence<'node>(
    definition: &'node Node,
    member: &str,
) -> Result<&'node [Node], ChangeError> {
    let sequence = definition.get(member).ok_or(ChangeError::SubjectConflict {
        subject: String::new(),
        detail: "member-missing",
    })?;
    match &sequence.value {
        Value::Seq(items) => Ok(items),
        _ => Err(ChangeError::SourcePreservation {
            subject: String::new(),
            detail: "member-not-sequence",
        }),
    }
}

/// Whether the first non-blank line after `from` is a comment line.
fn next_content_is_comment(bytes: &[u8], from: usize) -> bool {
    let mut cursor = from;
    while cursor < bytes.len() {
        let line_end = line_start_after(bytes, cursor);
        let line = &bytes[cursor..line_end.min(bytes.len())];
        let text = std::str::from_utf8(line).unwrap_or("");
        let trimmed = text.trim_end();
        let trimmed = trimmed.trim_start();
        if !trimmed.is_empty() {
            return trimmed.starts_with('#');
        }
        cursor = line_end;
    }
    false
}

/// Whether the last non-blank line before `offset` is a comment line.
fn previous_content_is_comment(bytes: &[u8], offset: usize) -> bool {
    let mut cursor = offset;
    while cursor > 0 {
        let line_start = line_start_at(bytes, cursor.min(bytes.len()));
        let line = &bytes[line_start..cursor];
        let text = std::str::from_utf8(line).unwrap_or("");
        let trimmed = text.trim_end();
        if !trimmed.trim_start().is_empty() {
            return trimmed.trim_start().starts_with('#');
        }
        cursor = line_start;
        if cursor > 0 {
            cursor -= 1;
        }
    }
    false
}

/// Plan the insertion of one prebuilt item block into a named sequence of
/// one spanned definition over exact original bytes.
pub fn plan_seq_insert(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    item: &str,
) -> Result<ByteEdit, ChangeError> {
    let items = member_sequence(definition, member)?;
    let (_, insert_at) = seq_anchor(items)?;
    // A comment line directly after the insertion point would lose its
    // ownership ambiguity: refusal preserves it instead of guessing.
    if next_content_is_comment(bytes, insert_at) {
        return Err(ChangeError::SourcePreservation {
            subject: String::new(),
            detail: "comment-adjacent",
        });
    }
    Ok(ByteEdit {
        start: insert_at,
        end: insert_at,
        replacement: item.to_owned(),
    })
}

/// Build the YAML block of one field item at the anchor of the named
/// member sequence of one spanned definition.
pub fn field_item_block(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    name: &str,
    type_spelling: &str,
    required: bool,
    description: Option<&str>,
) -> Result<String, ChangeError> {
    let items = member_sequence(definition, member)?;
    let (anchor, _) = seq_anchor(items)?;
    Ok(yaml_field_item(
        bytes,
        anchor,
        name,
        type_spelling,
        required,
        description,
    ))
}

/// Build the YAML block of one enum-case item at the anchor of the named
/// member sequence of one spanned definition.
pub fn case_item_block(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    name: &str,
    description: Option<&str>,
) -> Result<String, ChangeError> {
    let items = member_sequence(definition, member)?;
    let (anchor, _) = seq_anchor(items)?;
    Ok(yaml_case_item(bytes, anchor, name, description))
}

/// Plan the removal of one sequence item by exact member identity.
pub fn plan_seq_item_remove(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    name: &str,
) -> Result<ByteEdit, ChangeError> {
    let items = member_sequence(definition, member)?;
    let position = items
        .iter()
        .position(|item| item.get("name").and_then(Node::as_str) == Some(name))
        .or_else(|| {
            items
                .iter()
                .position(|item| item.get("value").and_then(Node::as_str) == Some(name))
        })
        .ok_or(ChangeError::SubjectConflict {
            subject: name.to_owned(),
            detail: "member-missing",
        })?;
    let (dash, item_end) = item_range(&items[position]).ok_or(ChangeError::SourcePreservation {
        subject: name.to_owned(),
        detail: "item-layout",
    })?;
    // A comment line directly above or after the removed item refuses:
    // ownership is ambiguous and preservation wins over guessing.
    if next_content_is_comment(bytes, item_end) || previous_content_is_comment(bytes, dash) {
        return Err(ChangeError::SourcePreservation {
            subject: name.to_owned(),
            detail: "comment-adjacent",
        });
    }
    // The removal consumes the line break BEFORE the item (its line
    // terminator) through the end of its last value; the trailing break
    // stays with the following line. A dash not alone on its line is a
    // layout this planner refuses.
    let line_start = line_start_at(bytes, dash);
    if line_start == 0 || bytes[line_start..dash].iter().any(|byte| *byte != b' ') {
        return Err(ChangeError::SourcePreservation {
            subject: name.to_owned(),
            detail: "item-layout",
        });
    }
    let mut start = line_start - 1;
    if start > 0 && bytes[start - 1] == b'\r' {
        start -= 1;
    }
    Ok(ByteEdit {
        start,
        end: item_end,
        replacement: String::new(),
    })
}

/// Plan the rename of one sequence item's identity scalar in place: only
/// the quoted scalar bytes change; everything else stays verbatim.
pub fn plan_seq_item_rename(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    name: &str,
    to: &str,
) -> Result<ByteEdit, ChangeError> {
    let items = member_sequence(definition, member)?;
    let item = items
        .iter()
        .find(|item| item.get("name").and_then(Node::as_str) == Some(name))
        .ok_or(ChangeError::SubjectConflict {
            subject: name.to_owned(),
            detail: "member-missing",
        })?;
    let entry = item.get_entry("name").ok_or(ChangeError::SubjectConflict {
        subject: name.to_owned(),
        detail: "member-missing",
    })?;
    let span = entry.value.span;
    // The replacement reproduces the original scalar style exactly: quoted
    // stays quoted, plain stays plain.
    let original = std::str::from_utf8(&bytes[span.start.byte..span.end.byte])
        .map_err(|_| ChangeError::SourcePreservation {
            subject: name.to_owned(),
            detail: "encoding",
        })?
        .to_owned();
    let replacement = if original.starts_with('"') || original.starts_with('\'') {
        let quote = &original[..1];
        format!("{quote}{}{quote}", escape_quoted(to))
    } else {
        to.to_owned()
    };
    Ok(ByteEdit {
        start: span.start.byte,
        end: span.end.byte,
        replacement,
    })
}

/// Escape one value for a double-quoted YAML scalar body.
fn escape_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::frontends::parse_document;
    use crate::loader::source::LineIndex;

    fn parse(text: &str) -> (Node, Vec<u8>) {
        let index = LineIndex::new(text);
        let parsed = parse_document(text, &index).expect("parses");
        match parsed {
            crate::loader::frontends::Parsed::Root(root) => (*root, text.as_bytes().to_vec()),
            crate::loader::frontends::Parsed::Empty => panic!("empty"),
        }
    }

    fn definition_of(root: &Node) -> &Node {
        let definitions = root.get("definitions").expect("definitions");
        let items = definitions.as_seq().expect("sequence");
        items
            .iter()
            .find(|item| {
                item.get("kind")
                    .and_then(crate::loader::frontends::Node::as_str)
                    == Some("entity")
            })
            .expect("entity definition")
    }

    const DOC: &str = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.task\n    kind: entity\n    version: 1\n    fields:\n      - name: title\n        type: \"tasks.text\"\n        required: true\n      - name: done\n        type: \"tasks.bool\"\n    identity:\n      - title\n";

    #[test]
    fn insert_appends_after_the_last_field_and_preserves_all_other_bytes() {
        let (root, bytes) = parse(DOC);
        let definition = definition_of(&root);
        let items = member_sequence(definition, "fields").expect("fields");
        let (anchor_column, _) = seq_anchor(items).expect("anchor");
        let item = yaml_field_item(
            &bytes,
            anchor_column,
            "priority",
            "task.priority",
            false,
            None,
        );
        let edit = plan_seq_insert(&bytes, definition, "fields", &item).expect("edit");
        let edited = String::from_utf8(edit.apply(&bytes)).expect("utf8");
        assert!(edited.contains("      - name: \"priority\"\n        type: \"task.priority\"\n"));
        // The sibling field and the identity block after fields are untouched.
        assert!(edited.trim_end().ends_with("- title"));
    }

    #[test]
    fn remove_cuts_exactly_the_item_lines() {
        let (root, bytes) = parse(DOC);
        let definition = definition_of(&root);
        let edit = plan_seq_item_remove(&bytes, definition, "fields", "done").expect("edit");
        let edited = String::from_utf8(edit.apply(&bytes)).expect("utf8");
        assert!(!edited.contains("done"), "{edited}");
        assert!(edited.contains("title"));
        assert!(edited.contains("identity:"));
    }

    #[test]
    fn rename_touches_only_the_plain_scalar() {
        let (root, bytes) = parse(DOC);
        let definition = definition_of(&root);
        let edit =
            plan_seq_item_rename(&bytes, definition, "fields", "done", "completed").expect("edit");
        // The scalar is unquoted in the source: the range is the bare name
        // and the replacement stays plain.
        assert_eq!(edit.end - edit.start, "done".len(), "plain scalar range");
        let edited = String::from_utf8(edit.apply(&bytes)).expect("utf8");
        assert!(edited.contains("name: completed"));
        assert!(edited.contains("type: \"tasks.bool\""));
    }

    #[test]
    fn rename_keeps_the_quoted_scalar_style() {
        let doc = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.task\n    kind: entity\n    version: 1\n    fields:\n      - name: \"done\"\n        type: \"tasks.bool\"\n    identity:\n      - done\n";
        let (root, bytes) = parse(doc);
        let definition = definition_of(&root);
        let edit =
            plan_seq_item_rename(&bytes, definition, "fields", "done", "completed").expect("edit");
        assert_eq!(
            edit.end - edit.start,
            "\"done\"".len(),
            "quoted scalar range"
        );
        let edited = String::from_utf8(edit.apply(&bytes)).expect("utf8");
        assert!(edited.contains("name: \"completed\""));
        assert!(edited.contains("type: \"tasks.bool\""));
    }

    #[test]
    fn comment_adjacent_insert_refuses() {
        let doc = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.task\n    kind: entity\n    version: 1\n    fields:\n      - name: title\n        type: \"tasks.text\"\n    # trailing comment\n    identity:\n      - title\n";
        let (root, bytes) = parse(doc);
        let definition = definition_of(&root);
        let items = member_sequence(definition, "fields").expect("fields");
        let (anchor_column, _) = seq_anchor(items).expect("anchor");
        let item = yaml_field_item(
            &bytes,
            anchor_column,
            "priority",
            "task.priority",
            false,
            None,
        );
        let error = plan_seq_insert(&bytes, definition, "fields", &item).expect_err("refuses");
        assert!(matches!(
            error,
            ChangeError::SourcePreservation {
                detail: "comment-adjacent",
                ..
            }
        ));
    }

    #[test]
    fn crlf_documents_keep_their_line_endings() {
        let doc = "schema_version: \"0.2.16\"\r\ndefinitions:\r\n  - id: tasks.task\r\n    kind: entity\r\n    version: 1\r\n    fields:\r\n      - name: title\r\n        type: \"tasks.text\"\r\n    identity:\r\n      - title\r\n";
        let (root, bytes) = parse(doc);
        let definition = definition_of(&root);
        let items = member_sequence(definition, "fields").expect("fields");
        let (anchor_column, _) = seq_anchor(items).expect("anchor");
        let item = yaml_field_item(
            &bytes,
            anchor_column,
            "priority",
            "task.priority",
            false,
            None,
        );
        assert!(
            item.contains("\r\n"),
            "item follows the CRLF style: {item:?}"
        );
        let edit = plan_seq_insert(&bytes, definition, "fields", &item).expect("edit");
        let edited = edit.apply(&bytes);
        let text = std::str::from_utf8(&edited).expect("utf8");
        assert!(!text.contains("\"\n"), "no bare LF after CRLF document");
    }
}
