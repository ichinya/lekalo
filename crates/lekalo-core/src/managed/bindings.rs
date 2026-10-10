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

/// The comment-free surface of the pinned bytes: a bounded scanner that
/// removes `//` line comments and `/* */` block comments while respecting
/// string and template literals, so a commented-out export never qualifies
/// (finding 6). Unterminated constructs fail closed (scan aborted).
pub(crate) fn comment_free(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    #[derive(PartialEq)]
    enum Span {
        Code,
        Line,
        Block,
        Str(char),
    }
    let mut span = Span::Code;
    while let Some(character) = characters.next() {
        match span {
            Span::Code => {
                if character == '/' && characters.peek() == Some(&'/') {
                    characters.next();
                    span = Span::Line;
                } else if character == '/' && characters.peek() == Some(&'*') {
                    characters.next();
                    span = Span::Block;
                } else {
                    if character == '"' || character == '\'' || character == '`' {
                        span = Span::Str(character);
                    }
                    out.push(character);
                }
            }
            Span::Line => {
                if character == '\n' {
                    span = Span::Code;
                    out.push(character);
                }
            }
            Span::Block => {
                if character == '*' && characters.peek() == Some(&'/') {
                    characters.next();
                    span = Span::Code;
                    out.push(' ');
                }
            }
            Span::Str(quote) => {
                out.push(character);
                if character == '\\' {
                    if let Some(escaped) = characters.next() {
                        out.push(escaped);
                    }
                } else if character == quote {
                    span = Span::Code;
                }
            }
        }
    }
    // A comment or literal left open at EOF fails closed: the surface is
    // not provably the executable source.
    if span == Span::Line || span == Span::Block {
        return None;
    }
    Some(out)
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
/// `function`/`async function`/`class` expressions, or a parenthesized
/// arrow — `(params) =>` / `(params): Type =>` — never an arbitrary
/// parenthesized expression like `(7)`.
fn callable_initializer(init: &str) -> bool {
    let init = init.trim_start();
    if init.starts_with("function") || init.starts_with("class") {
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

/// Whether the executable source declares `name` as a callable binding
/// anywhere (finding 3): a `function`/`async function`/`class` declaration
/// of that exact name, or a `const/let/var name = <callable initializer>`
/// whose initializer is a function/arrow/class expression — a plain value
/// initializer (`const hook = 7`, `const hook = (7)`) never qualifies.
fn declares_callable(text: &str, name: &str) -> bool {
    for keyword in ["function", "class"] {
        let mut from = 0usize;
        while let Some(found) = text[from..].find(keyword) {
            let at = from + found;
            from = at + keyword.len();
            if !word_boundary_before(text, at) {
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
/// function`/`export class` declarations, `export const/let/var <name> =
/// <callable expression>` initializers, and `export { … }` clauses whose
/// local name resolves back to one of those callable declarations in the
/// same file. A named export of a non-callable value (`const hook = 7;
/// export { hook }`), an arbitrary parenthesized initializer (`export
/// const hook = (7)`), and a re-export from another module (unverifiable
/// here) never qualify. Text inside comments is invisible.
pub(crate) fn exports_symbol(bytes: &[u8], symbol: &str) -> bool {
    let Some(text) = comment_free(bytes) else {
        return false;
    };
    // Direct exported declaration forms.
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("export ") else {
            continue;
        };
        if rest.starts_with('{') {
            continue;
        }
        if declares_callable(rest, symbol) {
            return true;
        }
    }
    // Named-export clauses: `export { local as symbol }` resolves the
    // local name back to its declaration in the same file and requires a
    // callable form (r4 finding 3). A `from "…"` re-export names a
    // binding this scan cannot verify and never qualifies.
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("export ") else {
            continue;
        };
        let Some(list) = rest.strip_prefix('{') else {
            continue;
        };
        let Some(close) = list.find('}') else {
            continue;
        };
        if list[close + 1..].trim_start().starts_with("from") {
            continue;
        }
        for clause in list[..close].split(',') {
            let clause = clause.trim();
            if clause.is_empty() {
                continue;
            }
            let (local, exported) = match clause.split_once(" as ") {
                Some((local, exported)) => (local.trim(), exported.trim()),
                None => (clause, clause),
            };
            if exported == symbol && declares_callable(&text, local) {
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
