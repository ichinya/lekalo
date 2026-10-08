//! The stdio transport for the MCP server (issue #33): LF-delimited
//! UTF-8 JSON-RPC frames, bounded admission, and serialized writes.
//!
//! Stdin is one JSON-RPC message per LF-delimited frame; stdout carries
//! protocol frames only and is written under one lock so notifications
//! and concurrent replies can never interleave bytes; sanitized bounded
//! logging goes to stderr. Malformed JSON is `-32700`, unknown methods
//! `-32601`, unknown tools and invalid call parameters `-32602`; private
//! operands are never echoed.

use std::io::{BufRead, Read, Write};

use lekalo_core::mcp::version::{MAX_FRAME_BYTES, MAX_NESTING_DEPTH};

/// The JSON-RPC interstitial-error code the writer's own refusals use.
const INVALID_REQUEST_CODE: i64 = -32600;

/// Why one incoming frame failed admission or parsing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    /// The frame exceeded the recorded byte bound (never echoed).
    OverLimit,
    /// The frame is not valid UTF-8 or not valid JSON.
    Malformed,
    /// The parsed document exceeds the nesting bound.
    Depth,
    /// The client closed the stream.
    Eof,
}

/// Compact one JSON text without re-ordering or re-escaping members:
/// whitespace outside strings is removed, so an embedded pretty-printed
/// contract or a reused envelope's diagnostics splice into a one-line LF
/// frame with the exact wire member order preserved.
pub(crate) fn compact_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for character in text.chars() {
        if escaped {
            out.push(character);
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_string => {
                out.push(character);
                escaped = true;
            }
            '"' => {
                in_string = !in_string;
                out.push(character);
            }
            ' ' | '\t' | '\n' | '\r' if !in_string => {}
            _ => out.push(character),
        }
    }
    out
}

/// Read one LF-delimited frame from the reader, bounded by the recorded
/// frame cap. The buffer itself is capped: reading stops at
/// `MAX_FRAME_BYTES + 1` bytes and reports `OverLimit` without ever
/// allocating past the bound, so one newline-free or oversized stream
/// cannot exhaust memory before rejection. A missing final LF at EOF
/// still yields the frame.
pub fn read_frame(reader: &mut impl BufRead) -> Result<String, FrameError> {
    // One past the bound: the first byte beyond the cap proves the
    // overrun and the frame is refused without consuming the rest.
    let mut buffer = Vec::with_capacity(1024.min(MAX_FRAME_BYTES + 1));
    let mut taken = reader.take((MAX_FRAME_BYTES + 1) as u64);
    let read = taken.read_until(b'\n', &mut buffer);
    match read {
        Ok(0) => return Err(FrameError::Eof),
        Ok(_) => {}
        Err(_) => return Err(FrameError::Malformed),
    }
    if buffer.len() > MAX_FRAME_BYTES {
        return Err(FrameError::OverLimit);
    }
    if buffer.last() == Some(&b'\n') {
        buffer.pop();
        if buffer.last() == Some(&b'\r') {
            buffer.pop();
        }
    }
    let text = String::from_utf8(buffer).map_err(|_| FrameError::Malformed)?;
    if text.is_empty() {
        // An empty line is not a frame; tolerate keep-alive whitespace
        // frames by reporting them as malformed-empty (the caller skips).
        return Err(FrameError::Malformed);
    }
    Ok(text)
}

/// The maximum JSON nesting depth of one admitted frame.
pub fn check_depth(value: &serde_json::Value) -> Result<(), FrameError> {
    fn walk(value: &serde_json::Value, depth: usize) -> Result<(), FrameError> {
        if depth > MAX_NESTING_DEPTH {
            return Err(FrameError::Depth);
        }
        match value {
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, depth + 1)?;
                }
            }
            serde_json::Value::Object(map) => {
                for item in map.values() {
                    walk(item, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(value, 1)
}

/// The serialized protocol writer: one lock, whole frames, no interleaving.
/// Every response rides the single serialization chokepoint
/// [`Writer::send_result`]/[`Writer::send_error`], which builds the
/// `serde_json::Value` envelope, serializes once, and measures the
/// complete encoded frame against the effective response bound: an
/// over-limit response is replaced by the typed `mcp.output-limit`
/// refusal — never a partial frame and never raw interpolation.
pub struct Writer {
    sink: std::sync::Mutex<Box<dyn Write + Send>>,
    /// The effective complete-response bound in bytes.
    max_response_bytes: usize,
}

impl Writer {
    /// Wrap one destination (stdout for the server lifetime) with the
    /// effective response bound.
    pub fn new(sink: Box<dyn Write + Send>, max_response_bytes: usize) -> Self {
        Self {
            sink: std::sync::Mutex::new(sink),
            max_response_bytes: max_response_bytes.max(1),
        }
    }

    /// The bound this writer enforces.
    #[allow(dead_code)]
    #[cfg(test)]
    pub fn max_response_bytes(&self) -> usize {
        self.max_response_bytes
    }

    /// Serialize one complete JSON-RPC frame (already a closed string)
    /// and write it; the trailing LF is the frame delimiter and every
    /// write is flushed before returning. Hand-assembled frames are
    /// admitted only through the closed builders below.
    pub fn send(&self, frame: &str) -> bool {
        let mut sink = self.sink.lock().expect("writer lock");
        let written = sink
            .write_all(frame.as_bytes())
            .and_then(|()| sink.write_all(b"\n"))
            .and_then(|()| sink.flush());
        written.is_ok()
    }

    /// The single outgoing-response chokepoint: build the success
    /// envelope as a `serde_json::Value`, serialize once, measure the
    /// whole encoded frame, and substitute the typed `mcp.output-limit`
    /// refusal when the response exceeds the effective bound. The
    /// substituted refusal also cannot exceed the bound because its own
    /// bytes are checked before the write; a refusal envelope over the
    /// bound itself is dropped without a write (nothing partial ships).
    pub fn send_result(&self, id: &serde_json::Value, result: serde_json::Value) -> bool {
        let frame = self.envelope_frame(
            id,
            serde_json::Value::Object(
                [
                    ("jsonrpc".to_owned(), serde_json::Value::from("2.0")),
                    ("id".to_owned(), id.clone()),
                    ("result".to_owned(), result),
                ]
                .into_iter()
                .collect(),
            ),
        );
        self.write_measured(frame, "tool-result")
    }

    /// The error-path twin of [`Writer::send_result`]: one chokepoint,
    /// one measurement, one typed substitution.
    pub fn send_error(&self, id: &serde_json::Value, code: i64, message: &str) -> bool {
        let frame = self.envelope_frame(
            id,
            serde_json::Value::Object(
                [
                    ("jsonrpc".to_owned(), serde_json::Value::from("2.0")),
                    ("id".to_owned(), id.clone()),
                    (
                        "error".to_owned(),
                        serde_json::json!({"code": code, "message": message}),
                    ),
                ]
                .into_iter()
                .collect(),
            ),
        );
        self.write_measured(frame, "protocol-error")
    }

    /// Serialize the whole envelope once.
    fn envelope_frame(&self, id: &serde_json::Value, mut envelope: serde_json::Value) -> String {
        if !id.is_null()
            && !matches!(
                id,
                serde_json::Value::String(_) | serde_json::Value::Number(_)
            )
        {
            // A structurally unsafe id degenerates to null; the frame
            // stays a closed JSON-RPC document.
            if let Some(object) = envelope.as_object_mut() {
                object.insert("id".to_owned(), serde_json::Value::Null);
            }
        }
        serde_json::to_string(&envelope).unwrap_or_else(|_| {
            "{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32700,\"message\":\"parse-error\"}}"
                .to_owned()
        })
    }

    /// Measure the complete encoded frame (payload + LF) against the
    /// effective bound and either write it or substitute the compact
    /// typed `mcp.output-limit` refusal. The refusal carries the
    /// registered diagnostic identity and the byte counts — never the
    /// refused payload. A bound too small for even the refusal drops the
    /// write entirely: nothing partial ever ships.
    fn write_measured(&self, frame: String, surface: &str) -> bool {
        let encoded = frame.len() + 1;
        if encoded <= self.max_response_bytes {
            return self.send(&frame);
        }
        let refusal = serde_json::json!({
            "jsonrpc": "2.0",
            "id": serde_json::Value::Null,
            "error": {
                "code": INVALID_REQUEST_CODE,
                "message": lekalo_core::mcp::diagnostic::OUTPUT_LIMIT,
                "data": {
                    "code": "LEK-MCP-005",
                    "registry": "0.8.0",
                    "surface": surface,
                    "limit": self.max_response_bytes,
                    "actual": encoded
                }
            }
        });
        let text = serde_json::to_string(&refusal).unwrap_or_default();
        if text.len() + 1 > self.max_response_bytes {
            // Even the refusal exceeds the bound: no partial frame ever
            // ships, so the write is dropped entirely.
            return false;
        }
        self.send(&text)
    }
}

/// A bounded stderr log line (sanitized: one closed event id, no payload
/// echo). Failures are silent: logging never breaks the protocol stream.
pub fn log_event(event: &str) {
    let mut sink = std::io::stderr().lock();
    let _ = writeln!(sink, "lekalo-mcp: {event}");
}

/// The JSON-RPC error response for a request id, built as one closed
/// `serde_json::Value` (F16: no string interpolation on the response
/// path; the id rides serialized JSON).
#[cfg(test)]
pub(crate) fn error_response(id: &serde_json::Value, code: i64, message: &str) -> String {
    serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    }))
    .unwrap_or_default()
}

/// The JSON-RPC success response for a request id, built as one closed
/// `serde_json::Value`.
#[cfg(test)]
pub(crate) fn result_response(id: &serde_json::Value, result: serde_json::Value) -> String {
    serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Own the frame bytes in one cursor per call.
    fn reader(frames: Vec<String>) -> std::io::BufReader<std::io::Cursor<Vec<u8>>> {
        let mut bytes = Vec::new();
        for frame in frames {
            bytes.extend_from_slice(frame.as_bytes());
            bytes.push(b'\n');
        }
        std::io::BufReader::new(std::io::Cursor::new(bytes))
    }

    #[test]
    fn frames_split_on_lf_and_strip_cr() {
        let mut reader = reader(vec!["{\"a\":1}".to_owned(), "{\"b\":2}".to_owned()]);
        assert_eq!(read_frame(&mut reader), Ok("{\"a\":1}".to_owned()));
        assert_eq!(read_frame(&mut reader), Ok("{\"b\":2}".to_owned()));
        assert_eq!(read_frame(&mut reader), Err(FrameError::Eof));
    }

    #[test]
    fn oversized_frames_refuse_before_any_parse() {
        let mut reader = reader(vec!["x".repeat(MAX_FRAME_BYTES + 1)]);
        assert_eq!(read_frame(&mut reader), Err(FrameError::OverLimit));
    }

    #[test]
    fn a_newline_free_oversized_stream_refuses_without_unbounded_memory() {
        // No LF anywhere: the capped read stops one byte past the bound
        // and refuses; the allocation never exceeds cap + 1.
        let payload = "{".repeat(MAX_FRAME_BYTES * 4);
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(payload.into_bytes()));
        assert_eq!(read_frame(&mut reader), Err(FrameError::OverLimit));
    }

    #[test]
    fn an_oversized_frame_does_not_swallow_the_next_frame() {
        // The over-limit frame is abandoned at its first bytes; the next
        // read resumes at the remaining bytes of the refused line and
        // then continues with the following line, so the session keeps
        // serving (each protocol frame is one line).
        let mut bytes = Vec::new();
        bytes.extend_from_slice("x".repeat(MAX_FRAME_BYTES + 8).as_bytes());
        bytes.extend_from_slice(b"\n{}\n");
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
        assert_eq!(read_frame(&mut reader), Err(FrameError::OverLimit));
        // The abandoned tail of the refused line resurfaces as its own
        // short (malformed) frame; the stream never loses alignment.
        assert_eq!(read_frame(&mut reader), Ok("xxxxxxx".to_owned()));
        assert_eq!(read_frame(&mut reader), Ok("{}".to_owned()));
    }

    #[test]
    fn depth_is_bounded() {
        let mut deep = String::new();
        for _ in 0..MAX_NESTING_DEPTH {
            deep.push('[');
        }
        for _ in 0..MAX_NESTING_DEPTH {
            deep.push(']');
        }
        let value: serde_json::Value = serde_json::from_str(&deep).expect("json");
        assert!(check_depth(&value).is_ok());
        let deeper: serde_json::Value = serde_json::from_str(&format!("[{deep}]")).expect("json");
        assert_eq!(check_depth(&deeper), Err(FrameError::Depth));
    }

    #[test]
    fn writer_serializes_whole_frames() {
        let shared: std::sync::Arc<std::sync::Mutex<Vec<u8>>> = std::sync::Arc::default();
        {
            let writer = Writer::new(
                Box::new(SharedBuffer(std::sync::Arc::clone(&shared))),
                lekalo_core::mcp::version::DEFAULT_MAX_RESPONSE_BYTES,
            );
            assert!(writer.send("{\"x\":1}"));
            assert!(writer.send("{\"x\":2}"));
        }
        assert_eq!(
            shared.lock().expect("buffer").as_slice(),
            b"{\"x\":1}\n{\"x\":2}\n"
        );
    }

    #[derive(Clone)]
    struct SharedBuffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for SharedBuffer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().expect("buffer").extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn error_responses_carry_the_closed_codes() {
        let id = serde_json::json!(7);
        let error = error_response(&id, -32601, "method-not-found");
        let document: serde_json::Value = serde_json::from_str(&error).expect("closed frame");
        assert_eq!(document["jsonrpc"], "2.0");
        assert_eq!(document["id"], 7);
        assert_eq!(document["error"]["code"], -32601);
        assert_eq!(document["error"]["message"], "method-not-found");
        let result = result_response(&id, serde_json::json!({}));
        let document: serde_json::Value = serde_json::from_str(&result).expect("closed frame");
        assert_eq!(document["id"], 7);
        assert_eq!(document["result"], serde_json::json!({}));
    }

    #[test]
    fn the_chokepoint_substitutes_the_typed_output_limit_refusal() {
        let shared: std::sync::Arc<std::sync::Mutex<Vec<u8>>> = std::sync::Arc::default();
        {
            let writer = Writer::new(Box::new(SharedBuffer(std::sync::Arc::clone(&shared))), 256);
            let result = serde_json::json!({"content": "x".repeat(4096)});
            assert!(writer.send_result(&serde_json::json!(2), result));
        }
        let written = String::from_utf8(shared.lock().expect("buffer").clone()).expect("utf8");
        let document: serde_json::Value = serde_json::from_str(&written).expect("one closed frame");
        assert_eq!(document["error"]["message"], "mcp.output-limit");
        assert_eq!(document["error"]["code"], -32600);
        assert_eq!(document["error"]["data"]["code"], "LEK-MCP-005");
        // The refusal frame itself stays under the bound and never
        // carries the refused payload.
        assert!(written.len() < 256, "{}", written.len());
        assert!(!written.contains("xxxxxxxxxx"));
    }

    #[test]
    fn under_limit_responses_pass_the_chokepoint_unchanged() {
        let shared: std::sync::Arc<std::sync::Mutex<Vec<u8>>> = std::sync::Arc::default();
        {
            let writer = Writer::new(Box::new(SharedBuffer(std::sync::Arc::clone(&shared))), 4096);
            assert!(writer.send_result(&serde_json::json!(2), serde_json::json!({"ok": true})));
        }
        let written = String::from_utf8(shared.lock().expect("buffer").clone()).expect("utf8");
        let document: serde_json::Value = serde_json::from_str(&written).expect("one frame");
        assert_eq!(document["result"]["ok"], true);
    }
}
