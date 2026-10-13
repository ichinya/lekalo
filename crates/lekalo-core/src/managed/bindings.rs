//! Extension binding resolution: the real pinned implementation behind a
//! custom/foreign extension selection (issue #41).
//!
//! `validated: true` on a coverage row is evidence, not authority. The
//! selection resolves only when the pinned binding names a real project
//! file whose exact bytes carry the pinned `contractDigest` **and**
//! actually export the declared symbol with a callable value (finding
//! 6): a hash-correct file without the export is unresolved. The bound
//! implementation stays maintained source: the managed lane renders its
//! explicit invocation in the generated implementation and test, but
//! never owns, regenerates, or cleans those bytes.

use super::fs;
use super::wire::{Coverage, TargetAssignment};
use super::{DomainResult, EXTENSION_UNRESOLVED};

fn unresolved(symbol: &str, reason: &str) -> DomainResult {
    super::diagnostic::failure(
        EXTENSION_UNRESOLVED,
        &[("requirement", symbol), ("reason", reason)],
    )
}

/// The project-relative file of one binding spelling: the binding is a
/// project-relative path, optionally followed by `#symbol`. The file part
/// must be a safe project-relative coordinate.
pub fn binding_file(binding: &str) -> Option<&str> {
    let file = binding.split('#').next()?;
    if !super::rel_path(file) {
        return None;
    }
    Some(file)
}

/// The file and exported symbol of one binding spelling. The symbol is
/// the explicit `#symbol` suffix, or the camelCase spelling of the
/// extension's own symbol.
pub fn binding_parts(binding: &str, fallback_symbol: &str) -> Option<(String, String)> {
    let (file, symbol) = match binding.split_once('#') {
        Some((file, symbol)) => (file, symbol.to_owned()),
        None => (binding, super::canonical::camel(fallback_symbol)),
    };
    if file.is_empty() || symbol.is_empty() {
        return None;
    }
    Some((file.to_owned(), symbol.to_owned()))
}

/// The exported symbol one binding must provide: the explicit `#symbol`
/// suffix, or the camelCase spelling of the extension's own symbol (the
/// renderer's import name).
pub fn binding_symbol(binding: &str, fallback_symbol: &str) -> Option<String> {
    let symbol = match binding.split_once('#') {
        Some((_, symbol)) => symbol.to_owned(),
        None => super::canonical::camel(fallback_symbol),
    };
    if symbol.is_empty() {
        return None;
    }
    Some(symbol)
}

/// The scanned surface of pinned source bytes (r6, devin F2 / codex F7):
/// one bounded span machine that strips comments and records every
/// string/template literal, producing the two surfaces the structural
/// qualifiers consume:
///
/// - `code`: comment-free text whose string *contents* are blanked (the
///   quote characters themselves survive), so neither comments nor
///   inert literals can ever satisfy a token scan;
/// - `top_level`: the `code` surface with every nested `{ … }` region
///   (function/class bodies, blocks) blanked as well, so a declaration
///   inside another scope is invisible to the callable scan;
/// - `literals`: the exact contents of every string/template literal, so
///   a token may qualify as a *standalone quoted key* (`"describe"`)
///   rather than only structurally.
///
/// Unterminated comments or literals fail closed (scan aborted).
pub(crate) struct Surface {
    pub code: String,
    pub top_level: String,
    pub literals: Vec<String>,
    /// Every string/template literal's `[start, end)` span in `code`
    /// coordinates (the blanking is one-to-one, so code indices are text
    /// indices): the conformance probe can ask WHERE a literal sits, not
    /// only what it says (r6, devin F4 / codex F5 — a literal inside a
    /// call argument is not a protocol document row).
    pub literal_spans: Vec<(usize, usize)>,
}

/// Scan pinned bytes into the [`Surface`] surfaces. Shared by the binding
/// callable qualifier and the adapter-entry conformance probe.
///
/// Regex literals are blanked like strings (r6, devin F3): `/function
/// hook(x)/` is pattern text, not a declaration, and a `/` that starts a
/// regex (no preceding value token) never leaks its body onto either
/// surface. A regex left open at EOF fails closed like an open string.
pub(crate) fn scan_surface(bytes: &[u8]) -> Option<Surface> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut code = String::with_capacity(text.len());
    let mut top_level = String::with_capacity(text.len());
    let mut literals = Vec::new();
    let mut literal_spans = Vec::new();
    let mut characters = text.chars().peekable();
    #[derive(PartialEq)]
    enum Span {
        Code,
        Line,
        Block,
        Str(char),
        Regex,
        RegexClass,
    }
    let mut span = Span::Code;
    let mut depth = 0usize;
    let mut literal = String::new();
    let mut unterminated = false;
    // The last significant code character (strings count as their closing
    // quote): decides whether a `/` divides or opens a regex literal.
    let mut last_significant: Option<char> = None;
    while let Some(character) = characters.next() {
        let mut string_char = false; // blank on BOTH surfaces
        let mut nested = false; // blank on the top-level surface only
        match span {
            Span::Code => {
                if character == '/' && characters.peek() == Some(&'/') {
                    characters.next();
                    span = Span::Line;
                    unterminated = true;
                    continue;
                } else if character == '/' && characters.peek() == Some(&'*') {
                    characters.next();
                    span = Span::Block;
                    unterminated = true;
                    continue;
                } else if character == '/' && opens_regex(last_significant) {
                    span = Span::Regex;
                    unterminated = true;
                    string_char = true;
                } else if character == '"' || character == '\'' || character == '`' {
                    span = Span::Str(character);
                    literal.clear();
                    literal_spans.push((code.len(), code.len()));
                    unterminated = true;
                    string_char = true;
                } else {
                    if character == '{' {
                        depth += 1;
                    } else if character == '}' && depth > 0 {
                        depth -= 1;
                    }
                    nested = depth > 0 && character != '\n';
                }
            }
            Span::Line => {
                if character == '\n' {
                    span = Span::Code;
                    unterminated = false;
                } else {
                    continue;
                }
            }
            Span::Block => {
                if character == '*' && characters.peek() == Some(&'/') {
                    characters.next();
                    span = Span::Code;
                    unterminated = false;
                    code.push(' ');
                    top_level.push(' ');
                    continue;
                }
                continue;
            }
            Span::Str(quote) => {
                string_char = true;
                if character == '\\' {
                    if let Some(escaped) = characters.next() {
                        literal.push(escaped);
                    }
                } else if character == quote {
                    span = Span::Code;
                    unterminated = false;
                    literals.push(literal.clone());
                    if let Some((_, end)) = literal_spans.last_mut() {
                        *end = code.len() + character.len_utf8();
                    }
                } else {
                    literal.push(character);
                }
            }
            Span::Regex => {
                // Pattern text is never executable surface.
                string_char = true;
                if character == '\\' {
                    let _ = characters.next();
                } else if character == '[' {
                    span = Span::RegexClass;
                } else if character == ']' {
                    // Unbalanced `]` outside a class: stays pattern text.
                } else if character == '/' {
                    // Closing delimiter: trailing flag letters stay on the
                    // surface as identifier characters.
                    while characters
                        .peek()
                        .is_some_and(|next| next.is_ascii_alphabetic())
                    {
                        let flag = characters.next().expect("flag");
                        code.push(flag);
                        top_level.push(flag);
                        last_significant = Some(flag);
                    }
                    span = Span::Code;
                    unterminated = false;
                } else if character == '\n' {
                    // A regex literal never spans lines: this was not a
                    // regex after all, but the scan cannot rewind — fail
                    // closed instead of trusting the surface.
                    return None;
                }
            }
            Span::RegexClass => {
                string_char = true;
                if character == '\\' {
                    let _ = characters.next();
                } else if character == ']' {
                    span = Span::Regex;
                } else if character == '\n' {
                    return None;
                }
            }
        }
        if !string_char && span == Span::Code && !character.is_whitespace() {
            last_significant = Some(character);
        } else if string_char && span == Span::Code {
            // A completed literal or regex counts as a value token for the
            // next `/` decision.
            last_significant = Some('"');
        }
        code.push(if string_char { ' ' } else { character });
        top_level.push(if string_char || nested {
            ' '
        } else {
            character
        });
    }
    // A comment, literal, or regex left open at EOF fails closed: the
    // surface is not provably the executable source.
    if unterminated {
        return None;
    }
    Some(Surface {
        code,
        top_level,
        literals,
        literal_spans,
    })
}

/// Whether a `/` at this position opens a regex literal rather than a
/// division: a regex follows start-of-file, an opening bracket, an
/// operator, or a keyword separator — never a value token (identifier,
/// literal, `)`, `]`) that a division would follow.
fn opens_regex(last_significant: Option<char>) -> bool {
    match last_significant {
        None => true,
        Some(
            '(' | '[' | '{' | ',' | ';' | ':' | '=' | '!' | '?' | '&' | '|' | '+' | '-' | '*' | '/'
            | '%' | '~' | '^' | '<' | '>' | '\n',
        ) => true,
        Some(_) => false,
    }
}

/// Whether `text[at..]` starts a word at `at`: the preceding character is
/// not an identifier character, so `myfunction` never matches `function`.
fn word_boundary_before(text: &str, at: usize) -> bool {
    at == 0
        || !text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Whether the identifier is followed by a boundary in `tail`.
fn word_boundary_after(tail: &str) -> bool {
    tail.chars()
        .next()
        .map(|c| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(true)
}

/// The substring following the parenthesis that opens at the start of
/// `init`, with string literals and nested parentheses respected; `None`
/// when the parenthesis never closes (fail closed).
fn after_balanced_paren(init: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let bytes = init.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let character = init[index..].chars().next()?;
        if let Some(open) = quote {
            if character == '\\' {
                index += character.len_utf8();
                index += init[index..].chars().next()?.len_utf8();
                continue;
            }
            if character == open {
                quote = None;
            }
        } else {
            match character {
                '"' | '\'' | '`' => quote = Some(character),
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&init[index + 1..]);
                    }
                }
                _ => {}
            }
        }
        index += character.len_utf8();
    }
    None
}

/// Whether one initializer expression is a callable value (finding 3):
/// `function`/`async function` expressions or a parenthesized arrow —
/// `(params) =>` / `(params): Type =>` — never an arbitrary parenthesized
/// expression like `(7)`, and never a class expression: the generated
/// handler invokes bindings with a plain call, so a class-shaped value
/// cannot execute as one (r6, devin F2b / codex F7).
fn callable_initializer(init: &str) -> bool {
    let init = init.trim_start();
    if init.starts_with("function") && word_boundary_after(&init["function".len()..]) {
        return true;
    }
    if let Some(rest) = init.strip_prefix("async") {
        if rest.starts_with(' ') || rest.starts_with('\t') {
            return callable_initializer(rest);
        }
    }
    if !init.starts_with('(') {
        return false;
    }
    let Some(after) = after_balanced_paren(init) else {
        return false;
    };
    let after = after.trim_start();
    if after.starts_with("=>") {
        return true;
    }
    // An optional TypeScript return-type annotation may sit between the
    // parameter list and the arrow: `(a: T): Ret => ...`. The annotation
    // is bounded; a `;` before any `=>` means this is not an arrow.
    if let Some(annotation) = after.strip_prefix(':') {
        let bound = annotation.len().min(200);
        let window = &annotation[..bound];
        match window.find(';') {
            Some(semicolon) => window[..semicolon].contains("=>"),
            None => window.contains("=>"),
        }
    } else {
        false
    }
}

/// Whether the identifier keyword occurrence at `at` sits in statement
/// position on the surface (r6, devin F3): the preceding non-whitespace
/// character must be nothing (start of the surface), `;`, or `}` — or one
/// of the reserved modifier words `async`/`export`/`default`, themselves
/// recursively in statement position. A `function` inside a parenthesized
/// expression, an argument list, an array, or after an assignment never
/// declares a module-scope binding: its name binds inside the expression
/// only, so the emitted test's named import cannot resolve.
fn statement_position(text: &str, at: usize) -> bool {
    let mut cursor = at;
    loop {
        // Skip whitespace backward.
        let mut start = cursor;
        while start > 0
            && text[..start]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace())
        {
            start -= text[..start].chars().next_back().expect("char").len_utf8();
        }
        if start == 0 {
            return true;
        }
        let previous = text[..start].chars().next_back().expect("char");
        if previous == ';' || previous == '}' {
            return true;
        }
        if !previous.is_ascii_alphabetic() {
            return false;
        }
        // A reserved modifier word may stand between the statement start
        // and the keyword; anything else (an identifier, `=`, `(`, `,`,
        // `return`, …) is expression context.
        let word_start = text[..start]
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_ascii_alphanumeric() && *c != '_')
            .map(|(index, _)| index + text[index..].chars().next().expect("char").len_utf8())
            .unwrap_or(0);
        match &text[word_start..start] {
            "async" | "export" | "default" => cursor = word_start,
            _ => return false,
        }
    }
}

/// Whether the executable source declares `name` as a callable binding in
/// a **top-level executable position** (finding 3; r6 devin F2 / codex
/// F7): a `function`/`async function` declaration of that exact name in
/// *statement position* (r6, devin F3 — a named function expression
/// inside parentheses, an argument list, or an assignment never declares
/// a module-scope binding, and regex-literal bodies are blanked by
/// [`scan_surface`]), or a `const/let/var name = <callable initializer>`
/// whose initializer is a function/arrow expression — a plain value
/// initializer (`const hook = 7`, `const hook = (7)`) never qualifies, a
/// class declaration never qualifies (the emitted test invokes plain
/// calls), a declaration nested inside another scope's braces never
/// qualifies, and text inside a string literal never qualifies: the scan
/// runs over the top-level surface of [`scan_surface`], where nested
/// bodies and literal contents are blank.
fn declares_callable(text: &str, name: &str) -> bool {
    for keyword in ["function"] {
        let mut from = 0usize;
        while let Some(found) = text[from..].find(keyword) {
            let at = from + found;
            from = at + keyword.len();
            if !word_boundary_before(text, at) || !word_boundary_after(&text[at + keyword.len()..])
            {
                continue;
            }
            // Statement position only: expression contexts (a named
            // function expression) never declare the name in module scope.
            if !statement_position(text, at) {
                continue;
            }
            // Generators spell `function* name`.
            let mut tail = text[at + keyword.len()..].trim_start();
            if let Some(rest) = tail.strip_prefix('*') {
                tail = rest.trim_start();
            }
            if tail.starts_with(name) && word_boundary_after(&tail[name.len()..]) {
                return true;
            }
        }
    }
    for keyword in ["const", "let", "var"] {
        let mut from = 0usize;
        while let Some(found) = text[from..].find(keyword) {
            let at = from + found;
            from = at + keyword.len();
            if !word_boundary_before(text, at) {
                continue;
            }
            if !statement_position(text, at) || !word_boundary_after(&text[at + keyword.len()..]) {
                continue;
            }
            let tail = text[at + keyword.len()..].trim_start();
            let Some(after_name) = tail.strip_prefix(name) else {
                continue;
            };
            if !word_boundary_after(after_name) {
                continue;
            }
            let after_name = after_name.trim_start();
            let Some(initializer) = after_name.strip_prefix('=') else {
                continue;
            };
            if callable_initializer(initializer) {
                return true;
            }
        }
    }
    false
}

/// Whether the executable source exports the symbol as a callable top-
/// level binding (findings 6 and r4-3): `export function`/`export async
/// function` declarations, `export const/let/var <name> = <callable
/// expression>` initializers, and `export { … }` clauses whose local name
/// resolves back to one of those callable declarations in the same file.
/// The whole qualification runs over the scanned surfaces of
/// [`scan_surface`] (r6, devin F2 / codex F7): string-literal decoys are
/// blank (a `"function hook(){}"` inside a literal is invisible), nested
/// declarations are blank (a `function hook` inside another function's
/// body does not satisfy an `export { hook }` clause), and class-shaped
/// bindings never qualify — the emitted test invokes plain calls, so a
/// constructor-only value would fail natively. A named export of a
/// non-callable value (`const hook = 7; export { hook }`), an arbitrary
/// parenthesized initializer (`export const hook = (7)`), and a re-export
/// from another module (unverifiable here) never qualify. Text inside
/// comments is invisible.
pub(crate) fn exports_symbol(bytes: &[u8], symbol: &str) -> bool {
    let Some(surface) = scan_surface(bytes) else {
        return false;
    };
    // Export keywords are recognized at top-level statement boundaries,
    // not line starts: `stmt(); export { hook }` and multiline clauses are
    // valid module surfaces too. Nested/literal export text stays blank.
    for (at, _) in surface.top_level.match_indices("export") {
        if !word_boundary_before(&surface.top_level, at)
            || !word_boundary_after(&surface.top_level[at + "export".len()..])
            || !statement_position(&surface.top_level, at)
        {
            continue;
        }
        let rest = surface.code[at + "export".len()..].trim_start();
        if let Some(list) = rest.strip_prefix('{') {
            let Some(close) = list.find('}') else {
                continue;
            };
            // Re-exports do not resolve to a declaration in these bytes.
            if list[close + 1..].trim_start().starts_with("from") {
                continue;
            }
            for clause in list[..close].split(',') {
                let words: Vec<_> = clause.split_whitespace().collect();
                let (local, exported) = match words.as_slice() {
                    [local] => (*local, *local),
                    [local, "as", exported] => (*local, *exported),
                    _ => continue,
                };
                if exported == symbol && declares_callable(&surface.top_level, local) {
                    return true;
                }
            }
        } else {
            let top = surface.top_level[at + "export".len()..].trim_start();
            // A default export is not a named import, and a later callable
            // declaration must not accidentally qualify an earlier export.
            if top.starts_with("default") {
                continue;
            }
            let end = top.find([';', '}']).unwrap_or(top.len());
            if declares_callable(&top[..end], symbol) {
                return true;
            }
        }
    }
    false
}

/// Resolve every validated extension of one coverage document against the
/// live project: the pinned binding file must exist with the exact pinned
/// digest, and its exact bytes must export the declared symbol with a
/// callable declaration (finding 6). Selected-but-unvalidated rows are
/// already refused by the semantic admission; this check runs after it.
pub fn resolve(
    root: &std::path::Path,
    _target: &TargetAssignment,
    coverage: &Coverage,
) -> Result<(), DomainResult> {
    for check in &coverage.extensions {
        if !check.validated {
            continue;
        }
        let Some(file) = binding_file(&check.binding) else {
            return Err(unresolved(&check.symbol, "binding"));
        };
        let Some(symbol) = binding_symbol(&check.binding, &check.symbol) else {
            return Err(unresolved(&check.symbol, "symbol"));
        };
        let bytes = match fs::read_file(root, file) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return Err(unresolved(&check.symbol, "missing")),
            Err(_) => return Err(unresolved(&check.symbol, "unresolved")),
        };
        if super::digest(&bytes) != check.contract_digest {
            return Err(unresolved(&check.symbol, "stale"));
        }
        if !exports_symbol(&bytes, &symbol) {
            return Err(unresolved(&check.symbol, "export"));
        }
    }
    Ok(())
}
