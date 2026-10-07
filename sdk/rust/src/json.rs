//! Strict JSON admission (issue #106): closed parsing with duplicate-key,
//! trailing-document, explicit-null, and non-finite refusals, plus the
//! canonical byte serializer the wire requires (sorted keys, compact, no
//! trailing LF). Mirrors the TypeScript reference runtime exactly.

use std::collections::BTreeSet;
use std::fmt;

/// The closed safe reason tokens a strict parse refuses with.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseReason {
    NotJson,
    TrailingDocument,
    DuplicateKey,
    Oversize,
}

impl fmt::Display for ParseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let token = match self {
            Self::NotJson => "not-json",
            Self::TrailingDocument => "trailing-document",
            Self::DuplicateKey => "duplicate-key",
            Self::Oversize => "oversize",
        };
        f.write_str(token)
    }
}

/// A parsed JSON value preserving member order (for duplicate detection)
/// and integer/float distinction (for canonical bytes).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

impl Value {
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Self::Object(members) => members.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn member_names(&self) -> Vec<&str> {
        match self {
            Self::Object(members) => members.iter().map(|(k, _)| k.as_str()).collect(),
            _ => Vec::new(),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Int(value) if *value >= 0 => Some(*value as u64),
            Self::Float(value) if value.fract() == 0.0 && *value >= 0.0 => Some(*value as u64),
            _ => None,
        }
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Self::Object(_))
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    index: usize,
}

/// Strict parse of one complete JSON document.
pub fn strict_parse(text: &str) -> Result<Value, ParseReason> {
    let bytes = text.as_bytes();
    if bytes.first() == Some(&0xEF) && bytes.len() >= 3 && bytes[1] == 0xBB && bytes[2] == 0xBF {
        return Err(ParseReason::NotJson);
    }
    let mut parser = Parser { bytes, index: 0 };
    parser.skip_whitespace();
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.index != bytes.len() {
        return Err(ParseReason::TrailingDocument);
    }
    Ok(value)
}

/// Read exactly one bounded JSON object document.
pub fn read_single_object(text: &str, max_bytes: usize) -> Result<Value, ParseReason> {
    if text.len() > max_bytes {
        return Err(ParseReason::Oversize);
    }
    let value = strict_parse(text)?;
    if !value.is_object() {
        return Err(ParseReason::NotJson);
    }
    Ok(value)
}

impl<'a> Parser<'a> {
    fn skip_whitespace(&mut self) {
        while self.index < self.bytes.len()
            && matches!(self.bytes[self.index], b' ' | b'\t' | b'\r' | b'\n')
        {
            self.index += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn parse_value(&mut self) -> Result<Value, ParseReason> {
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => Ok(Value::Str(self.parse_string()?)),
            Some(b't') => self.parse_literal("true", Value::Bool(true)),
            Some(b'f') => self.parse_literal("false", Value::Bool(false)),
            Some(b'n') => self.parse_literal("null", Value::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.parse_number(),
            _ => Err(ParseReason::NotJson),
        }
    }

    fn parse_literal(&mut self, literal: &str, value: Value) -> Result<Value, ParseReason> {
        if self.bytes[self.index..].starts_with(literal.as_bytes()) {
            self.index += literal.len();
            Ok(value)
        } else {
            Err(ParseReason::NotJson)
        }
    }

    fn parse_object(&mut self) -> Result<Value, ParseReason> {
        self.index += 1;
        let mut members: Vec<(String, Value)> = Vec::new();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.index += 1;
            return Ok(Value::Object(members));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(ParseReason::NotJson);
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(ParseReason::NotJson);
            }
            self.index += 1;
            self.skip_whitespace();
            let value = self.parse_value()?;
            if !seen.insert(key.clone()) {
                return Err(ParseReason::DuplicateKey);
            }
            members.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.index += 1,
                Some(b'}') => {
                    self.index += 1;
                    return Ok(Value::Object(members));
                }
                _ => return Err(ParseReason::NotJson),
            }
        }
    }

    fn parse_array(&mut self) -> Result<Value, ParseReason> {
        self.index += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.index += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.parse_value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.index += 1,
                Some(b']') => {
                    self.index += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(ParseReason::NotJson),
            }
        }
    }

    fn parse_string(&mut self) -> Result<String, ParseReason> {
        self.index += 1;
        let mut out = String::new();
        loop {
            let byte = self.peek().ok_or(ParseReason::NotJson)?;
            self.index += 1;
            match byte {
                b'"' => return Ok(out),
                b'\\' => {
                    let escape = self.peek().ok_or(ParseReason::NotJson)?;
                    self.index += 1;
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
                            let high = self.parse_hex4()?;
                            if (0xD800..0xDC00).contains(&high) {
                                if self.peek() == Some(b'\\') {
                                    self.index += 1;
                                    if self.peek() != Some(b'u') {
                                        return Err(ParseReason::NotJson);
                                    }
                                    self.index += 1;
                                    let low = self.parse_hex4()?;
                                    let combined = 0x10000
                                        + (((high - 0xD800) as u32) << 10)
                                        + (low - 0xDC00) as u32;
                                    out.push(char::from_u32(combined).ok_or(ParseReason::NotJson)?);
                                } else {
                                    return Err(ParseReason::NotJson);
                                }
                            } else {
                                out.push(char::from_u32(high as u32).ok_or(ParseReason::NotJson)?);
                            }
                        }
                        _ => return Err(ParseReason::NotJson),
                    }
                }
                0x00..=0x1F => return Err(ParseReason::NotJson),
                _ => {
                    // Consume the full UTF-8 sequence starting at this byte.
                    let start = self.index - 1;
                    let width = utf8_width(byte).ok_or(ParseReason::NotJson)?;
                    let end = start + width;
                    if end > self.bytes.len() {
                        return Err(ParseReason::NotJson);
                    }
                    let chunk = std::str::from_utf8(&self.bytes[start..end])
                        .map_err(|_| ParseReason::NotJson)?;
                    out.push_str(chunk);
                    self.index = end;
                }
            }
        }
    }

    fn parse_hex4(&mut self) -> Result<u16, ParseReason> {
        if self.index + 4 > self.bytes.len() {
            return Err(ParseReason::NotJson);
        }
        let chunk = std::str::from_utf8(&self.bytes[self.index..self.index + 4])
            .map_err(|_| ParseReason::NotJson)?;
        let value = u16::from_str_radix(chunk, 16).map_err(|_| ParseReason::NotJson)?;
        self.index += 4;
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<Value, ParseReason> {
        let start = self.index;
        if self.peek() == Some(b'-') {
            self.index += 1;
        }
        while self.peek().is_some_and(|b| b.is_ascii_digit()) {
            self.index += 1;
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.index += 1;
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.index += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.index += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.index += 1;
            }
            while self.peek().is_some_and(|b| b.is_ascii_digit()) {
                self.index += 1;
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.index])
            .map_err(|_| ParseReason::NotJson)?;
        if text.is_empty() || text == "-" {
            return Err(ParseReason::NotJson);
        }
        if is_float {
            let value: f64 = text.parse().map_err(|_| ParseReason::NotJson)?;
            Ok(Value::Float(value))
        } else {
            match text.parse::<i64>() {
                Ok(value) => Ok(Value::Int(value)),
                Err(_) => {
                    let value: f64 = text.parse().map_err(|_| ParseReason::NotJson)?;
                    Ok(Value::Float(value))
                }
            }
        }
    }
}

fn utf8_width(byte: u8) -> Option<usize> {
    match byte {
        0x00..=0x7F => Some(1),
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

/// Canonical compact JSON: byte-sorted keys, no whitespace, no trailing LF.
pub fn canonical(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(true) => "true".to_owned(),
        Value::Bool(false) => "false".to_owned(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => {
            if value.fract() == 0.0 && value.is_finite() && value.abs() < 9.007_199_254_740_992e15 {
                format!("{}", *value as i64)
            } else {
                format!("{value}")
            }
        }
        Value::Str(text) => quote(text),
        Value::Array(items) => {
            let body: Vec<String> = items.iter().map(canonical).collect();
            format!("[{}]", body.join(","))
        }
        Value::Object(members) => {
            let mut sorted: Vec<(String, String)> = members
                .iter()
                .map(|(key, member)| (key.clone(), canonical(member)))
                .collect();
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            let body: Vec<String> = sorted
                .into_iter()
                .map(|(key, formatted)| format!("{}:{}", quote(&key), formatted))
                .collect();
            format!("{{{}}}", body.join(","))
        }
    }
}

/// One JSON string literal with the closed escape set.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
