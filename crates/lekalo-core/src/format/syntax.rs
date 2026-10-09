//! Trivia extraction over the original document bytes (issue #94).
//!
//! A lossless sidecar covers every byte the semantic nodes do not own:
//! comment tokens and blank separator lines. A `#` opens a comment only
//! outside every scalar span — quoted hashes, plain-scalar hashes, and
//! literal/folded block content are string data, never comments.

use crate::loader::frontends::{Node, Value};

/// One comment token with its byte span and normalized content.
#[derive(Clone, Debug)]
pub(super) struct CommentToken {
    /// 1-based line the comment starts on.
    pub line: usize,
    /// Byte offset of the `#` (diagnostic provenance).
    #[allow(dead_code)]
    pub start: usize,
    /// Byte offset one past the comment (end of line, before the EOL).
    #[allow(dead_code)]
    pub end: usize,
    /// The trimmed text after `#`.
    pub text: String,
    /// Whether only whitespace precedes the comment on its line.
    pub standalone: bool,
}

/// Per-line trivia facts.
#[derive(Clone, Copy, Debug)]
pub(super) struct LineFacts {
    /// Byte offset of the first byte of the line.
    pub start: usize,
    /// Byte offset one past the last content byte (EOL and any CR removed).
    pub end: usize,
    /// Whether the line is whitespace-only outside every scalar span.
    pub blank: bool,
}

/// The extracted trivia of one document.
#[derive(Clone, Debug)]
pub(super) struct Trivia {
    pub comments: Vec<CommentToken>,
    #[allow(dead_code)] // per-line facts back the blank-line gates in tests
    pub lines: Vec<LineFacts>,
    /// 1-based blank separator lines (outside scalar spans).
    pub blank_lines: std::collections::BTreeSet<usize>,
}

impl Trivia {
    /// Whether any blank line lies strictly between two 1-based lines.
    pub fn blank_between(&self, from_exclusive: usize, to_exclusive: usize) -> bool {
        (from_exclusive + 1..to_exclusive).any(|line| self.blank_lines.contains(&line))
    }
}

/// Collect the byte spans of every scalar token (values and mapping keys).
///
/// Each quoted-scalar span is clamped to its true token extent (finding
/// r4 1): `saphyr-parser` 0.0.8 extends a quoted scalar's token span
/// through the trailing whitespace *and the end-of-line comment* after
/// the closing quote (`scanner.rs` runs `skip_ws_to_eol` after the right
/// quote is eaten, before the span's end mark is taken). Trusting that
/// span verbatim masked every trailing `#` comment behind
/// `inside_spans`, so the comment was silently destroyed in every mode —
/// neither preserved nor reported, violating the comment-preservation
/// AC. The clamp recovers the exact quoted-token extent (through the
/// closing quote), so comments after it tokenize again while hashes
/// inside the quotes stay string data.
fn scalar_spans(text: &str, root: &Node, out: &mut Vec<(usize, usize)>) {
    match &root.value {
        Value::Scalar(_) => out.push(clamped_scalar_span(text, root.span.start.byte, root.span.end.byte)),
        Value::Seq(items) => {
            for item in items {
                scalar_spans(text, item, out);
            }
        }
        Value::Map(entries) => {
            for entry in entries {
                out.push(clamped_scalar_span(
                    text,
                    entry.key_span.start.byte,
                    entry.key_span.end.byte,
                ));
                scalar_spans(text, &entry.value, out);
            }
        }
    }
    // Degenerate spans (parser EOF artifacts) own nothing.
    out.retain(|(start, end)| end > start);
}

/// The true token extent of one scalar token span. Only a span opening
/// with a quote character is clamped: a quoted scalar's parser span is
/// the only one the scanner extends past the token (through trailing
/// whitespace and the EOL comment). Plain scalars already stop before a
/// ` #` comment, and block-scalar content owns its whole lines, so both
/// keep the parser span verbatim. Clamping a quoted span to the position
/// after its closing quote can never cut string data: the excluded
/// region is, by construction, only whitespace and the comment the
/// scanner folded in.
fn clamped_scalar_span(text: &str, start: usize, end: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let start = start.min(bytes.len());
    let end = end.min(bytes.len()).max(start);
    match bytes.get(start) {
        Some(quote @ (b'"' | b'\'')) => match quoted_token_end(bytes, start, end, *quote) {
            Some(token_end) => (start, token_end),
            None => (start, end),
        },
        _ => (start, end),
    }
}

/// The byte offset one past the closing quote of the quoted token that
/// starts at `start`, honoring `\\` escapes in double-quoted and `''`
/// doubling in single-quoted content. The scan never runs past `bound`
/// (the parser's own end mark covers the whole token); `None` keeps the
/// parser span.
fn quoted_token_end(bytes: &[u8], start: usize, bound: usize, quote: u8) -> Option<usize> {
    let mut position = start + 1;
    while position < bound {
        match bytes[position] {
            b'\\' if quote == b'"' => position += 2,
            byte if byte == quote => {
                if quote == b'\'' && bytes.get(position + 1) == Some(&b'\'') {
                    position += 2;
                } else {
                    return Some(position + 1);
                }
            }
            _ => position += 1,
        }
    }
    None
}

fn inside_spans(spans: &[(usize, usize)], position: usize) -> bool {
    spans
        .binary_search_by(|(start, end)| {
            if position < *start {
                std::cmp::Ordering::Greater
            } else if position >= *end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Extract comments and blank-line facts from the original text.
pub(super) fn extract(text: &str, root: &Node) -> Trivia {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    scalar_spans(text, root, &mut spans);
    spans.sort_unstable();
    spans.dedup();

    // Line table: content excludes the EOL and a trailing CR.
    let mut lines: Vec<LineFacts> = Vec::new();
    let mut cursor = 0usize;
    for raw_line in text.split('\n') {
        let start = cursor;
        cursor += raw_line.len() + 1;
        let mut end = start + raw_line.len();
        if end > start && text.as_bytes()[end - 1] == b'\r' {
            end -= 1;
        }
        let blank = text[start..end]
            .bytes()
            .all(|byte| byte.is_ascii_whitespace())
            && !inside_spans(&spans, start)
            && !((start..end).any(|position| inside_spans(&spans, position)));
        lines.push(LineFacts { start, end, blank });
    }
    // The final split segment after a trailing newline is empty; it owns
    // no bytes and is never a separator.
    if let Some(last) = lines.last_mut() {
        if last.start >= text.len() {
            last.blank = false;
        }
    }

    let mut blank_lines = std::collections::BTreeSet::new();
    for (index, line) in lines.iter().enumerate() {
        if line.blank {
            blank_lines.insert(index + 1);
        }
    }

    // Comment tokens: a `#` opens a comment exactly when it is outside
    // every scalar span and not already part of a comment on this line.
    let mut comments: Vec<CommentToken> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let mut comment_end = None;
        for position in line.start..line.end {
            if text.as_bytes()[position] != b'#' {
                continue;
            }
            if inside_spans(&spans, position) {
                continue;
            }
            if let Some(end) = comment_end {
                if position < end {
                    continue;
                }
            }
            comment_end = Some(line.end);
            comments.push(CommentToken {
                line: index + 1,
                start: position,
                end: line.end,
                text: text[position + 1..line.end].trim().to_owned(),
                standalone: text[line.start..position]
                    .bytes()
                    .all(|byte| byte.is_ascii_whitespace()),
            });
        }
    }

    Trivia {
        comments,
        lines,
        blank_lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::frontends::parse_document;
    use crate::loader::source::LineIndex;

    fn trivia_of(text: &str) -> Trivia {
        let index = LineIndex::new(text);
        let parsed = parse_document(text, &index).expect("parse");
        let root = match parsed {
            crate::loader::frontends::Parsed::Root(root) => *root,
            crate::loader::frontends::Parsed::Empty => panic!("empty"),
        };
        extract(text, &root)
    }

    fn comment_texts(text: &str) -> Vec<String> {
        trivia_of(text)
            .comments
            .into_iter()
            .map(|token| token.text)
            .collect()
    }

    #[test]
    fn comments_outside_scalars_are_tokens() {
        assert_eq!(comment_texts("# header\na: 1 # tail\n"), ["header", "tail"]);
        assert_eq!(comment_texts("a: 1\n# between\nb: 2\n"), ["between"]);
        assert!(comment_texts("a: 1\n").is_empty());
    }

    #[test]
    fn quoted_and_plain_hashes_are_string_data() {
        // Quoted string hash.
        assert!(comment_texts("a: \"x # y\"\n").is_empty());
        // Plain scalar hash without a preceding space is part of the token.
        assert!(comment_texts("a: x#y\n").is_empty());
        // Plain scalar hash after a space opens a comment.
        assert_eq!(comment_texts("a: x # y\n"), ["y"]);
        // Block scalar content is string data.
        assert!(comment_texts("a: |\n  # not a comment\n  text\n").is_empty());
        // Quoted key hash.
        assert!(comment_texts("\"k#y\": 1\n").is_empty());
    }

    #[test]
    fn trailing_comments_after_quoted_scalars_are_tokens() {
        // Finding r4 1: the parser extends a quoted scalar's token span
        // through the EOL comment after the closing quote. The clamp
        // recovers the comment so it tokenizes in every scalar style.
        assert_eq!(comment_texts("a: \"x\" # c\n"), ["c"]);
        assert_eq!(comment_texts("a: 'x' # c\n"), ["c"]);
        assert_eq!(comment_texts("\"a\": \"x\" # c\n"), ["c"]);
        // Hashes inside the quotes stay string data.
        assert!(comment_texts("a: \"x # not c\"\n").is_empty());
        assert!(comment_texts("a: 'x # not c'\n").is_empty());
        // Empty and doubled-quote spellings still clamp exactly.
        assert_eq!(comment_texts("a: \"\" # c\n"), ["c"]);
        assert_eq!(comment_texts("a: 'it''s' # c\n"), ["c"]);
        // Escaped quote then comment.
        assert_eq!(comment_texts("a: \"x\\\"y\" # c\n"), ["c"]);
        // Nested maps and sequence items.
        assert_eq!(comment_texts("outer:\n  inner: \"v\" # c\n"), ["c"]);
        assert_eq!(comment_texts("list:\n  - \"one\" # c\n"), ["c"]);
        // No trailing comment: span keeps the parser extent.
        assert!(comment_texts("a: \"x\"\n").is_empty());
    }

    #[test]
    fn blank_lines_exclude_block_scalar_interiors() {
        let facts = trivia_of("a:\n  b: 1\n\nc: 2\n");
        assert!(facts.blank_between(1, 4), "blank line 3 separates");
        let block = trivia_of("a: |\n  text\n\n  more\n");
        assert!(
            !block.blank_between(1, 5),
            "a blank line inside a literal block is content"
        );
    }
}
