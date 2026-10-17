//! Canonical byte helpers for the search contracts (issue #93).
//!
//! - Duplicate JSON keys are rejected before any decode, matching the
//!   repository's closed-document convention.
//! - Canonical JSON is compact UTF-8 with byte-sorted object keys and no
//!   trailing LF; every digest over search bytes uses exactly those
//!   bytes with SHA-256.

use serde_json::Value as Json;

/// Reject duplicate keys anywhere in one JSON text. Object key order is
/// preserved; this is a closed-document safety check, not a
/// canonicalizer. The scan is single-pass and linear in document
/// bytes (issue #93 r6): keys decode into per-object keyed sets for
/// duplicate comparison, values scan without materialization.
pub(crate) fn reject_duplicate_keys(text: &str) -> Result<(), ()> {
    let mut lexer = KeyLexer::new(text);
    lexer.scan_value(0)?;
    lexer.skip_ws();
    if lexer.peek().is_some() {
        return Err(());
    }
    Ok(())
}

// Well above the closed search wires' nesting, and below serde_json's
// default recursion limit. Bound containers before recursing so even a
// sub-MiB hostile input cannot overflow the CLI stack (issue #93 r11).
const MAX_JSON_DEPTH: usize = 64;

struct KeyLexer<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> KeyLexer<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            bytes: text.as_bytes(),
            position: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.position += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ()> {
        if self.peek() == Some(byte) {
            self.position += 1;
            Ok(())
        } else {
            Err(())
        }
    }

    fn scan_value(&mut self, depth: usize) -> Result<(), ()> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') if depth < MAX_JSON_DEPTH => self.scan_object(depth + 1),
            Some(b'[') if depth < MAX_JSON_DEPTH => self.scan_array(depth + 1),
            Some(b'{' | b'[') => Err(()),
            Some(b'"') => self.scan_string(None),
            Some(b't') => self.scan_literal("true"),
            Some(b'f') => self.scan_literal("false"),
            Some(b'n') => self.scan_literal("null"),
            Some(_) => self.scan_number(),
            None => Err(()),
        }
    }

    fn scan_literal(&mut self, literal: &str) -> Result<(), ()> {
        if self.bytes[self.position..].starts_with(literal.as_bytes()) {
            self.position += literal.len();
            Ok(())
        } else {
            Err(())
        }
    }

    fn scan_number(&mut self) -> Result<(), ()> {
        let start = self.position;
        while matches!(self.peek(), Some(byte) if byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.position += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position]).map_err(|_| ())?;
        if text.is_empty() {
            return Err(());
        }
        if text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok() {
            Ok(())
        } else {
            Err(())
        }
    }

    /// Scan one JSON string. `sink` receives the decoded scalars only
    /// when duplicate-key comparison needs them (object keys); value
    /// strings scan without materialization. Every scalar costs O(1):
    /// its width comes from the lead byte alone — the input is `&str`
    /// (valid UTF-8 by invariant) and the position only ever rests on
    /// scalar boundaries — never a re-validation of the whole
    /// remaining tail, which was quadratic in document bytes
    /// (issue #93 r6).
    fn scan_string(&mut self, mut sink: Option<&mut String>) -> Result<(), ()> {
        self.expect(b'"')?;
        loop {
            match self.peek() {
                None => return Err(()),
                Some(b'"') => {
                    self.position += 1;
                    return Ok(());
                }
                Some(b'\\') => {
                    self.position += 1;
                    let escape = self.peek().ok_or(())?;
                    self.position += 1;
                    let decoded: char = match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{0008}',
                        b'f' => '\u{000C}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            // A truncated \uXXXX escape (fewer than four
                            // hex bytes left) is a malformed document,
                            // never a panic: slice with get and refuse.
                            let hex = self
                                .bytes
                                .get(self.position..self.position + 4)
                                .and_then(|window| std::str::from_utf8(window).ok())
                                .ok_or(())?;
                            let code = u32::from_str_radix(hex, 16).map_err(|_| ())?;
                            self.position += 4;
                            // Validation runs even without a sink: a
                            // lone surrogate never decodes anywhere.
                            char::from_u32(code).ok_or(())?
                        }
                        _ => return Err(()),
                    };
                    if let Some(sink) = sink.as_deref_mut() {
                        sink.push(decoded);
                    }
                }
                Some(byte) => {
                    // Consume one UTF-8 scalar (multi-byte safe); a
                    // lead byte that cannot begin a scalar, or a
                    // truncated scalar at end of input, is malformed,
                    // never a panic.
                    let width = scalar_width(byte).ok_or(())?;
                    let end = self.position + width;
                    let chunk = self.bytes.get(self.position..end).ok_or(())?;
                    if let Some(sink) = sink.as_deref_mut() {
                        // O(1): only this scalar's own bytes decode.
                        let decoded = std::str::from_utf8(chunk).map_err(|_| ())?;
                        sink.push_str(decoded);
                    }
                    self.position = end;
                }
            }
        }
    }

    fn scan_array(&mut self, depth: usize) -> Result<(), ()> {
        self.expect(b'[')?;
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Ok(());
        }
        loop {
            self.scan_value(depth)?;
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    return Ok(());
                }
                _ => return Err(()),
            }
        }
    }

    /// Object keys decode into one per-object keyed set — single-pass,
    /// linear-time duplicate detection (issue #93 r6). Values scan
    /// without materialization: the parsed document is never kept.
    fn scan_object(&mut self, depth: usize) -> Result<(), ()> {
        self.expect(b'{')?;
        let mut keys: std::collections::HashSet<String> = std::collections::HashSet::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.position += 1;
            return Ok(());
        }
        loop {
            self.skip_ws();
            let mut key = String::new();
            self.scan_string(Some(&mut key))?;
            self.skip_ws();
            self.expect(b':')?;
            self.scan_value(depth)?;
            if !keys.insert(key) {
                // A duplicate key never decodes.
                return Err(());
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    return Ok(());
                }
                _ => return Err(()),
            }
        }
    }
}

/// The UTF-8 scalar width of one lead byte. Continuation bytes and the
/// lead bytes valid UTF-8 never produces (0xC0/0xC1, 0xF5..=0xFF)
/// return `None`: they cannot begin a scalar inside a valid `&str`.
fn scalar_width(byte: u8) -> Option<usize> {
    match byte {
        0x00..=0x7F => Some(1),
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

/// Canonical JSON: compact UTF-8 with byte-sorted object keys.
pub(crate) fn canonical_json(value: &Json) -> String {
    match value {
        Json::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
            let mut out = String::from("{");
            for (index, key) in keys.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("key serializes"));
                out.push(':');
                out.push_str(&canonical_json(&map[*key]));
            }
            out.push('}');
            out
        }
        Json::Array(items) => {
            let mut out = String::from("[");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&canonical_json(item));
            }
            out.push(']');
            out
        }
        other => serde_json::to_string(other).expect("scalar serializes"),
    }
}

/// SHA-256 over the exact bytes spelled as bare lowercase hex; callers
/// add the `sha256:` prefix where the wire grammar requires it.
pub(crate) fn sha256_digest(bytes: &[u8]) -> String {
    crate::digest::sha256_hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_keys_are_rejected_everywhere() {
        assert!(reject_duplicate_keys(r#"{"a":1,"b":{"c":2,"c":3}}"#).is_err());
        assert!(reject_duplicate_keys(r#"{"a":1,"a":2}"#).is_err());
        assert!(reject_duplicate_keys(r#"{"a":1,"b":2}"#).is_ok());
        assert!(reject_duplicate_keys(r#"[{"a":1},{"a":2}]"#).is_ok());
        assert!(reject_duplicate_keys("[1,2,]").is_err());
    }

    #[test]
    fn adversarial_bytes_are_typed_refusals_never_panics() {
        // A truncated \u escape: the historic panic class.
        assert!(reject_duplicate_keys("{\"x\":\"\\u").is_err());
        assert!(reject_duplicate_keys("{\"x\":\"\\u00").is_err());
        assert!(reject_duplicate_keys("{\"x\":\"\\uzzzz\"}").is_err());
        // Lone surrogates are not scalar values, and unpaired escapes
        // never decode (fail-closed, matching the closed lexer).
        assert!(reject_duplicate_keys("{\"x\":\"\\ud800\"}").is_err());
        assert!(reject_duplicate_keys("{\"x\":\"\\uDC00\\uD800\"}").is_err());
        assert!(reject_duplicate_keys("{\"x\":\"\\uD83D\\uDE00\"}").is_err());
        // A truncated multi-byte scalar, a bare value, and truncation
        // anywhere all refuse.
        assert!(reject_duplicate_keys("{\"x\":\"\u{e4}\"").is_err());
        assert!(reject_duplicate_keys("trunc").is_err());
        assert!(reject_duplicate_keys("{").is_err());
        assert!(reject_duplicate_keys("-").is_err());
        assert!(reject_duplicate_keys("1e").is_err());
        assert!(reject_duplicate_keys("\"").is_err());
        assert!(reject_duplicate_keys("\"\\").is_err());
    }

    #[test]
    fn nesting_is_bounded_before_recursing() {
        for (open, close) in [("[", "]"), ("{\"x\":", "}")] {
            let shallow = format!("{}0{}", open.repeat(8), close.repeat(8));
            assert!(reject_duplicate_keys(&shallow).is_ok());
            let at_limit = format!(
                "{}0{}",
                open.repeat(MAX_JSON_DEPTH),
                close.repeat(MAX_JSON_DEPTH)
            );
            assert!(reject_duplicate_keys(&at_limit).is_ok());
            let over_limit = format!(
                "{}0{}",
                open.repeat(MAX_JSON_DEPTH + 1),
                close.repeat(MAX_JSON_DEPTH + 1)
            );
            assert!(reject_duplicate_keys(&over_limit).is_err());
            let hostile = format!("{}0{}", open.repeat(400_000), close.repeat(400_000));
            assert!(reject_duplicate_keys(&hostile).is_err());
        }
    }

    #[test]
    fn canonical_json_sorts_object_keys_only() {
        let value: Json = serde_json::from_str(r#"{"b":1,"a":{"d":[2,1],"c":3}}"#).expect("parses");
        assert_eq!(canonical_json(&value), r#"{"a":{"c":3,"d":[2,1]},"b":1}"#);
    }

    /// One broad document of unique keys and multi-byte value strings
    /// (issue #93 r6): the pre-fix lexer re-validated the whole
    /// remaining tail per scalar — O(n\u{00b2}) in document bytes, tens
    /// of seconds for a ~1.5 MiB generation and hours at the 32 MiB
    /// cap — so every store load, producer parse, and freshness check
    /// paid quadratic cost. The scan must stay single-pass linear:
    /// this deadline is generous for the linear mechanism (~100x
    /// margin even on a loaded CI runner) while the quadratic one
    /// could not finish it at all.
    #[test]
    fn duplicate_detection_is_linear_at_scale() {
        // ~4 MiB: 40k objects, each with unique keys and multi-byte
        // value strings, so both the key set and the scalar scanner
        // see real UTF-8 widths.
        let mut document = String::from("[");
        for index in 0..40_000u32 {
            document.push_str(&format!(
                "{{\"kind\":\"scale-\u{00e9}\u{e4}o\",\"index\":{index},\"note-\u{fc}nique-{index}\":\"\u{4e00}\u{6587}\u{30c6}\u{30ad}\u{30b9}\u{c3a9}val\u{eb}\u{14d}\u{65e5}\u{672c}\u{8a9e} value \u{e4}\u{f6}\u{fc}\u{df} {index}\",\"flags\":[true,false,null,-12.5e3]}}"
            ));
            if index + 1 < 40_000 {
                document.push(',');
            }
        }
        document.push(']');
        assert!(document.len() > 3 * 1024 * 1024, "fixture stays at scale");
        let started = std::time::Instant::now();
        assert!(reject_duplicate_keys(&document).is_ok());
        let accept = started.elapsed();
        // The same scan must also REFUSE a duplicate planted at the very
        // end — the adversarial slow-reject case (a corrupt-tail
        // generation previously took ~24 s just to be branded corrupt).
        let mut hostile = document.clone();
        hostile.truncate(hostile.len().saturating_sub(1));
        hostile.push_str(",{\"dup\":1,\"dup\":2}]");
        let started = std::time::Instant::now();
        assert!(reject_duplicate_keys(&hostile).is_err());
        let reject = started.elapsed();
        // Honest bound, not a gate: 4 MiB linear scans finish in tens of
        // milliseconds in debug builds; ten seconds cannot pass for a
        // linear lexer but is far inside reach for a quadratic one.
        assert!(
            accept.as_secs() < 10 && reject.as_secs() < 10,
            "duplicate-key scan must stay linear: accept {accept:?}, reject {reject:?}"
        );
    }
}
