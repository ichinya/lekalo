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

use crate::loader::frontends::{Node, Scalar, Value};

use super::diagnostic::ChangeError;
use super::request::{FieldDecl, Literal};

pub type FieldSpec<'a> = (&'a str, &'a str, bool, Option<&'a str>, Option<&'a Literal>);
type OwnedFieldSpec = (
    String,
    String,
    bool,
    Option<String>,
    Option<serde_json::Value>,
);

/// Render one admitted JSON literal using the existing block-only YAML
/// grammar. Empty collections have no spelling in that grammar and refuse
/// before any source write; JSON source decoding still represents them.
fn yaml_literal_suffix(
    value: &serde_json::Value,
    indent: usize,
    newline: &str,
) -> Result<String, ChangeError> {
    use serde_json::Value as Json;
    let prefix = format!("{newline}{}", " ".repeat(indent + 2));
    match value {
        Json::Array(items) if !items.is_empty() => {
            let mut out = String::new();
            for item in items {
                out.push_str(&prefix);
                out.push('-');
                out.push_str(&yaml_literal_suffix(item, indent + 2, newline)?);
            }
            Ok(out)
        }
        Json::Object(entries) if !entries.is_empty() => {
            let mut out = String::new();
            let mut entries: Vec<_> = entries.iter().collect();
            entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            for (key, value) in entries {
                out.push_str(&prefix);
                out.push_str(&serde_json::to_string(key).expect("key"));
                out.push(':');
                out.push_str(&yaml_literal_suffix(value, indent + 2, newline)?);
            }
            Ok(out)
        }
        Json::Array(_) | Json::Object(_) => Err(ChangeError::SourcePreservation {
            subject: "default".to_owned(),
            detail: "literal-empty-collection",
        }),
        _ => Ok(format!(
            " {}",
            serde_json::to_string(value).expect("literal")
        )),
    }
}

fn yaml_default_field_item(
    bytes: &[u8],
    anchor: usize,
    field: FieldSpec<'_>,
) -> Result<String, ChangeError> {
    let (name, typ, required, description, default) = field;
    let mut block = yaml_field_item(bytes, anchor, name, typ, required, description);
    if let Some(default) = default {
        block.push_str(newline_before(bytes, bytes.len()));
        block.push_str(&" ".repeat(anchor + 2));
        block.push_str("default:");
        block.push_str(&yaml_literal_suffix(
            &default.to_json_value(),
            anchor + 2,
            newline_before(bytes, bytes.len()),
        )?);
    }
    Ok(block)
}

/// A YAML event span can include trailing trivia. Prove the scalar token's
/// lexical end before replacing it, so an inline comment retains its owner.
fn literal_scalar_end(bytes: &[u8], node: &Node, member: &str) -> Result<usize, ChangeError> {
    let start = node.span.start.byte;
    let end = node.span.end.byte;
    let original = bytes
        .get(start..end)
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    if let Some(quote @ (b'\'' | b'"')) = original.first().copied() {
        let mut cursor = 1;
        while cursor < original.len() {
            if quote == b'"' && original[cursor] == b'\\' {
                cursor += 2;
            } else if original[cursor] == quote {
                if quote == b'\'' && original.get(cursor + 1) == Some(&quote) {
                    cursor += 2;
                } else {
                    return Ok(start + cursor + 1);
                }
            } else {
                cursor += 1;
            }
        }
    } else if !matches!(original.first(), Some(b'|' | b'>')) {
        let mut cursor = 0;
        while cursor < original.len() && !matches!(original[cursor], b'\r' | b'\n') {
            if original[cursor] == b'#'
                && (cursor == 0 || matches!(original[cursor - 1], b' ' | b'\t'))
            {
                break;
            }
            cursor += 1;
        }
        while cursor > 0 && matches!(original[cursor - 1], b' ' | b'\t') {
            cursor -= 1;
        }
        if cursor > 0 {
            return Ok(start + cursor);
        }
    }
    Err(ChangeError::SourcePreservation {
        subject: member.to_owned(),
        detail: "scalar-layout",
    })
}

fn replace_literal_member(
    bytes: &[u8],
    item: &Node,
    member: &str,
    suffix: &str,
) -> Result<ByteEdit, ChangeError> {
    let entry = item
        .get_entry(member)
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    if !matches!(entry.value.value, Value::Scalar(_)) {
        return plan_raw_member_replace(bytes, item, member, suffix);
    }
    let span = entry.value.span;
    let token_end = literal_scalar_end(bytes, &entry.value, member)?;
    let line_end = content_end(
        bytes,
        line_start_at(bytes, token_end),
        line_start_after(bytes, token_end),
    );
    if suffix.starts_with(['\n', '\r']) && bytes[token_end..line_end].contains(&b'#') {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "comment-adjacent",
        });
    }
    Ok(ByteEdit {
        start: span.start.byte,
        end: token_end,
        replacement: suffix.strip_prefix(' ').unwrap_or(suffix).to_owned(),
    })
}

fn remove_default(bytes: &[u8], item: &Node) -> Result<ByteEdit, ChangeError> {
    let entry = item
        .get_entry("default")
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    if !matches!(entry.value.value, Value::Scalar(_)) {
        let mut removal = plan_raw_member_replace(bytes, item, "default", "")?;
        removal.replacement.clear();
        return Ok(removal);
    }
    let start = line_start_at(bytes, entry.key_span.start.byte);
    let token_end = literal_scalar_end(bytes, &entry.value, "default")?;
    let end = content_end(
        bytes,
        line_start_at(bytes, token_end),
        line_start_after(bytes, token_end),
    );
    if bytes[start..entry.key_span.start.byte]
        .iter()
        .any(|b| *b != b' ')
        || bytes[token_end..end].contains(&b'#')
        || previous_content_is_comment(bytes, start)
    {
        return Err(ChangeError::SourcePreservation {
            subject: "default".to_owned(),
            detail: "comment-adjacent",
        });
    }
    Ok(ByteEdit {
        start,
        end,
        replacement: String::new(),
    })
}

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

    /// Apply a set of disjoint original-relative edits in
    /// descending-offset order so every range is interpreted against the
    /// original bytes (the multi-operation batch convention, review
    /// finding 7). Single-edit sets are the degenerate case.
    pub fn apply_all(bytes: &[u8], edits: &[ByteEdit]) -> Vec<u8> {
        let mut sorted: Vec<&ByteEdit> = edits.iter().collect();
        sorted.sort_by(|left, right| left.start.cmp(&right.start).then(left.end.cmp(&right.end)));
        let mut out = bytes.to_vec();
        for edit in sorted.iter().rev() {
            out = edit.apply(&out);
        }
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
    } else if offset >= 1 && bytes[offset - 1] == b'\n' {
        "\n"
    } else {
        newline_near(bytes, offset)
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
/// so the item range is derived from the first key's position and the
/// last entry's value span; the dash column is derived from the exact
/// bytes (the single dash-anchor rule every list edit shares, round 5,
/// finding 1).
fn seq_anchor(bytes: &[u8], items: &[Node]) -> Result<(usize, usize), ChangeError> {
    let Some(last) = items.last() else {
        return Err(ChangeError::SourcePreservation {
            subject: String::new(),
            detail: "sequence-empty",
        });
    };
    let end = item_end(last).ok_or(ChangeError::SourcePreservation {
        subject: String::new(),
        detail: "item-layout",
    })?;
    let anchor_column = item_anchor_column(bytes, last).ok_or(ChangeError::SourcePreservation {
        subject: String::new(),
        detail: "item-layout",
    })?;
    Ok((anchor_column, end))
}

/// The byte offset of the `-` marker that introduces one sequence item
/// on its line, derived from the bytes: walk back from the item's first
/// token (its first key for mapping items, the scalar itself for scalar
/// items) over the separating spaces to the dash, then require only
/// spaces before the dash. Both item shapes share this ONE rule, so an
/// ordinary `- item` line, an indented one, and a two-space-after-dash
/// layout all derive the same anchor (round 5, finding 1: the previous
/// per-shape column arithmetic anchored scalar items two columns too
/// deep and corrupted every ordinary indented list edit).
fn dash_byte(bytes: &[u8], item: &Node) -> Option<usize> {
    let token = match &item.value {
        Value::Map(entries) if !entries.is_empty() => entries.first()?.key_span.start.byte,
        Value::Scalar(_) => item.span.start.byte,
        _ => return None,
    };
    let mut cursor = token.min(bytes.len());
    while cursor > 0 && bytes.get(cursor - 1) == Some(&b' ') {
        cursor -= 1;
    }
    if cursor == 0 || bytes.get(cursor - 1) != Some(&b'-') {
        return None;
    }
    let dash = cursor - 1;
    let line_start = line_start_at(bytes, dash);
    if bytes[line_start..dash].iter().any(|byte| *byte != b' ') {
        return None;
    }
    Some(dash)
}

/// The 0-based dash column of one block-sequence item (its indentation).
pub(crate) fn item_anchor_column(bytes: &[u8], item: &Node) -> Option<usize> {
    let dash = dash_byte(bytes, item)?;
    Some(dash - line_start_at(bytes, dash))
}

/// The exclusive end byte of one block-sequence item: the end of its
/// last entry's value for mapping items, the scalar's own end otherwise.
fn item_end(item: &Node) -> Option<usize> {
    match &item.value {
        Value::Map(entries) if !entries.is_empty() => item_end(&entries.last()?.value),
        Value::Seq(items) if !items.is_empty() => item_end(items.last()?),
        Value::Scalar(_) => Some(item.span.end.byte),
        _ => None,
    }
}

/// The exact byte range of one block-sequence item: the dash marker
/// through the end of its last value (the removal convention).
fn item_range(bytes: &[u8], item: &Node) -> Option<(usize, usize)> {
    let dash = dash_byte(bytes, item)?;
    let end = item_end(item)?;
    (end > dash).then_some((dash, end))
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
        cursor = cursor.saturating_sub(1);
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
    let (_, insert_at) = seq_anchor(bytes, items)?;
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
    field: FieldSpec<'_>,
) -> Result<String, ChangeError> {
    let items = member_sequence(definition, member)?;
    let (anchor, _) = seq_anchor(bytes, items)?;
    yaml_default_field_item(bytes, anchor, field)
}

/// Build the YAML block of one enum-case item at the anchor of the named
/// member sequence of one spanned definition.
pub fn case_item_block(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    name: &str,
    description: Option<&str>,
    wire_value: Option<&Literal>,
) -> Result<String, ChangeError> {
    let items = member_sequence(definition, member)?;
    let (anchor, _) = seq_anchor(bytes, items)?;
    let mut block = yaml_case_item(bytes, anchor, name, description);
    if let Some(value) = wire_value {
        let newline = newline_before(bytes, bytes.len());
        block.push_str(&format!("{newline}{}wire_value:", " ".repeat(anchor + 2)));
        block.push_str(&yaml_literal_suffix(
            &value.to_json_value(),
            anchor + 2,
            newline,
        )?);
    }
    Ok(block)
}

/// Build the YAML block of one effect-reference item (a scalar string
/// list entry) at the anchor of the named member sequence.
pub fn effect_item_block(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    effect_id: &str,
) -> Result<String, ChangeError> {
    let items = member_sequence(definition, member)?;
    let (anchor, _) = seq_anchor(bytes, items)?;
    let indent = " ".repeat(anchor);
    let newline = newline_before(bytes, bytes.len());
    Ok(format!("{newline}{indent}- {}", quote_yaml(effect_id)))
}

/// Plan the insertion of one prebuilt item block after the last scalar
/// item of a named scalar sequence (the effect reference lists).
pub fn plan_seq_scalar_insert(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    value: &str,
    item: &str,
) -> Result<ByteEdit, ChangeError> {
    let items = member_sequence(definition, member)?;
    if items
        .iter()
        .any(|existing| existing.as_str() == Some(value))
    {
        return Err(ChangeError::SubjectConflict {
            subject: value.to_owned(),
            detail: "member-duplicate",
        });
    }
    let (_, insert_at) = seq_anchor(bytes, items)?;
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

/// Plan the removal of one scalar item by its exact value.
pub fn plan_seq_scalar_remove(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    value: &str,
) -> Result<ByteEdit, ChangeError> {
    let items = member_sequence(definition, member)?;
    let position = items
        .iter()
        .position(|item| item.as_str() == Some(value))
        .ok_or(ChangeError::SubjectConflict {
            subject: value.to_owned(),
            detail: "member-missing",
        })?;
    let (dash, item_end) =
        item_range(bytes, &items[position]).ok_or(ChangeError::SourcePreservation {
            subject: value.to_owned(),
            detail: "item-layout",
        })?;
    if next_content_is_comment(bytes, item_end) || previous_content_is_comment(bytes, dash) {
        return Err(ChangeError::SourcePreservation {
            subject: value.to_owned(),
            detail: "comment-adjacent",
        });
    }
    let line_start = line_start_at(bytes, dash);
    if line_start == 0 || bytes[line_start..dash].iter().any(|byte| *byte != b' ') {
        return Err(ChangeError::SourcePreservation {
            subject: value.to_owned(),
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

/// The 0-based anchor column and end offset of the document-level
/// definitions sequence (the last definition's mapping range).
pub fn definitions_anchor(
    bytes: &[u8],
    definitions: &[Node],
) -> Result<(usize, usize), ChangeError> {
    seq_anchor(bytes, definitions)
}

/// Whether the line containing `offset` is a comment line (its first
/// non-blank character is `#`).
fn line_is_comment(bytes: &[u8], offset: usize) -> bool {
    let anchor = offset.min(bytes.len().saturating_sub(1));
    let line_start = line_start_at(bytes, anchor);
    let line_end = line_start_after(bytes, line_start).min(bytes.len());
    let text = std::str::from_utf8(&bytes[line_start..line_end]).unwrap_or("");
    text.trim().starts_with('#')
}

/// Plan the append of one prebuilt definition block after the last
/// definition of a document.
pub fn plan_document_definition_append(
    bytes: &[u8],
    definitions: &[&Node],
    item: &str,
) -> Result<ByteEdit, ChangeError> {
    if definitions.is_empty() {
        return Err(ChangeError::SourcePreservation {
            subject: String::new(),
            detail: "sequence-empty",
        });
    }
    // The insert position is the end of the last content line of the
    // document, textually: the definitions sequence is the last member of
    // a kind-home document, and container spans are unreliable.
    let mut end = 0usize;
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        let line_end = line_start_after(bytes, cursor);
        let trimmed = content_end(bytes, cursor, line_end);
        if trimmed > cursor {
            end = trimmed;
        }
        cursor = line_end;
    }
    if end == 0 {
        return Err(ChangeError::SourcePreservation {
            subject: String::new(),
            detail: "sequence-empty",
        });
    }
    // Round 5, finding 3: a file whose last content line is a comment
    // (a file-trailing note) would silently re-home the comment onto the
    // appended definition — the insert point sits after it, so the note
    // stops closing the file and starts annotating the previous
    // definition. The bidirectional comment rule refuses instead.
    if line_is_comment(bytes, end.saturating_sub(1)) {
        return Err(ChangeError::SourcePreservation {
            subject: String::new(),
            detail: "comment-adjacent",
        });
    }
    if next_content_is_comment(bytes, end) {
        return Err(ChangeError::SourcePreservation {
            subject: String::new(),
            detail: "comment-adjacent",
        });
    }
    Ok(ByteEdit {
        start: end,
        end,
        replacement: item.to_owned(),
    })
}

/// The exact byte range of one named member block of one definition,
/// derived textually from the member key line: container value spans are
/// unreliable in the closed YAML frontend, so the block extends over the
/// lines indented deeper than the member key until the next member.
/// Returns (key line start, end of the last content line, key indent).
fn member_block_range(
    bytes: &[u8],
    definition: &Node,
    member: &str,
) -> Result<(usize, usize, usize), ChangeError> {
    let entry = definition
        .get_entry(member)
        .ok_or(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-absent",
        })?;
    let key_line_start = line_start_at(bytes, entry.key_span.start.byte);
    let key_indent = entry.key_span.start.column - 1;
    let mut cursor = line_start_after(bytes, key_line_start);
    let mut end = key_line_start;
    let mut content = 0usize;
    while cursor < bytes.len() {
        let line_end = line_start_after(bytes, cursor);
        let trimmed_end = content_end(bytes, cursor, line_end);
        let line = &bytes[cursor..trimmed_end];
        let offset = line.iter().position(|byte| *byte != b' ');
        match offset {
            // A blank line: skip it without extending the block.
            None => {
                cursor = line_end;
            }
            Some(offset) if offset > key_indent => {
                // Round 5, finding 2: a comment line inside the member
                // block has ambiguous ownership — replacing the block
                // would delete it, and keeping it would re-home it. The
                // refusal matches the removal paths (LEK-CHANGE-007).
                if text_is_comment(&bytes[cursor..trimmed_end]) {
                    return Err(ChangeError::SourcePreservation {
                        subject: member.to_owned(),
                        detail: "comment-adjacent",
                    });
                }
                end = trimmed_end;
                content += 1;
                cursor = line_end;
            }
            Some(_) => break,
        }
    }
    if content == 0 {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-empty",
        });
    }
    Ok((key_line_start, end, key_indent))
}

/// The end offset of the line's content (before its line break and
/// trailing spaces).
fn content_end(bytes: &[u8], line_start: usize, line_end: usize) -> usize {
    let mut end = line_end;
    while end > line_start {
        let byte = bytes[end - 1];
        if byte == b'\n' || byte == b'\r' || byte == b' ' {
            end -= 1;
        } else {
            break;
        }
    }
    end
}

/// Plan the bounded byte edits that replace one named member sequence's
/// field list (round 3, finding 14): an unchanged signature plans NO byte
/// edit at all — comments, quoting, blank lines, and CRLF endings stay
/// verbatim; a changed signature performs proven per-item edits only
/// (bounded scalar replacements inside an item, whole-line removals,
/// member-line appends, whole-item insertions/removals that already
/// refuse comment-adjacent layouts). Reorders and items carrying members
/// outside the closed field grammar refuse explicitly instead of
/// reformatting the block.
pub fn plan_member_sequence_replace(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    fields: &[FieldSpec<'_>],
) -> Result<Vec<ByteEdit>, ChangeError> {
    if fields.is_empty() {
        if definition.get(member).is_none() {
            return Ok(Vec::new());
        }
        let mut removal = plan_raw_member_replace(bytes, definition, member, "")?;
        removal.replacement.clear();
        return Ok(vec![removal]);
    }
    if definition.get(member).is_none() {
        let indent = definition
            .get_entry("id")
            .ok_or(ChangeError::WriteBoundaryDenied)?
            .key_span
            .start
            .column
            - 1;
        let mut suffix = String::new();
        for field in fields {
            suffix.push_str(&yaml_default_field_item(bytes, indent + 2, *field)?);
        }
        return Ok(vec![plan_raw_member_append(
            bytes, definition, member, &suffix,
        )?]);
    }
    let items = member_sequence(definition, member)?;
    // Decode the exact current member facts, including successor defaults;
    // anything else refuses instead of being silently dropped.
    let mut existing: Vec<OwnedFieldSpec> = Vec::with_capacity(items.len());
    for item in items {
        let Value::Map(entries) = &item.value else {
            return Err(ChangeError::SourcePreservation {
                subject: member.to_owned(),
                detail: "member-not-sequence",
            });
        };
        let mut name = None;
        let mut field_type = None;
        let mut required = false;
        let mut description = None;
        let mut default = None;
        let mut extra = 0usize;
        for entry in entries {
            match entry.key.as_str() {
                "name" => name = entry.value.as_str().map(str::to_owned),
                "type" => field_type = entry.value.as_str().map(str::to_owned),
                "required" => {
                    required = matches!(entry.value.value, Value::Scalar(Scalar::Bool(true)))
                }
                "description" => description = entry.value.as_str().map(str::to_owned),
                "default" => {
                    let value = crate::ir::DefaultValue::from_node(&entry.value).ok_or(
                        ChangeError::SourcePreservation {
                            subject: member.to_owned(),
                            detail: "field-default-literal",
                        },
                    )?;
                    default = Some(value.as_json().clone());
                }
                _ => extra += 1,
            }
        }
        let (Some(name), Some(field_type)) = (name, field_type) else {
            return Err(ChangeError::SourcePreservation {
                subject: member.to_owned(),
                detail: "member-layout",
            });
        };
        if extra > 0 {
            return Err(ChangeError::SourcePreservation {
                subject: name,
                detail: "member-layout",
            });
        }
        existing.push((name, field_type, required, description, default));
    }
    let requested: Vec<OwnedFieldSpec> = fields
        .iter()
        .map(|(name, field_type, required, description, default)| {
            (
                (*name).to_owned(),
                (*field_type).to_owned(),
                *required,
                description.map(str::to_owned),
                default.map(Literal::to_json_value),
            )
        })
        .collect();
    // The unchanged signature is a byte-identical no-op: no comment is
    // transferred, no quoting is normalized, no blank line appears.
    if existing == requested {
        return Ok(Vec::new());
    }
    // A reorder of the common members is not a proven bounded edit:
    // refuse instead of reformatting the whole block.
    let existing_names: Vec<&str> = existing.iter().map(|(name, ..)| name.as_str()).collect();
    let requested_common: Vec<&str> = requested
        .iter()
        .map(|(name, ..)| name.as_str())
        .filter(|name| existing_names.contains(name))
        .collect();
    let existing_common: Vec<&str> = existing_names
        .iter()
        .copied()
        .filter(|name| requested.iter().any(|(other, ..)| other == name))
        .collect();
    if requested_common != existing_common {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-order",
        });
    }
    let mut edits: Vec<ByteEdit> = Vec::new();
    // Removals of disappearing members (whole-item line ranges; the
    // comment-adjacent refusal applies per item).
    for (name, ..) in &existing {
        if !requested.iter().any(|(other, ..)| other == name) {
            edits.push(plan_seq_item_remove(bytes, definition, member, name)?);
        }
    }
    // In-place updates of kept members: bounded edits inside the item's
    // own lines only. Appended member lines are collected first so a
    // same-offset tie with the trailing new-item insertion (both insert
    // at the last kept item's end) resolves with the appended member
    // landing inside its item and the new item after it.
    let mut appended = Vec::new();
    for (index, (name, field_type, required, description, default)) in existing.iter().enumerate() {
        let Some((_, requested_type, requested_required, requested_description, requested_default)) =
            requested.iter().find(|(other, ..)| other == name)
        else {
            continue;
        };
        let item = &items[index];
        if default != requested_default {
            let indent = item
                .get_entry("name")
                .ok_or(ChangeError::WriteBoundaryDenied)?
                .key_span
                .start
                .column
                - 1;
            let suffix = requested_default
                .as_ref()
                .map(|v| yaml_literal_suffix(v, indent, newline_before(bytes, bytes.len())))
                .transpose()?;
            match (default, suffix) {
                (Some(_), Some(suffix)) => {
                    edits.push(replace_literal_member(bytes, item, "default", &suffix)?)
                }
                (Some(_), None) => {
                    edits.push(remove_default(bytes, item)?);
                }
                (None, Some(suffix)) => {
                    appended.push(plan_raw_member_append(bytes, item, "default", &suffix)?)
                }
                (None, None) => {}
            }
        }
        if field_type != requested_type {
            edits.push(replace_item_scalar(bytes, item, "type", requested_type)?);
        }
        if required != requested_required {
            if *requested_required {
                appended.push(append_item_member(bytes, item, "required", "true")?);
            } else {
                edits.push(remove_item_member(bytes, item, "required")?);
            }
        }
        if description != requested_description {
            match (description, requested_description) {
                (Some(_), Some(new)) => {
                    edits.push(replace_item_scalar(bytes, item, "description", new)?);
                }
                (Some(_), None) => {
                    edits.push(remove_item_member(bytes, item, "description")?);
                }
                (None, Some(new)) => {
                    appended.push(append_item_member(
                        bytes,
                        item,
                        "description",
                        &format!("\"{}\"", escape_quoted(new)),
                    )?);
                }
                (None, None) => {}
            }
        }
    }
    edits.extend(appended);
    // Insertions of new members: ONE combined block at the sequence's
    // end anchor (requested order preserved; comment-adjacent refusal
    // applies). An empty member (no items yet) cannot reach here — the
    // sequence exists because the member decodes.
    let new_fields: Vec<&OwnedFieldSpec> = requested
        .iter()
        .filter(|(name, ..)| !existing.iter().any(|(other, ..)| other == name))
        .collect();
    if !new_fields.is_empty() {
        let (anchor, _) = seq_anchor(bytes, items)?;
        let mut block = String::new();
        for (name, field_type, required, description, default) in new_fields {
            block.push_str(&yaml_field_item(
                bytes,
                anchor,
                name,
                field_type,
                *required,
                description.as_deref(),
            ));
            if let Some(default) = default {
                block.push_str(newline_before(bytes, bytes.len()));
                block.push_str(&" ".repeat(anchor + 2));
                block.push_str("default:");
                block.push_str(&yaml_literal_suffix(
                    default,
                    anchor + 2,
                    newline_before(bytes, bytes.len()),
                )?);
            }
        }
        edits.push(plan_seq_insert(bytes, definition, member, &block)?);
    }
    Ok(edits)
}

/// Replace one scalar member's value inside one sequence item in place,
/// reproducing the original scalar style exactly (quoted stays quoted,
/// plain stays plain).
fn replace_item_scalar(
    bytes: &[u8],
    item: &Node,
    member: &str,
    new_text: &str,
) -> Result<ByteEdit, ChangeError> {
    let entry = item
        .get_entry(member)
        .ok_or(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-absent",
        })?;
    let span = entry.value.span;
    let token_end = literal_scalar_end(bytes, &entry.value, member)?;
    let original = std::str::from_utf8(&bytes[span.start.byte..token_end])
        .map_err(|_| ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "encoding",
        })?
        .to_owned();
    let replacement = if original.starts_with('\'') {
        format!("'{}'", new_text.replace('\'', "''"))
    } else if original.starts_with('"') {
        format!("\"{}\"", escape_quoted(new_text))
    } else {
        new_text.to_owned()
    };
    Ok(ByteEdit {
        start: span.start.byte,
        end: token_end,
        replacement,
    })
}

/// Remove one whole member line (`key: value`) from inside one sequence
/// item: the line break before the key line through the value's end.
fn remove_item_member(bytes: &[u8], item: &Node, member: &str) -> Result<ByteEdit, ChangeError> {
    let entry = item
        .get_entry(member)
        .ok_or(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-absent",
        })?;
    let line_start = line_start_at(bytes, entry.key_span.start.byte);
    if line_start == 0
        || bytes[line_start..entry.key_span.start.byte]
            .iter()
            .any(|b| *b != b' ')
    {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-layout",
        });
    }
    let mut start = line_start - 1;
    if start > 0 && bytes[start - 1] == b'\r' {
        start -= 1;
    }
    Ok(ByteEdit {
        start,
        end: entry.value.span.end.byte,
        replacement: String::new(),
    })
}

/// Append one member line (`key: value`) inside one sequence item, at the
/// end of the item's last entry, at the item's own member indentation.
fn append_item_member(
    bytes: &[u8],
    item: &Node,
    member: &str,
    value: &str,
) -> Result<ByteEdit, ChangeError> {
    let item_end = item_end(item).ok_or(ChangeError::SourcePreservation {
        subject: member.to_owned(),
        detail: "member-layout",
    })?;
    let Value::Map(entries) = &item.value else {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-layout",
        });
    };
    let key_column = entries
        .first()
        .map(|entry| entry.key_span.start.column)
        .unwrap_or(1);
    let Some(indent) = key_column.checked_sub(1) else {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-layout",
        });
    };
    let newline = if bytes.get(item_end) == Some(&b'\r') {
        "\r\n"
    } else {
        "\n"
    };
    Ok(ByteEdit {
        start: item_end,
        end: item_end,
        replacement: format!("{newline}{}{member}: {value}", " ".repeat(indent)),
    })
}

/// Plan the replacement of the whole `returns` member block with the new
/// type mapping.
pub fn plan_returns_replace(
    bytes: &[u8],
    definition: &Node,
    type_spelling: &str,
) -> Result<ByteEdit, ChangeError> {
    if definition.get("returns").is_none() {
        let indent = definition
            .get_entry("id")
            .and_then(|entry| entry.key_span.start.column.checked_sub(1))
            .ok_or(ChangeError::WriteBoundaryDenied)?;
        let suffix = type_mapping_lines(
            type_spelling,
            indent + 2,
            newline_before(bytes, bytes.len()),
        );
        return plan_raw_member_append(bytes, definition, "returns", &suffix);
    }
    let (start, end, key_indent) = member_block_range(bytes, definition, "returns")?;
    let block = yaml_returns_block(bytes, key_indent - 2, type_spelling);
    // The replaced range starts at the key line; the block's leading
    // newline is not part of it.
    Ok(ByteEdit {
        start,
        end,
        replacement: block
            .strip_prefix(newline_before(bytes, bytes.len()))
            .ok_or(ChangeError::SourcePreservation {
                subject: "returns".to_owned(),
                detail: "member-layout",
            })?
            .to_owned(),
    })
}

/// Build the YAML block of one whole definition item at the given anchor
/// column (the document's definitions dash column).
pub fn yaml_definition_item(
    bytes: &[u8],
    anchor_column: usize,
    id: &str,
    kind: &str,
    description: Option<&str>,
    members: &str,
) -> String {
    let indent = " ".repeat(anchor_column);
    let inner = " ".repeat(anchor_column + 2);
    let newline = newline_before(bytes, bytes.len());
    let mut item = String::new();
    item.push_str(newline);
    item.push_str(&indent);
    item.push_str("- id: ");
    item.push_str(&quote_yaml(id));
    item.push_str(newline);
    item.push_str(&inner);
    item.push_str("kind: ");
    item.push_str(&quote_yaml(kind));
    item.push_str(newline);
    item.push_str(&inner);
    item.push_str("version: 1");
    if let Some(description) = description {
        item.push_str(newline);
        item.push_str(&inner);
        item.push_str("description: ");
        item.push_str(&quote_yaml(description));
    }
    item.push_str(members);
    item
}

/// The members block (already newline+indent prefixed) of one field list
/// (`input`/`payload`) at the definition's inner indentation.
pub fn yaml_field_list(
    bytes: &[u8],
    anchor_column: usize,
    member: &str,
    fields: &[FieldDecl],
) -> Result<String, ChangeError> {
    let mut out = format!(
        "{}{}{member}:",
        newline_before(bytes, bytes.len()),
        " ".repeat(anchor_column + 2)
    );
    for field in fields {
        out.push_str(&yaml_default_field_item(
            bytes,
            anchor_column + 4,
            (
                &field.name,
                &field.r#type.to_spelling(),
                field.required,
                field.description.as_deref(),
                field.default.as_ref(),
            ),
        )?);
    }
    Ok(out)
}

/// The members block of one id-reference list (`effects`, `reads`,
/// `appliesTo`, `covers`) at the definition's inner indentation.
pub fn yaml_id_list(bytes: &[u8], anchor_column: usize, member: &str, ids: &[&str]) -> String {
    let inner = " ".repeat(anchor_column + 2);
    let item_indent = " ".repeat(anchor_column + 4);
    let newline = newline_before(bytes, bytes.len());
    let mut out = String::new();
    out.push_str(newline);
    out.push_str(&inner);
    out.push_str(member);
    out.push(':');
    for id in ids {
        out.push_str(newline);
        out.push_str(&item_indent);
        out.push_str("- ");
        out.push_str(&quote_yaml(id));
    }
    out
}

/// The members block of the query `returns` mapping at the inner
/// indentation.
pub fn yaml_returns_block(bytes: &[u8], anchor_column: usize, type_spelling: &str) -> String {
    let inner = " ".repeat(anchor_column + 2);
    let newline = newline_before(bytes, bytes.len());
    let mut out = String::new();
    out.push_str(newline);
    out.push_str(&inner);
    out.push_str("returns:");
    out.push_str(&type_mapping_lines(
        type_spelling,
        anchor_column + 4,
        newline,
    ));
    out
}

/// The closed mapping spelling of one type reference: `ref: "x"`, nested
/// `list:` / `optional:` mappings at increasing indent.
fn type_mapping_lines(spelling: &str, indent: usize, newline: &str) -> String {
    let pad = " ".repeat(indent);
    if let Some(inner) = spelling.strip_suffix('?') {
        return format!(
            "{newline}{pad}optional:{}",
            type_mapping_lines(inner, indent + 2, newline)
        );
    }
    if let Some(inner) = spelling
        .strip_prefix("list<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        return format!(
            "{newline}{pad}list:{}",
            type_mapping_lines(inner, indent + 2, newline)
        );
    }
    format!("{newline}{pad}ref: {}", quote_yaml(spelling))
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
    let (dash, item_end) =
        item_range(bytes, &items[position]).ok_or(ChangeError::SourcePreservation {
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
        .find(|item| {
            item.get("name").and_then(Node::as_str) == Some(name)
                || item.get("value").and_then(Node::as_str) == Some(name)
        })
        .ok_or(ChangeError::SubjectConflict {
            subject: name.to_owned(),
            detail: "member-missing",
        })?;
    // The identity key of the item: fields carry `name`, enum cases
    // carry `value`.
    let identity_key = if item.get_entry("name").is_some() {
        "name"
    } else {
        "value"
    };
    let entry = item
        .get_entry(identity_key)
        .ok_or(ChangeError::SubjectConflict {
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

/// Plan the rename of one SCALAR sequence item (the entity's `identity`
/// field references) in place: only the scalar bytes change; everything
/// else stays verbatim (round 3, finding 12).
pub fn plan_scalar_item_rename(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    value: &str,
    to: &str,
) -> Result<ByteEdit, ChangeError> {
    let items = member_sequence(definition, member)?;
    let item = items
        .iter()
        .find(|item| item.as_str() == Some(value))
        .ok_or(ChangeError::SubjectConflict {
            subject: value.to_owned(),
            detail: "member-missing",
        })?;
    let (start, end) = (item.span.start.byte, item.span.end.byte);
    let original = std::str::from_utf8(&bytes[start..end])
        .map_err(|_| ChangeError::SourcePreservation {
            subject: value.to_owned(),
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
        start,
        end,
        replacement,
    })
}

/// Escape one value for a double-quoted YAML scalar body.
fn escape_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Edit one typed scalar list, including its first and last member.
/// The closed YAML frontend accepts block collections only, so removing
/// the final optional member removes its key as well as its item.
pub fn plan_scalar_list_change(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    value: &str,
    add: bool,
) -> Result<ByteEdit, ChangeError> {
    let Some(sequence) = definition.get(member) else {
        return if add {
            plan_member_append(bytes, definition, member, value)
        } else {
            Err(ChangeError::SubjectConflict {
                subject: value.to_owned(),
                detail: "member-missing",
            })
        };
    };
    let items = sequence.as_seq().ok_or(ChangeError::SourcePreservation {
        subject: member.to_owned(),
        detail: "member-not-sequence",
    })?;
    if !add && items.len() == 1 && items[0].as_str() == Some(value) {
        let (start, end, _) = member_block_range(bytes, definition, member)?;
        let entry = definition.get_entry(member).expect("member exists");
        let key_end = content_end(bytes, start, line_start_after(bytes, start));
        if bytes[start..entry.key_span.start.byte]
            .iter()
            .any(|byte| *byte != b' ')
            || bytes[entry.key_span.end.byte..key_end].contains(&b'#')
            || previous_content_is_comment(bytes, start)
            || next_content_is_comment(bytes, end)
        {
            return Err(ChangeError::SourcePreservation {
                subject: member.to_owned(),
                detail: "comment-adjacent",
            });
        }
        let mut remove_start = start
            .checked_sub(1)
            .ok_or(ChangeError::SourcePreservation {
                subject: member.to_owned(),
                detail: "member-layout",
            })?;
        if remove_start > 0 && bytes[remove_start - 1] == b'\r' {
            remove_start -= 1;
        }
        return Ok(ByteEdit {
            start: remove_start,
            end,
            replacement: String::new(),
        });
    }
    if add {
        let item = effect_item_block(bytes, definition, member, value)?;
        plan_seq_scalar_insert(bytes, definition, member, value, &item)
    } else {
        plan_seq_scalar_remove(bytes, definition, member, value)
    }
}

fn text_is_comment(line: &[u8]) -> bool {
    std::str::from_utf8(line)
        .map(|text| text.trim_start().starts_with('#'))
        .unwrap_or(false)
}

/// The end of the last member block of one spanned definition, derived
/// textually: container value spans are zero-width in the closed YAML
/// frontend, so the byte after which a sibling member may append is the
/// end of the last content line of the definition (round 5, finding 1:
/// the container span previously landed the insertion mid-token inside
/// the last item of every sequence-valued member).
fn definition_content_end(bytes: &[u8], definition: &Node) -> Result<usize, ChangeError> {
    let entries = match &definition.value {
        Value::Map(entries) if !entries.is_empty() => entries,
        _ => {
            return Err(ChangeError::SourcePreservation {
                subject: String::new(),
                detail: "item-layout",
            })
        }
    };
    let last = entries.last().ok_or(ChangeError::SourcePreservation {
        subject: String::new(),
        detail: "item-layout",
    })?;
    match member_block_range(bytes, definition, &last.key) {
        Ok((_, end, _)) => Ok(end),
        Err(ChangeError::SourcePreservation {
            detail: "member-empty",
            ..
        }) => Ok(last.value.span.end.byte),
        Err(other) => Err(other),
    }
}

/// Insert a reviewed member block without reformatting its owner. The
/// caller supplies only the value suffix after the colon (including the
/// line break and indentation for a block value).
pub(crate) fn plan_raw_member_append(
    bytes: &[u8],
    parent: &Node,
    member: &str,
    suffix: &str,
) -> Result<ByteEdit, ChangeError> {
    if parent.get(member).is_some() {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-present",
        });
    }
    let Value::Map(entries) = &parent.value else {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "item-layout",
        });
    };
    let last = entries.last().ok_or(ChangeError::WriteBoundaryDenied)?;
    let end = inline_trivia_end(bytes, definition_content_end(bytes, parent)?);
    if next_content_is_comment(bytes, end) {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "comment-adjacent",
        });
    }
    Ok(ByteEdit {
        start: end,
        end,
        replacement: format!(
            "{}{}{member}:{suffix}",
            newline_near(bytes, end),
            " ".repeat(last.key_span.start.column - 1)
        ),
    })
}

/// Replace one block member only when its trivia has unambiguous ownership.
pub(crate) fn plan_raw_member_replace(
    bytes: &[u8],
    parent: &Node,
    member: &str,
    suffix: &str,
) -> Result<ByteEdit, ChangeError> {
    let (start, end, indent) = member_block_range(bytes, parent, member)?;
    let key = parent
        .get_entry(member)
        .ok_or(ChangeError::WriteBoundaryDenied)?;
    if bytes[start..key.key_span.start.byte]
        .iter()
        .any(|byte| *byte != b' ')
        || bytes[start..end].contains(&b'#')
        || next_content_is_comment(bytes, end)
    {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "comment-adjacent",
        });
    }
    Ok(ByteEdit {
        start,
        end,
        replacement: format!("{}{member}:{suffix}", " ".repeat(indent)),
    })
}

/// Cut a whole declaration only at proven block-sequence boundaries.
/// Internal comments travel with the declaration; adjacent outside comments
/// refuse because their ownership cannot be established.
pub(crate) fn plan_definition_cut(
    bytes: &[u8],
    node: &Node,
) -> Result<(ByteEdit, String), ChangeError> {
    let (dash, end) = item_range(bytes, node).ok_or(ChangeError::SourcePreservation {
        subject: "symbol".to_owned(),
        detail: "item-layout",
    })?;
    let end = inline_trivia_end(bytes, end);
    if next_content_is_comment(bytes, end) || previous_content_is_comment(bytes, dash) {
        return Err(ChangeError::SourcePreservation {
            subject: "symbol".to_owned(),
            detail: "comment-adjacent",
        });
    }
    let line_start = line_start_at(bytes, dash);
    if line_start == 0 || bytes[line_start..dash].iter().any(|byte| *byte != b' ') {
        return Err(ChangeError::SourcePreservation {
            subject: "symbol".to_owned(),
            detail: "item-layout",
        });
    }
    let block = std::str::from_utf8(&bytes[line_start..end])
        .map_err(|_| ChangeError::WriteBoundaryDenied)?
        .to_owned();
    let mut start = line_start - 1;
    if start > 0 && bytes[start - 1] == b'\r' {
        start -= 1;
    }
    Ok((
        ByteEdit {
            start,
            end,
            replacement: String::new(),
        },
        block,
    ))
}

/// A trailing comment belongs to the value on its own physical line.
/// Insertions go after it; a declaration relocation takes it along.
fn inline_trivia_end(bytes: &[u8], end: usize) -> usize {
    let line_end = line_start_after(bytes, end);
    let mut content = line_end;
    while content > end && matches!(bytes[content - 1], b'\r' | b'\n') {
        content -= 1;
    }
    let first = bytes[end..content]
        .iter()
        .copied()
        .find(|byte| !matches!(byte, b' ' | b'\t'));
    if matches!(first, None | Some(b'#')) {
        content
    } else {
        end
    }
}

/// Use the physical line's terminator, or the previous one at EOF. Unlike
/// `newline_before`, this accepts a content-end insertion anchor.
fn newline_near(bytes: &[u8], offset: usize) -> &'static str {
    let boundary = bytes[offset..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|index| offset + index)
        .or_else(|| bytes[..offset].iter().rposition(|byte| *byte == b'\n'));
    if boundary.is_some_and(|index| index > 0 && bytes[index - 1] == b'\r') {
        "\r\n"
    } else {
        "\n"
    }
}

/// Plan the insertion of one missing member block (with its first item)
/// after the last member of one definition. Used when an operation adds
/// the first item of a sequence the definition does not carry yet.
pub fn plan_member_append(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    first_item: &str,
) -> Result<ByteEdit, ChangeError> {
    if definition.get(member).is_some() {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "member-present",
        });
    }
    let entries = match &definition.value {
        Value::Map(entries) if !entries.is_empty() => entries,
        _ => {
            return Err(ChangeError::SourcePreservation {
                subject: member.to_owned(),
                detail: "item-layout",
            })
        }
    };
    let last = entries.last().ok_or(ChangeError::SourcePreservation {
        subject: member.to_owned(),
        detail: "item-layout",
    })?;
    let insert_at = definition_content_end(bytes, definition)?;
    if next_content_is_comment(bytes, insert_at) {
        return Err(ChangeError::SourcePreservation {
            subject: member.to_owned(),
            detail: "comment-adjacent",
        });
    }
    // The member key sits two columns under the definition's dash; its
    // items two columns under the key.
    let key_column =
        last.key_span
            .start
            .column
            .checked_sub(1)
            .ok_or(ChangeError::SourcePreservation {
                subject: member.to_owned(),
                detail: "item-layout",
            })?;
    let key_indent = " ".repeat(key_column);
    let item_indent = " ".repeat(key_column + 2);
    let newline = newline_before(bytes, bytes.len());
    let mut block = format!(
        "{newline}{key_indent}{member}:{newline}{item_indent}- {}",
        quote_yaml(first_item)
    );
    let _ = &mut block;
    Ok(ByteEdit {
        start: insert_at,
        end: insert_at,
        replacement: block,
    })
}

/// Case edits share the same literal-token and member-boundary proofs as
/// defaults. No-op values retain all original quoting and comments.
pub fn plan_case_change(
    bytes: &[u8],
    definition: &Node,
    name: &str,
    change: &super::request::CaseChangeDecl,
) -> Result<Vec<ByteEdit>, ChangeError> {
    let items = member_sequence(definition, "values")?;
    let item = items
        .iter()
        .find(|item| item.get("value").and_then(Node::as_str) == Some(name))
        .ok_or(ChangeError::SourcePreservation {
            subject: name.to_owned(),
            detail: "member-missing",
        })?;
    let mut requested = Vec::new();
    if let Some(rename) = &change.rename {
        requested.push(("value", serde_json::Value::String(rename.clone())));
    }
    if let Some(description) = &change.description {
        requested.push((
            "description",
            serde_json::Value::String(description.clone()),
        ));
    }
    if let Some(value) = &change.value {
        requested.push(("wire_value", value.to_json_value()));
    }
    let indent = item
        .get_entry("value")
        .ok_or(ChangeError::WriteBoundaryDenied)?
        .key_span
        .start
        .column
        - 1;
    let mut edits = Vec::new();
    for (key, value) in requested {
        if let Some(existing) = item.get(key) {
            let existing = crate::ir::DefaultValue::from_node(existing).ok_or(
                ChangeError::SourcePreservation {
                    subject: name.to_owned(),
                    detail: "literal-layout",
                },
            )?;
            if existing.as_json() == &value {
                continue;
            }
            let mut suffix =
                yaml_literal_suffix(&value, indent, newline_before(bytes, bytes.len()))?;
            if key == "value" {
                let alias = value.as_str().ok_or(ChangeError::WriteBoundaryDenied)?;
                let original = item.get(key).ok_or(ChangeError::WriteBoundaryDenied)?;
                suffix = match bytes.get(original.span.start.byte) {
                    Some(b'\'') => format!(" '{alias}'"),
                    Some(b'"') => suffix,
                    _ => format!(" {alias}"),
                };
            }
            edits.push(replace_literal_member(bytes, item, key, &suffix)?);
        } else {
            let suffix = yaml_literal_suffix(&value, indent, newline_before(bytes, bytes.len()))?;
            edits.push(plan_raw_member_append(bytes, item, key, &suffix)?);
        }
    }
    for (i, left) in edits.iter().enumerate() {
        if edits
            .iter()
            .skip(i + 1)
            .any(|right| left.start < right.end && right.start < left.end)
        {
            return Err(ChangeError::SourcePreservation {
                subject: name.to_owned(),
                detail: "edit-overlap",
            });
        }
    }
    Ok(edits)
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
    fn nested_collection_spans_do_not_truncate_the_last_sequence_item() {
        let text = DOC.replace("type: \"tasks.bool\"", "type:\n          ref: tasks.bool");
        let (root, bytes) = parse(&text);
        let definition = definition_of(&root);
        let item = yaml_field_item(&bytes, 6, "priority", "tasks.text", false, None);
        let patch = plan_seq_insert(&bytes, definition, "fields", &item).expect("patch");
        let after = patch.apply(&bytes);
        let after = String::from_utf8(after).expect("UTF-8");
        assert!(
            after.contains("          ref: tasks.bool\n      - name: \"priority\""),
            "{after}"
        );
        parse(&after);
    }

    #[test]
    fn returns_replacement_preserves_crlf_without_a_stray_line_break() {
        let text = "definitions:\r\n  - id: tasks.all\r\n    kind: query\r\n    version: 1\r\n    returns:\r\n      ref: tasks.text\r\n";
        let (root, bytes) = parse(text);
        let query = &root.get("definitions").unwrap().as_seq().unwrap()[0];
        let patch = plan_returns_replace(&bytes, query, "tasks.text?").expect("patch");
        let after = String::from_utf8(patch.apply(&bytes)).unwrap();
        assert!(
            after.contains("    version: 1\r\n    returns:\r\n      optional:"),
            "{after:?}"
        );
        assert!(!after.replace("\r\n", "").contains('\n'));
        parse(&after);
    }

    #[test]
    fn insert_appends_after_the_last_field_and_preserves_all_other_bytes() {
        let (root, bytes) = parse(DOC);
        let definition = definition_of(&root);
        let items = member_sequence(definition, "fields").expect("fields");
        let (anchor_column, _) = seq_anchor(&bytes, items).expect("anchor");
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
        let (anchor_column, _) = seq_anchor(&bytes, items).expect("anchor");
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
        let (anchor_column, _) = seq_anchor(&bytes, items).expect("anchor");
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

    /// Round 5, finding 1: appending to an existing QUOTED scalar list
    /// (a command with non-empty `effects`) must anchor at the dash
    /// column and re-parse cleanly — the previous column arithmetic
    /// anchored scalar items two columns too deep and corrupted the file.
    #[test]
    fn scalar_list_append_after_quoted_items_reparses() {
        let doc = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.archive\n    kind: command\n    version: 1\n    effects:\n      - \"tasks.archive.done\"\n    input:\n      - name: tasks\n        type: \"tasks.task\"\n";
        let (root, bytes) = parse(doc);
        let definitions = root.get("definitions").expect("definitions");
        let Value::Seq(items) = &definitions.value else {
            panic!("sequence")
        };
        let command = &items[0];
        let block = effect_item_block(&bytes, command, "effects", "tasks.audit").expect("block");
        assert_eq!(
            block, "\n      - \"tasks.audit\"",
            "anchored at the dash column"
        );
        let edit = plan_seq_scalar_insert(&bytes, command, "effects", "tasks.audit", &block)
            .expect("edit");
        let edited = String::from_utf8(edit.apply(&bytes)).expect("utf8");
        assert!(
            edited.contains("      - \"tasks.archive.done\"\n      - \"tasks.audit\"\n    input:"),
            "{edited}"
        );
        // The edited bytes must re-parse (the reparse assert is the
        // operation pipeline's corruption oracle).
        let index = LineIndex::new(&edited);
        let reparsed = parse_document(&edited, &index);
        assert!(
            matches!(reparsed, Ok(crate::loader::frontends::Parsed::Root(_))),
            "{edited}"
        );
    }

    /// Round 5, finding 1: the same for UNQUOTED scalar items — the
    /// anchor rule must not depend on the scalar's quoting style.
    #[test]
    fn scalar_list_append_after_plain_items_reparses() {
        let doc = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.archive\n    kind: command\n    version: 1\n    effects:\n      - tasks.archive.done\n      - tasks.archive.log\n";
        let (root, bytes) = parse(doc);
        let definitions = root.get("definitions").expect("definitions");
        let Value::Seq(items) = &definitions.value else {
            panic!("sequence")
        };
        let command = &items[0];
        let block = effect_item_block(&bytes, command, "effects", "tasks.audit").expect("block");
        assert_eq!(block, "\n      - \"tasks.audit\"");
        let edit = plan_seq_scalar_insert(&bytes, command, "effects", "tasks.audit", &block)
            .expect("edit");
        let edited = String::from_utf8(edit.apply(&bytes)).expect("utf8");
        assert!(
            edited.contains("      - tasks.archive.log\n      - \"tasks.audit\""),
            "{edited}"
        );
        let index = LineIndex::new(&edited);
        assert!(matches!(
            parse_document(&edited, &index),
            Ok(crate::loader::frontends::Parsed::Root(_))
        ));
    }

    /// Round 5, finding 1: a deeper-indented scalar list (dashes at
    /// column 8) anchors at ITS OWN dash column, not the fixture's.
    #[test]
    fn scalar_list_append_follows_deeper_indentation() {
        let doc = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.archive\n    kind: command\n    version: 1\n    effects:\n        - \"tasks.archive.done\"\n";
        let (root, bytes) = parse(doc);
        let definitions = root.get("definitions").expect("definitions");
        let Value::Seq(items) = &definitions.value else {
            panic!("sequence")
        };
        let command = &items[0];
        let block = effect_item_block(&bytes, command, "effects", "tasks.audit").expect("block");
        assert_eq!(block, "\n        - \"tasks.audit\"");
        let edit = plan_seq_scalar_insert(&bytes, command, "effects", "tasks.audit", &block)
            .expect("edit");
        let edited = String::from_utf8(edit.apply(&bytes)).expect("utf8");
        assert!(
            edited.contains("        - \"tasks.archive.done\"\n        - \"tasks.audit\""),
            "{edited}"
        );
        let index = LineIndex::new(&edited);
        assert!(matches!(
            parse_document(&edited, &index),
            Ok(crate::loader::frontends::Parsed::Root(_))
        ));
    }

    /// Round 5, finding 1: appending the whole `effects` member after a
    /// sequence-valued member (`input:`) must insert after the block's
    /// last content line — the zero-width container span previously
    /// landed the insertion mid-token inside the last item.
    #[test]
    fn member_append_after_a_sequence_member_reparses() {
        let doc = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.archive\n    kind: command\n    version: 1\n    input:\n      - name: tasks\n        type: \"tasks.task\"\n";
        let (root, bytes) = parse(doc);
        let definitions = root.get("definitions").expect("definitions");
        let Value::Seq(items) = &definitions.value else {
            panic!("sequence")
        };
        let command = &items[0];
        let edit = plan_member_append(&bytes, command, "effects", "tasks.audit").expect("edit");
        let edited = String::from_utf8(edit.apply(&bytes)).expect("utf8");
        assert!(
            edited.contains("        type: \"tasks.task\"\n    effects:\n      - \"tasks.audit\""),
            "{edited}"
        );
        let index = LineIndex::new(&edited);
        let reparsed = parse_document(&edited, &index);
        assert!(
            matches!(reparsed, Ok(crate::loader::frontends::Parsed::Root(_))),
            "{edited}"
        );
    }

    /// Round 5, finding 2: replacing a `returns` block that contains a
    /// comment line refuses — the comment's ownership is ambiguous and
    /// preservation wins over silent deletion.
    #[test]
    fn returns_replace_refuses_a_comment_inside_the_block() {
        let doc = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.find_task\n    kind: query\n    version: 1\n    returns:\n      ref: \"tasks.text\"\n      # keep this explanation\n";
        let (root, bytes) = parse(doc);
        let definitions = root.get("definitions").expect("definitions");
        let Value::Seq(items) = &definitions.value else {
            panic!("sequence")
        };
        let query = &items[0];
        let error = plan_returns_replace(&bytes, query, "tasks.text?").expect_err("refuses");
        assert!(
            matches!(
                error,
                ChangeError::SourcePreservation {
                    detail: "comment-adjacent",
                    ..
                }
            ),
            "got {error:?}"
        );
    }

    /// Round 5, finding 3: appending a definition under a file-trailing
    /// comment refuses — the comment would silently re-home onto the
    /// previous definition instead of closing the file.
    #[test]
    fn definition_append_refuses_a_file_trailing_comment() {
        let doc = "schema_version: \"0.2.16\"\ndefinitions:\n  - id: tasks.find_task\n    kind: query\n    version: 1\n    returns:\n      ref: \"tasks.text\"\n# list open work\n";
        let (root, bytes) = parse(doc);
        let definitions = root.get("definitions").expect("definitions");
        let Value::Seq(items) = &definitions.value else {
            panic!("sequence")
        };
        let nodes: Vec<&Node> = items.iter().collect();
        let error =
            plan_document_definition_append(&bytes, &nodes, "- id: x\n").expect_err("refuses");
        assert!(
            matches!(
                error,
                ChangeError::SourcePreservation {
                    detail: "comment-adjacent",
                    ..
                }
            ),
            "got {error:?}"
        );
    }
}
