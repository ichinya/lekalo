//! The canonical YAML/JSON renderer (issue #94).
//!
//! The renderer re-emits the parsed spanned tree under the pinned policy:
//! two-space block style, double-quoted strings, byte-sorted mapping keys
//! and semantic-ID-sorted definitions within retained blank-line groups,
//! set-like import sorting, LF endings, and one final newline. Comments
//! ride their owners; a comment without an owner becomes an explicit
//! loss, never a silent deletion. The renderer never reorders
//! behaviorally ordered arrays.

use crate::loader::frontends::{MapEntry, Node, Scalar, Value};

use super::policy::Policy;
use super::syntax::Trivia;
use super::trivia::{LossMark, LossReason, Ownership};

/// The retained source syntax of one document.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Syntax {
    Yaml,
    Json,
}

impl Syntax {
    /// Dialect selection mirrors the frontend rule: the first
    /// non-whitespace byte `{` or `[` selects JSON. The root span starts
    /// at that first content byte.
    pub fn of(text: &str, root: &Node) -> Syntax {
        let start = root.span.start.byte.min(text.len());
        match text.as_bytes().get(start) {
            Some(b'{') | Some(b'[') => Syntax::Json,
            _ => Syntax::Yaml,
        }
    }
}

/// One rendered candidate with its trivia losses.
pub struct Rendered {
    /// The retained syntax of the document.
    pub syntax: Syntax,
    pub text: String,
    pub losses: Vec<LossMark>,
}

/// How a sequence is ordered by the policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SeqOrder {
    /// Set-like imports: byte-ascending, stable.
    SortImports,
    /// `definitions` members: full semantic ID within groups, stable.
    SortByItemId,
    /// Behaviorally ordered content: source order is semantic.
    Preserve,
}

/// Render one admitted document into canonical text.
pub(super) fn render(
    _policy: &Policy,
    root: &Node,
    syntax: Syntax,
    text: &str,
) -> Result<Rendered, crate::result::DomainResult> {
    let trivia = super::syntax::extract(text, root);
    let ownership = Ownership::assign(&trivia, root);
    let mut renderer = Renderer {
        syntax,
        trivia: &trivia,
        ownership: &ownership,
        consumed: vec![false; trivia.comments.len()],
        out: String::new(),
        losses: ownership.losses.clone(),
    };
    // Header decoration has a fixed stable placement: before everything.
    let header: Vec<usize> = ownership.header.clone();
    for index in header {
        renderer.write_comment_line(index, 0);
    }
    match &root.value {
        Value::Map(entries) if !entries.is_empty() => match syntax {
            Syntax::Yaml => renderer.write_map(entries, 0, false),
            Syntax::Json => renderer.write_json_map(entries, 0, None),
        },
        Value::Seq(items) if !items.is_empty() && syntax == Syntax::Json => {
            renderer.write_json_seq(items, 0, None)
        }
        _ => {
            return Err(super::failure_semantic("render", "root-not-mapping"));
        }
    }
    // Exactly one final newline for every produced document.
    if !renderer.out.ends_with('\n') {
        renderer.out.push('\n');
    }
    // Footer decoration: after everything, each on its own final line.
    let footer: Vec<usize> = ownership.footer.clone();
    for index in footer {
        renderer.write_comment_line(index, 0);
    }
    // Any comment the renderer could not place is an explicit loss.
    for (index, token) in trivia.comments.iter().enumerate() {
        if !renderer.consumed[index] && !renderer.losses.iter().any(|loss| loss.line == token.line)
        {
            renderer.losses.push(LossMark {
                line: token.line,
                reason: LossReason::AmbiguousOwner,
            });
        }
    }
    renderer.losses.sort_by_key(|loss| loss.line);
    renderer.losses.dedup_by_key(|loss| loss.line);
    Ok(Rendered {
        syntax,
        text: renderer.out,
        losses: renderer.losses,
    })
}

struct Renderer<'a> {
    syntax: Syntax,
    trivia: &'a Trivia,
    ownership: &'a Ownership,
    consumed: Vec<bool>,
    out: String,
    losses: Vec<LossMark>,
}

/// One entry of an ordered container: original index plus its group.
struct Ordered {
    index: usize,
    group: usize,
}

impl<'a> Renderer<'a> {
    // ---------- ordering ----------

    /// The last source line owned by one node's subtree. Block-collection
    /// spans carry only their start position, so the extent is derived
    /// recursively; blank lines inside an extent never split the
    /// surrounding container's groups.
    fn extent(&self, node: &Node) -> usize {
        match &node.value {
            Value::Scalar(_) => node.span.end.line.max(node.span.start.line),
            Value::Seq(items) => items
                .iter()
                .map(|item| self.extent(item))
                .max()
                .unwrap_or(node.span.start.line),
            Value::Map(entries) => entries
                .iter()
                .map(|entry| {
                    self.extent(&entry.value)
                        .max(entry.key_span.end.line)
                })
                .max()
                .unwrap_or(node.span.start.line),
        }
    }

    /// The blank-line group partition of consecutive owner lines. A group
    /// boundary is a blank line strictly between the previous member's
    /// subtree extent and the next member's start line.
    fn groups_of(&self, members: &[(usize, usize)]) -> Vec<(usize, usize)> {
        let mut result: Vec<(usize, usize)> = Vec::new();
        let mut previous_extent: Option<usize> = None;
        let mut previous_group = 0usize;
        for &(start, extent) in members {
            let group = match previous_extent {
                Some(previous) => {
                    if self.trivia.blank_between(previous, start) {
                        previous_group + 1
                    } else {
                        previous_group
                    }
                }
                None => 0,
            };
            previous_extent = Some(extent);
            previous_group = group;
            result.push((start, group));
        }
        result
    }

    /// Mapping entries: byte-ascending keys within retained groups; groups
    /// keep their original relative order.
    fn ordered_entries(&self, entries: &[MapEntry]) -> Vec<Ordered> {
        let members: Vec<(usize, usize)> = entries
            .iter()
            .map(|entry| {
                (
                    entry.key_span.start.line,
                    self.extent(&entry.value).max(entry.key_span.end.line),
                )
            })
            .collect();
        let groups = self.groups_of(&members);
        let mut ordered: Vec<Ordered> = groups
            .into_iter()
            .enumerate()
            .map(|(index, (_, group))| Ordered { index, group })
            .collect();
        ordered.sort_by(|left, right| {
            left.group
                .cmp(&right.group)
                .then_with(|| {
                    entries[left.index]
                        .key
                        .as_bytes()
                        .cmp(entries[right.index].key.as_bytes())
                })
                .then_with(|| left.index.cmp(&right.index))
        });
        ordered
    }

    /// Sequence items under the policy ordering of their owner key.
    fn ordered_items(&self, items: &[Node], owner_key: Option<&str>) -> Vec<Ordered> {
        let ordering = match owner_key {
            Some("imports") => SeqOrder::SortImports,
            Some("definitions") => SeqOrder::SortByItemId,
            _ => SeqOrder::Preserve,
        };
        let members: Vec<(usize, usize)> = items
            .iter()
            .map(|item| (item.span.start.line, self.extent(item)))
            .collect();
        let groups = self.groups_of(&members);
        let mut ordered: Vec<Ordered> = groups
            .into_iter()
            .enumerate()
            .map(|(index, (_, group))| Ordered { index, group })
            .collect();
        match ordering {
            SeqOrder::Preserve => {}
            SeqOrder::SortImports => {
                ordered.sort_by(|left, right| {
                    left.group
                        .cmp(&right.group)
                        .then_with(|| {
                            items[left.index]
                                .as_str()
                                .unwrap_or_default()
                                .as_bytes()
                                .cmp(items[right.index].as_str().unwrap_or_default().as_bytes())
                        })
                        .then_with(|| left.index.cmp(&right.index))
                });
            }
            SeqOrder::SortByItemId => {
                ordered.sort_by(|left, right| {
                    left.group
                        .cmp(&right.group)
                        .then_with(|| {
                            item_id(&items[left.index]).cmp(&item_id(&items[right.index]))
                        })
                        .then_with(|| left.index.cmp(&right.index))
                });
            }
        }
        ordered
    }

    // ---------- trivia ----------

    fn write_comment_line(&mut self, index: usize, indent: usize) {
        if self.consumed[index] {
            return;
        }
        let text = self.trivia.comments[index].text.clone();
        self.push_indent(indent);
        if text.is_empty() {
            self.out.push('#');
        } else {
            self.out.push_str("# ");
            self.out.push_str(&text);
        }
        self.out.push('\n');
        self.consumed[index] = true;
    }

    /// The leading comment block of one owner line.
    fn write_leading(&mut self, owner_line: usize, indent: usize) {
        let indices: Vec<usize> = self.ownership.leading_of(owner_line).to_vec();
        for index in indices {
            self.write_comment_line(index, indent);
        }
    }

    /// The same-line comment of one owner line, with one separating space.
    fn write_inline(&mut self, owner_line: usize) {
        if let Some(index) = self.ownership.inline_of(owner_line) {
            if !self.consumed[index] {
                let text = self.trivia.comments[index].text.clone();
                self.out.push(' ');
                if text.is_empty() {
                    self.out.push('#');
                } else {
                    self.out.push_str("# ");
                    self.out.push_str(&text);
                }
                self.consumed[index] = true;
            }
        }
    }

    fn push_indent(&mut self, indent: usize) {
        for _ in 0..indent {
            self.out.push(' ');
        }
    }

    // ---------- YAML ----------

    fn write_map(&mut self, entries: &[MapEntry], indent: usize, first_inline: bool) {
        let ordered = self.ordered_entries(entries);
        let mut previous_group: Option<usize> = None;
        for (position, ordered_entry) in ordered.iter().enumerate() {
            let entry = &entries[ordered_entry.index];
            if position == 0 && first_inline {
                // The first entry rides the sequence dash already written.
            } else {
                if position > 0 && previous_group != Some(ordered_entry.group) {
                    self.out.push('\n');
                }
                self.write_leading(entry.key_span.start.line, indent);
                self.push_indent(indent);
            }
            previous_group = Some(ordered_entry.group);
            self.out.push_str(&quote_yaml(&entry.key));
            self.out.push(':');
            let child_key = seq_owner_key(&entry.value, &entry.key);
            self.write_value(&entry.value, indent, entry.key_span.start.line, child_key);
        }
    }

    fn write_seq(
        &mut self,
        items: &[Node],
        indent: usize,
        first_inline: bool,
        owner_key: Option<&str>,
    ) {
        let ordered = self.ordered_items(items, owner_key);
        let mut previous_group: Option<usize> = None;
        for (position, ordered_item) in ordered.iter().enumerate() {
            let item = &items[ordered_item.index];
            if position == 0 && first_inline {
                // The first item rides the outer dash already written.
            } else {
                if position > 0 && previous_group != Some(ordered_item.group) {
                    self.out.push('\n');
                }
                self.write_leading(item.span.start.line, indent);
                self.push_indent(indent);
            }
            previous_group = Some(ordered_item.group);
            self.out.push('-');
            self.write_item(item, indent, item.span.start.line);
        }
    }

    /// One sequence item after its `-` marker; the item content begins on
    /// the dash line (dash-aligned).
    fn write_item(&mut self, item: &Node, dash_indent: usize, owner_line: usize) {
        match &item.value {
            Value::Scalar(scalar) => {
                self.out.push(' ');
                self.out.push_str(&self.render_scalar(scalar));
                self.write_inline(owner_line);
                self.out.push('\n');
            }
            Value::Map(entries) if !entries.is_empty() => {
                // First entry on the dash line, remaining entries aligned
                // two columns past the dash.
                let ordered = self.ordered_entries(entries);
                let first_group = ordered[0].group;
                let mut previous_group = first_group;
                for (position, ordered_entry) in ordered.iter().enumerate() {
                    let entry = &entries[ordered_entry.index];
                    if position == 0 {
                        self.out.push(' ');
                    } else {
                        if previous_group != ordered_entry.group {
                            self.out.push('\n');
                        }
                        self.write_leading(entry.key_span.start.line, dash_indent + 2);
                        self.push_indent(dash_indent + 2);
                    }
                    previous_group = ordered_entry.group;
                    self.out.push_str(&quote_yaml(&entry.key));
                    self.out.push(':');
                    let child_key = seq_owner_key(&entry.value, &entry.key);
                    self.write_value(
                        &entry.value,
                        dash_indent + 2,
                        entry.key_span.start.line,
                        child_key,
                    );
                }
            }
            Value::Seq(items) if !items.is_empty() => {
                // Nested sequence: the first inner item rides this dash.
                self.out.push(' ');
                self.write_seq(items, dash_indent + 2, true, None);
            }
            Value::Map(_) | Value::Seq(_) => {
                // An empty collection has no accepted YAML block spelling;
                // the renderer refuses instead of emitting flow syntax.
                self.losses.push(LossMark {
                    line: owner_line,
                    reason: LossReason::UnrepresentableForm,
                });
                self.out.push_str(" null");
                self.write_inline(owner_line);
                self.out.push('\n');
            }
        }
    }

    fn write_value(
        &mut self,
        value: &Node,
        indent: usize,
        owner_line: usize,
        owner_key: Option<&str>,
    ) {
        match &value.value {
            Value::Scalar(scalar) => {
                self.out.push(' ');
                self.out.push_str(&self.render_scalar(scalar));
                self.write_inline(owner_line);
                self.out.push('\n');
            }
            Value::Map(entries) if !entries.is_empty() => {
                self.write_inline(owner_line);
                self.out.push('\n');
                self.write_map(entries, indent + 2, false);
            }
            Value::Seq(items) if !items.is_empty() => {
                self.write_inline(owner_line);
                self.out.push('\n');
                self.write_seq(items, indent + 2, false, owner_key);
            }
            Value::Map(_) | Value::Seq(_) => {
                self.losses.push(LossMark {
                    line: owner_line,
                    reason: LossReason::UnrepresentableForm,
                });
                self.out.push_str(" null");
                self.write_inline(owner_line);
                self.out.push('\n');
            }
        }
    }

    fn render_scalar(&self, scalar: &Scalar) -> String {
        match scalar {
            Scalar::Null => "null".to_owned(),
            Scalar::Bool(true) => "true".to_owned(),
            Scalar::Bool(false) => "false".to_owned(),
            Scalar::Int(value) => value.to_string(),
            Scalar::Float(value) => format!("{value:?}"),
            Scalar::Str(text) => match self.syntax {
                Syntax::Yaml => quote_yaml(text),
                Syntax::Json => quote_json(text),
            },
        }
    }

    // ---------- JSON ----------

    fn write_json_map(&mut self, entries: &[MapEntry], indent: usize, _owner_key: Option<&str>) {
        if entries.is_empty() {
            self.out.push_str("{}");
            return;
        }
        let ordered = self.ordered_entries(entries);
        self.out.push_str("{\n");
        for (position, ordered_entry) in ordered.iter().enumerate() {
            let entry = &entries[ordered_entry.index];
            if position > 0 {
                self.out.push_str(",\n");
            }
            self.push_indent(indent + 2);
            self.out.push_str(&quote_json(&entry.key));
            self.out.push_str(": ");
            let child_key = seq_owner_key(&entry.value, &entry.key);
            self.write_json_value(&entry.value, indent + 2, child_key);
        }
        self.out.push('\n');
        self.push_indent(indent);
        self.out.push('}');
    }

    fn write_json_seq(&mut self, items: &[Node], indent: usize, owner_key: Option<&str>) {
        if items.is_empty() {
            self.out.push_str("[]");
            return;
        }
        let ordered = self.ordered_items(items, owner_key);
        self.out.push_str("[\n");
        for (position, ordered_item) in ordered.iter().enumerate() {
            let item = &items[ordered_item.index];
            if position > 0 {
                self.out.push_str(",\n");
            }
            self.push_indent(indent + 2);
            self.write_json_value(item, indent + 2, None);
        }
        self.out.push('\n');
        self.push_indent(indent);
        self.out.push(']');
    }

    fn write_json_value(&mut self, node: &Node, indent: usize, owner_key: Option<&str>) {
        match &node.value {
            Value::Scalar(scalar) => self.out.push_str(&self.render_scalar(scalar)),
            Value::Seq(items) => self.write_json_seq(items, indent, owner_key),
            Value::Map(entries) => self.write_json_map(entries, indent, None),
        }
    }
}

/// Sequence values under `imports`/`definitions` carry their ordering
/// context; every other sequence is behaviorally ordered.
fn seq_owner_key<'a>(value: &'a Node, key: &'a str) -> Option<&'a str> {
    match &value.value {
        Value::Seq(_) => Some(key),
        _ => None,
    }
}

/// The semantic ID of a `definitions` item; unnamed items keep source
/// order through the stable fallback.
fn item_id(item: &Node) -> String {
    item.get("id")
        .and_then(|id| id.as_str())
        .unwrap_or_default()
        .to_owned()
}

/// Canonical YAML double-quoted spelling: exact round-trip for every
/// accepted string, including control characters and the Unicode line
/// separators a parser would otherwise fold.
fn quote_yaml(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0000}' => out.push_str("\\0"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\u{000B}' => out.push_str("\\v"),
            '\u{0085}' => out.push_str("\\N"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            control if (control as u32) < 0x20 || (control as u32) == 0x7F => {
                out.push_str(&format!("\\x{:02x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Canonical JSON string spelling (RFC 8259 escapes, raw UTF-8).
fn quote_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    crate::loader::canonical::write_json_string(text, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::policy::Policy;
    use crate::loader::frontends::parse_document;
    use crate::loader::source::LineIndex;

    fn render_with(text: &str, syntax: Syntax) -> Rendered {
        let policy = Policy::embedded().expect("policy");
        let index = LineIndex::new(text);
        let parsed = parse_document(text, &index).expect("parse");
        let root = match parsed {
            crate::loader::frontends::Parsed::Root(root) => *root,
            crate::loader::frontends::Parsed::Empty => panic!("empty"),
        };
        render(&policy, &root, syntax, text).expect("render")
    }

    fn render_yaml(text: &str) -> Rendered {
        render_with(text, Syntax::Yaml)
    }

    fn rendered_ok(text: &str) -> String {
        let rendered = render_yaml(text);
        assert!(rendered.losses.is_empty(), "unexpected losses");
        rendered.text
    }

    #[test]
    fn dialect_selection_follows_the_first_content_byte() {
        let policy = Policy::embedded().expect("policy");
        for (text, expected) in [
            ("{\"a\":1}\n", Syntax::Json),
            ("  [1]\n", Syntax::Json),
            ("a: 1\n", Syntax::Yaml),
            ("# c\na: 1\n", Syntax::Yaml),
        ] {
            let index = LineIndex::new(text);
            let parsed = parse_document(text, &index).expect("parse");
            let root = match parsed {
                crate::loader::frontends::Parsed::Root(root) => *root,
                crate::loader::frontends::Parsed::Empty => panic!("empty"),
            };
            assert_eq!(Syntax::of(text, &root), expected, "{text:?}");
            let _ = &policy;
        }
    }

    #[test]
    fn canonical_block_style_and_quoting() {
        assert_eq!(
            rendered_ok("schema_version: '0.2.16'\n"),
            "\"schema_version\": \"0.2.16\"\n"
        );
    }

    #[test]
    fn keys_sort_by_bytes_within_one_group() {
        assert_eq!(
            rendered_ok(
                "zeta: 1
alpha: 2
mu: 3
"
            ),
            "\"alpha\": 2
\"mu\": 3
\"zeta\": 1
"
        );
    }

    #[test]
    fn blank_line_groups_keep_their_relative_order() {
        assert_eq!(
            rendered_ok(
                "zeta: 1

alpha: 2
"
            ),
            "\"zeta\": 1

\"alpha\": 2
"
        );
    }

    #[test]
    fn comments_move_with_their_sorted_owner() {
        assert_eq!(
            rendered_ok(
                "zeta: 1 # stays on zeta
alpha: 2
"
            ),
            "\"alpha\": 2
\"zeta\": 1 # stays on zeta
"
        );
    }

    #[test]
    fn header_and_footer_own_their_comments() {
        assert_eq!(
            rendered_ok(
                "# header
a: 1
# footer
"
            ),
            "# header
\"a\": 1
# footer
"
        );
    }

    #[test]
    fn leading_comments_between_entries_move_with_their_owner() {
        assert_eq!(
            rendered_ok(
                "zeta: 1
# leads alpha
alpha: 2
"
            ),
            "# leads alpha
\"alpha\": 2
\"zeta\": 1
"
        );
    }

    #[test]
    fn header_comments_stay_at_the_document_top() {
        assert_eq!(
            rendered_ok("# top\nzeta: 1\nalpha: 2\n"),
            "# top\n\"alpha\": 2\n\"zeta\": 1\n"
        );
    }
    #[test]
    fn nested_maps_and_sequences_render_in_block_style() {
        assert_eq!(
            rendered_ok("outer:\n    inner: 1\nlist:\n    - one\n    - two\n"),
            "\"list\":\n  - \"one\"\n  - \"two\"\n\"outer\":\n  \"inner\": 1\n"
        );
    }
    #[test]
    fn sequence_mapping_entries_are_dash_aligned() {
        assert_eq!(
            rendered_ok("definitions:\n    - id: a\n      kind: entity\n    - id: b\n      kind: entity\n"),
            "\"definitions\":\n  - \"id\": \"a\"\n    \"kind\": \"entity\"\n  - \"id\": \"b\"\n    \"kind\": \"entity\"\n"
        );
    }
    #[test]
    fn definitions_sort_by_semantic_id_within_groups() {
        let text = "definitions:
  - id: z
    kind: scalar
  - id: a
    kind: scalar
";
        let out = rendered_ok(text);
        assert!(out.find("\"id\": \"a\"").unwrap() < out.find("\"id\": \"z\"").unwrap());
    }

    #[test]
    fn imports_sort_byte_ascending() {
        assert_eq!(
            rendered_ok(
                "imports:
  - scheduling
  - planner
"
            ),
            "\"imports\":
  - \"planner\"
  - \"scheduling\"
"
        );
    }

    #[test]
    fn ordered_arrays_are_never_reordered() {
        assert_eq!(
            rendered_ok(
                "steps:
  - focus
  - notify
  - archive
"
            ),
            "\"steps\":
  - \"focus\"
  - \"notify\"
  - \"archive\"
"
        );
    }

    #[test]
    fn multiline_strings_round_trip_through_escapes() {
        let out = rendered_ok("a: |\n  line1\n  line2\n");
        // The literal block kept its trailing newline; the escaped
        // double-quoted form preserves it exactly.
        assert_eq!(out, "\"a\": \"line1\\nline2\\n\"\n");
        assert_eq!(rendered_ok(&out), out);
    }
    #[test]
    fn crlf_input_normalizes_to_lf() {
        assert_eq!(
            rendered_ok(
                "a: 1
# c
b: 2
"
            ),
            "\"a\": 1
# c
\"b\": 2
"
        );
    }

    #[test]
    fn unicode_values_and_comments_survive() {
        assert_eq!(
            rendered_ok(
                "clé: valeurs # комментář
"
            ),
            "\"clé\": \"valeurs\" # комментář
"
        );
    }

    #[test]
    fn ambiguous_comment_is_reported_not_dropped() {
        let rendered = render_yaml(
            "a: one
  two # trailing
",
        );
        assert_eq!(rendered.losses.len(), 1);
    }

    #[test]
    fn rendering_is_idempotent_on_a_mixed_document() {
        let text = "# model
schema_version: '0.2.16'

definitions:
  - id: zz
    kind: scalar
    version: 1
  # leads aa
  - id: aa
    kind: scalar
    version: 1
";
        let once = rendered_ok(text);
        let twice = rendered_ok(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn json_documents_render_expanded_and_sorted() {
        let text = "{\"b\":1,\"a\":[1,{\"z\":null,\"a\":true}]}
";
        assert_eq!(
            render_with(text, Syntax::Json).text,
            "{
  \"a\": [
    1,
    {
      \"a\": true,
      \"z\": null
    }
  ],
  \"b\": 1
}
"
        );
    }

    #[test]
    fn float_scalars_round_trip() {
        let text = "{\"x\":1.50,\"y\":-0.0}
";
        assert_eq!(
            render_with(text, Syntax::Json).text,
            "{
  \"x\": 1.5,
  \"y\": -0.0
}
"
        );
    }

    #[test]
    fn json_definitions_and_imports_sort_like_yaml() {
        let text =
            "{\"schema_version\":\"0.2.16\",\"definitions\":[{\"id\":\"z\"},{\"id\":\"a\"}]}\n";
        let out = render_with(text, Syntax::Json).text;
        assert!(out.find("\"id\": \"a\"").unwrap() < out.find("\"id\": \"z\"").unwrap());
    }
}
