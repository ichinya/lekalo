//! Lexical primitives for the pinned OpenSpec requirement-blocks parser.
//!
//! ECMAScript whitespace is shared by fences, headings, directives and body
//! trimming. Heading labels and directive labels deliberately differ in case
//! sensitivity. See the native source receipts and supported subset in docs.

/// ECMAScript `\s` and String trim, not Rust Unicode whitespace.
pub(super) fn whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'..='\u{000d}'
            | ' '
            | '\u{00a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

pub(super) fn trim(text: &str) -> &str {
    text.trim_matches(whitespace)
}

pub(super) fn trim_start(text: &str) -> &str {
    text.trim_start_matches(whitespace)
}

pub(super) fn trim_end(text: &str) -> &str {
    text.trim_end_matches(whitespace)
}

/// Native `^##\s+`; callers reject the ambiguous empty delta heading.
pub(super) fn section_title(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("##")?;
    rest.starts_with(whitespace).then(|| trim(rest))
}

/// Native `^###\s*Requirement:\s*(.+)\s*$` (case insensitive).
/// Empty captures after native trimming are retained so title validation
/// rejects them instead of absorbing a native block into a previous body.
pub(super) fn requirement_title(line: &str) -> Option<&str> {
    let heading = trim_start(line.strip_prefix("###")?);
    if !heading.get(..12)?.eq_ignore_ascii_case("Requirement:") {
        return None;
    }
    let rest = &heading[12..];
    (!rest.is_empty()).then(|| trim(rest))
}

/// Bounded subset of the separate, case-sensitive native reference regex.
/// Support unquoted references and exactly one balanced backtick pair.
/// Native also accepts unbalanced wrappers; these ambiguous forms (and
/// backticks within names) are deliberately unsupported, never reinterpreted.
pub(super) fn header_reference(text: &str) -> Option<&str> {
    let text = trim(text);
    let text = if let Some(quoted) = text.strip_prefix('`') {
        quoted.strip_suffix('`')?
    } else {
        text
    };
    if text.contains('`') {
        return None;
    }
    let heading = trim_start(text.strip_prefix("###")?);
    let name = heading.strip_prefix("Requirement:")?;
    (!name.is_empty()).then(|| trim(name))
}

/// ECMAScript regex `.` excludes these line terminators even within a
/// newline-split line. This provider rejects them in structural lines rather
/// than approximating the native regex's whitespace backtracking.
pub(super) fn has_line_separator(line: &str) -> bool {
    line.contains(['\u{2028}', '\u{2029}'])
}
