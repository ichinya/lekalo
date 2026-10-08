//! Canonical byte helpers for the search contracts (issue #93).
//!
//! - Duplicate JSON keys are rejected before any decode, matching the
//!   repository's closed-document convention.
//! - Canonical JSON is compact UTF-8 with byte-sorted object keys and no
//!   trailing LF; every digest over search bytes uses exactly those
//!   bytes with SHA-256.

use serde_json::Value as Json;

/// Reject duplicate keys anywhere in one JSON text. Object key order is
/// preserved; this is a closed-document safety check, not a canonicalizer.
pub(crate) fn reject_duplicate_keys(text: &str) -> Result<(), ()> {
    let mut lexer = KeyLexer::new(text);
    let value = lexer.parse_value()?;
    lexer.skip_ws();
    if lexer.peek().is_some() {
        return Err(());
    }
    let _ = value;
    Ok(())
}

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

    fn parse_value(&mut self) -> Result<Json, ()> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(Json::String),
            Some(b't') => self.parse_literal("true", Json::Bool(true)),
            Some(b'f') => self.parse_literal("false", Json::Bool(false)),
            Some(b'n') => self.parse_literal("null", Json::Null),
            Some(_) => self.parse_number(),
            None => Err(()),
        }
    }

    fn parse_literal(&mut self, literal: &str, value: Json) -> Result<Json, ()> {
        if self.bytes[self.position..].starts_with(literal.as_bytes()) {
            self.position += literal.len();
            Ok(value)
        } else {
            Err(())
        }
    }

    fn parse_number(&mut self) -> Result<Json, ()> {
        let start = self.position;
        while matches!(self.peek(), Some(byte) if byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.' | b'e' | b'E'))
        {
            self.position += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|_| ())?
            .to_owned();
        if text.is_empty() {
            return Err(());
        }
        text.parse::<i64>()
            .map(|value| Json::Number(value.into()))
            .or_else(|_| text.parse::<f64>().map(Json::from).map_err(|_| ()))
    }

    fn parse_string(&mut self) -> Result<String, ()> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(()),
                Some(b'"') => {
                    self.position += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.position += 1;
                    let escape = self.peek().ok_or(())?;
                    self.position += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
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
                            let character = char::from_u32(code).ok_or(())?;
                            out.push(character);
                        }
                        _ => return Err(()),
                    }
                }
                Some(_) => {
                    // Consume one UTF-8 scalar (multi-byte safe); a
                    // truncated scalar is malformed, never a panic.
                    let rest = std::str::from_utf8(&self.bytes[self.position..]).map_err(|_| ())?;
                    let character = rest.chars().next().ok_or(())?;
                    out.push(character);
                    self.position += character.len_utf8();
                }
            }
        }
    }

    fn parse_array(&mut self) -> Result<Json, ()> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.position += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    return Ok(Json::Array(items));
                }
                _ => return Err(()),
            }
        }
    }

    fn parse_object(&mut self) -> Result<Json, ()> {
        self.expect(b'{')?;
        let mut map = serde_json::Map::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.position += 1;
            return Ok(Json::Object(map));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            if map.insert(key.clone(), value).is_some() {
                // A duplicate key never decodes.
                return Err(());
            }
            self.skip_ws();
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    return Ok(Json::Object(map));
                }
                _ => return Err(()),
            }
        }
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
    fn canonical_json_sorts_object_keys_only() {
        let value: Json = serde_json::from_str(r#"{"b":1,"a":{"d":[2,1],"c":3}}"#).expect("parses");
        assert_eq!(canonical_json(&value), r#"{"a":{"c":3,"d":[2,1]},"b":1}"#);
    }
}
