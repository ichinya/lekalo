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
fn comment_free(bytes: &[u8]) -> Option<String> {
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

/// Whether the executable source declares the symbol as a callable top-
/// level export (finding 6): only `export function`/`export async
/// function` declarations, `export class` declarations, and
/// `export const/let/var <name> = (arrow | function | class)` initializers
/// qualify — a plain `export const <name> = 7` is a named export without a
/// callable value and is refused, and text inside comments is invisible.
fn exports_symbol(bytes: &[u8], symbol: &str) -> bool {
    let Some(text) = comment_free(bytes) else {
        return false;
    };
    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("export ") else {
            continue;
        };
        if rest.starts_with('{') {
            // `export { a, b as c }`: the symbol must appear as a name.
            if rest
                .trim_start_matches('{')
                .trim_end_matches('}')
                .split(',')
                .any(|clause| {
                    let clause = clause.trim();
                    clause == symbol
                        || clause
                            .split_once(" as ")
                            .map(|(_, exported)| exported.trim() == symbol)
                            .unwrap_or(false)
                })
            {
                return true;
            }
            continue;
        }
        // `export function <name>`, `export async function <name>`,
        // `export class <name>`: callable declarations.
        let declares_callable_kind = |keyword: &str| {
            rest.trim_start()
                .strip_prefix(keyword)
                .map(|tail| {
                    tail.trim_start()
                        .strip_prefix(symbol)
                        .map(|after| {
                            after
                                .chars()
                                .next()
                                .map(|c| !c.is_ascii_alphanumeric() && c != '_')
                                .unwrap_or(true)
                        })
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        };
        if declares_callable_kind("function ")
            || declares_callable_kind("async function ")
            || declares_callable_kind("class ")
        {
            return true;
        }
        // `export const/let/var <name> = <callable expression>`: the
        // initializer must be a function or arrow expression, not an
        // arbitrary value (finding 6).
        for keyword in ["const ", "let ", "var "] {
            let Some(tail) = rest.trim_start().strip_prefix(keyword) else {
                continue;
            };
            let Some(equals) = tail.find('=') else {
                continue;
            };
            let declared = tail[..equals].trim();
            if declared != symbol {
                continue;
            }
            let initializer = tail[equals + 1..].trim_start();
            let callable = initializer.starts_with("function")
                || initializer.starts_with("async function")
                || initializer.starts_with("class")
                || initializer.starts_with("(")
                || initializer.starts_with("async (");
            if callable {
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
