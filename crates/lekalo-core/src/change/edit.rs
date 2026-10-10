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

/// Build the YAML block of one effect-reference item (a scalar string
/// list entry) at the anchor of the named member sequence.
pub fn effect_item_block(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    effect_id: &str,
) -> Result<String, ChangeError> {
    let items = member_sequence(definition, member)?;
    let (anchor, _) = seq_anchor(items)?;
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
    let (_, insert_at) = seq_anchor(items)?;
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
    let (dash, item_end) = item_range(&items[position]).ok_or(ChangeError::SourcePreservation {
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
pub fn definitions_anchor(definitions: &[Node]) -> Result<(usize, usize), ChangeError> {
    seq_anchor(definitions)
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

/// Plan the replace of one scalar member inside one sequence item
/// (the case description). A missing member refuses: adding a member
/// whose ownership layout is unproven would guess.
pub fn plan_member_scalar_replace(
    _bytes: &[u8],
    definition: &Node,
    member: &str,
    name: &str,
    scalar: &str,
    new_text: &str,
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
    let entry = item
        .get_entry(scalar)
        .ok_or(ChangeError::SourcePreservation {
            subject: name.to_owned(),
            detail: "member-absent",
        })?;
    let span = entry.value.span;
    Ok(ByteEdit {
        start: span.start.byte,
        end: span.end.byte,
        replacement: format!("\"{}\"", escape_quoted(new_text)),
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

/// Plan the replacement of one named member sequence's items with a new
/// field list. The whole member block (key line plus indented items) is
/// replaced textually, so container spans never enter the calculation.
pub fn plan_member_sequence_replace(
    bytes: &[u8],
    definition: &Node,
    member: &str,
    fields: &[(&str, &str, bool)],
) -> Result<ByteEdit, ChangeError> {
    let (start, end, key_indent) = member_block_range(bytes, definition, member)?;
    let item_indent = " ".repeat(key_indent + 2);
    let newline = newline_before(bytes, end);
    let mut block = format!("{newline}{}{member}:", " ".repeat(key_indent));
    for (name, type_spelling, required) in fields {
        block.push_str(newline);
        block.push_str(&item_indent);
        block.push_str("- name: ");
        block.push_str(&quote_yaml(name));
        block.push_str(newline);
        block.push_str(&item_indent);
        block.push_str("  type: ");
        block.push_str(&quote_yaml(type_spelling));
        if *required {
            block.push_str(newline);
            block.push_str(&item_indent);
            block.push_str("  required: true");
        }
    }
    Ok(ByteEdit {
        start,
        end,
        replacement: block,
    })
}

/// Plan the replacement of the whole `returns` member block with the new
/// type mapping.
pub fn plan_returns_replace(
    bytes: &[u8],
    definition: &Node,
    type_spelling: &str,
) -> Result<ByteEdit, ChangeError> {
    let (start, end, key_indent) = member_block_range(bytes, definition, "returns")?;
    let block = yaml_returns_block(bytes, key_indent - 2, type_spelling);
    // The replaced range starts at the key line; the block's leading
    // newline is not part of it.
    Ok(ByteEdit {
        start,
        end,
        replacement: block[1..].to_owned(),
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
    fields: &[(&str, &str, bool)],
) -> String {
    let inner = " ".repeat(anchor_column + 2);
    let item_indent = " ".repeat(anchor_column + 4);
    let newline = newline_before(bytes, bytes.len());
    let mut out = String::new();
    out.push_str(newline);
    out.push_str(&inner);
    out.push_str(member);
    out.push(':');
    for (name, type_spelling, required) in fields {
        out.push_str(newline);
        out.push_str(&item_indent);
        out.push_str("- name: ");
        out.push_str(&quote_yaml(name));
        out.push_str(newline);
        out.push_str(&item_indent);
        out.push_str("  type: ");
        out.push_str(&quote_yaml(type_spelling));
        if *required {
            out.push_str(newline);
            out.push_str(&item_indent);
            out.push_str("  required: true");
        }
    }
    out
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

/// Escape one value for a double-quoted YAML scalar body.
fn escape_quoted(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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
    let insert_at = last.value.span.end.byte;
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
