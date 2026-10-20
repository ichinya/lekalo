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

/// The request-ID ceiling in encoded JSON bytes (r8 F6). The writer
/// additionally reserves space for the correlated output-limit refusal
/// within the effective response cap, which may be smaller than this.
pub const MAX_REQUEST_ID_BYTES: usize = 512;

/// Measure the serialized spelling, not decoded length: JSON escapes
/// can expand a short string sixfold. Structured and null ids refuse.
pub(crate) fn id_admitted(id: &serde_json::Value) -> bool {
    matches!(
        id,
        serde_json::Value::String(_) | serde_json::Value::Number(_)
    ) && serde_json::to_vec(id).is_ok_and(|bytes| bytes.len() <= MAX_REQUEST_ID_BYTES)
}

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

/// Read one LF-delimited frame from the reader, bounded by the recorded
/// frame cap. The buffer itself is capped: reading stops at
/// `MAX_FRAME_BYTES + 1` bytes and reports `OverLimit` without ever
/// allocating past the bound, so one newline-free or oversized stream
/// cannot exhaust memory before rejection. A missing final LF at EOF
/// still yields the frame.
///
/// Review r2 F8: an oversized frame's tail must poison the connection,
/// never execute as a new request — on overflow the reader discards
/// bytes through the line's own LF with a bounded scan (the frame
/// boundaries stay intact), so the remainder of the refused line can
/// never resurface as a frame.
pub fn read_frame(reader: &mut impl BufRead) -> Result<String, FrameError> {
    // One past the bound: the first byte beyond the cap proves the
    // overrun and the frame is refused without consuming the rest.
    let mut buffer = Vec::with_capacity(1024.min(MAX_FRAME_BYTES + 1));
    let ends_the_line = {
        let mut taken = reader.take((MAX_FRAME_BYTES + 1) as u64);
        match taken.read_until(b'\n', &mut buffer) {
            Ok(0) => return Err(FrameError::Eof),
            Ok(_) => {}
            Err(_) => return Err(FrameError::Malformed),
        }
        buffer.last() == Some(&b'\n')
    };
    if buffer.len() > MAX_FRAME_BYTES {
        // Drain the refused line through its own LF (bounded scan) so
        // the tail cannot be re-parsed as a frame (r2 F8). The scan is
        // byte-exact: it consumes exactly through the line's first LF and
        // never touches the following frames. The capped reader's borrow
        // already ended with the scope above.
        if !ends_the_line {
            let mut drained = 0usize;
            loop {
                let mut one = [0u8; 1];
                match reader.read_exact(&mut one) {
                    Ok(()) => {
                        if one[0] == b'\n' {
                            break;
                        }
                        drained += 1;
                        // A newline-free stream past the drain bound is
                        // hopeless framing: the connection is poisoned
                        // (the session ends instead of re-syncing).
                        if drained > MAX_FRAME_BYTES.saturating_mul(16) {
                            return Err(FrameError::Eof);
                        }
                    }
                    // The stream ended inside the refused line: the
                    // over-limit report still ships, and the next read
                    // reports the end of the session.
                    Err(_) => break,
                }
            }
        }
        return Err(FrameError::OverLimit);
    }
    if ends_the_line {
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

    /// A request may execute only when its id fits BOTH the ordinary
    /// frame policy and the largest correlated refusal at this cap.
    pub fn admits_id(&self, id: &serde_json::Value) -> bool {
        id_admitted(id)
            && self.limit_refusal(id, "protocol-error", usize::MAX).len() <= self.max_response_bytes
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
    /// substituted refusal carries the request id (r2 F5 — a host must
    /// be able to correlate the refusal to its pending call) and cannot
    /// exceed the bound because its own bytes are checked before the
    /// write; a refusal envelope over the bound itself is dropped
    /// without a write (nothing partial ships). With the request-id
    /// admission bound (r8 omp F6) the drop arm is unreachable for
    /// admitted ids: both the envelope and its refusal twin reserve
    /// their id bytes within the effective bound.
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
        self.write_measured(frame, "tool-result", id)
    }

    /// The error-path twin of [`Writer::send_result`]: one chokepoint,
    /// one measurement, one typed substitution — with the same request-id
    /// correlation (r2 F5).
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
        self.write_measured(frame, "protocol-error", id)
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

    fn limit_refusal(&self, id: &serde_json::Value, surface: &str, actual: usize) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": INVALID_REQUEST_CODE,
                "message": lekalo_core::mcp::diagnostic::OUTPUT_LIMIT,
                "data": {
                    "code": "LEK-MCP-005",
                    "registry": "0.8.0",
                    "surface": surface,
                    "limit": self.max_response_bytes,
                    "actual": actual
                }
            }
        })
        .to_string()
    }

    /// Measure the complete encoded frame (payload + LF) against the
    /// effective bound and either write it or substitute the compact
    /// typed `mcp.output-limit` refusal. The refusal carries the request
    /// id (bounded: an unspellable id degenerates to null by the same
    /// rule the envelope applies, r2 F5), the registered diagnostic
    /// identity, and the byte counts — never the refused payload. When
    /// correlation cannot fit, a compact null-id request-limit refusal
    /// is used; an unrepresentably small cap returns false.
    fn write_measured(&self, frame: String, surface: &str, id: &serde_json::Value) -> bool {
        let encoded = frame.len() + 1;
        if encoded <= self.max_response_bytes {
            return self.send(&frame);
        }
        let bounded_id = if self.admits_id(id) {
            id.clone()
        } else {
            serde_json::Value::Null
        };
        let text = self.limit_refusal(&bounded_id, surface, encoded);
        if text.len() < self.max_response_bytes {
            return self.send(&text);
        }
        // An unadmitted id or tiny cap must not black-hole a pending
        // request while keeping the session alive. Try a compact null-id
        // refusal; false tells the caller that even this cannot ship.
        let compact = serde_json::json!({"jsonrpc":"2.0", "id":null,
            "error":{"code":INVALID_REQUEST_CODE,"message":"mcp.request-limit"}})
        .to_string();
        compact.len() < self.max_response_bytes && self.send(&compact)
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
    fn an_oversized_frame_poisons_its_tail_not_the_next_frame() {
        // Review r2 F8: the refused frame's tail is drained through its
        // own LF, so it can never resurface as a frame; the next line is
        // served intact.
        let mut bytes = Vec::new();
        bytes.extend_from_slice("x".repeat(MAX_FRAME_BYTES + 8).as_bytes());
        bytes.extend_from_slice(b"\n{}\n");
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(bytes));
        assert_eq!(read_frame(&mut reader), Err(FrameError::OverLimit));
        assert_eq!(read_frame(&mut reader), Ok("{}".to_owned()));
        assert_eq!(read_frame(&mut reader), Err(FrameError::Eof));
    }

    #[test]
    fn a_newline_free_oversized_stream_ends_the_session() {
        // No LF anywhere within the drain bound: the connection is
        // poisoned (the tail can never execute) and the reader reports
        // end-of-stream.
        let payload = "{".repeat(MAX_FRAME_BYTES * 4);
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(payload.into_bytes()));
        assert_eq!(read_frame(&mut reader), Err(FrameError::OverLimit));
        assert_eq!(read_frame(&mut reader), Err(FrameError::Eof));
    }

    #[test]
    fn id_admission_reserves_the_refusal_budget() {
        // Exercise the real writer at reduced caps, not a hypothetical
        // frame-size floor (the launcher may configure a much lower cap).
        let id = serde_json::json!("x".repeat(MAX_REQUEST_ID_BYTES - 2));
        assert!(id_admitted(&id));
        for cap in [256, 512, 2048] {
            for id in [
                serde_json::json!(7),
                serde_json::json!("x".repeat(100)),
                id.clone(),
            ] {
                let shared = std::sync::Arc::default();
                let writer =
                    Writer::new(Box::new(SharedBuffer(std::sync::Arc::clone(&shared))), cap);
                if !writer.admits_id(&id) {
                    continue;
                }
                assert!(writer.send_result(&id, serde_json::json!({"content":"x".repeat(8192)})));
                let written = shared.lock().unwrap().clone();
                assert!(written.len() <= cap);
                let frame: serde_json::Value = serde_json::from_slice(&written).unwrap();
                assert_eq!(frame["id"], id);
                assert_eq!(frame["error"]["message"], "mcp.output-limit");
            }
        }
        assert!(!id_admitted(&serde_json::Value::String(
            "x".repeat(MAX_REQUEST_ID_BYTES + 1)
        )));
        assert!(!id_admitted(&serde_json::Value::String(
            "\u{0001}".repeat(100)
        )));
        let writer = Writer::new(Box::new(Vec::<u8>::new()), 512);
        assert!(writer.admits_id(&serde_json::json!(7)));
        assert!(!writer.admits_id(&serde_json::json!("x".repeat(512 - 2))));
        assert!(id_admitted(&serde_json::Value::Number(7.into())));
        assert!(!id_admitted(&serde_json::Value::Bool(true)));
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
    fn the_chokepoint_substitutes_the_typed_output_limit_refusal_with_the_request_id() {
        let shared: std::sync::Arc<std::sync::Mutex<Vec<u8>>> = std::sync::Arc::default();
        {
            let writer = Writer::new(Box::new(SharedBuffer(std::sync::Arc::clone(&shared))), 256);
            let result = serde_json::json!({"content": "x".repeat(4096)});
            // Review r2 F5: the substitution carries the pending call's
            // id, so the host can correlate the refusal.
            assert!(writer.send_result(&serde_json::json!(2), result));
        }
        let written = String::from_utf8(shared.lock().expect("buffer").clone()).expect("utf8");
        let document: serde_json::Value = serde_json::from_str(&written).expect("one closed frame");
        assert_eq!(document["error"]["message"], "mcp.output-limit");
        assert_eq!(document["error"]["code"], -32600);
        assert_eq!(document["error"]["data"]["code"], "LEK-MCP-005");
        assert_eq!(document["id"], 2, "the request id correlates the refusal");
        // The refusal frame itself stays under the bound and never
        // carries the refused payload.
        assert!(written.len() < 256, "{}", written.len());
        assert!(!written.contains("xxxxxxxxxx"));
    }

    #[test]
    fn an_unspellable_request_id_degenerates_in_the_output_limit_refusal() {
        let shared: std::sync::Arc<std::sync::Mutex<Vec<u8>>> = std::sync::Arc::default();
        {
            let writer = Writer::new(Box::new(SharedBuffer(std::sync::Arc::clone(&shared))), 128);
            let result = serde_json::json!({"content": "x".repeat(4096)});
            assert!(writer.send_result(&serde_json::json!({"structured": true}), result));
        }
        let written = String::from_utf8(shared.lock().expect("buffer").clone()).expect("utf8");
        assert!(written.len() <= 128);
        let document: serde_json::Value = serde_json::from_str(&written).expect("one frame");
        assert!(document["id"].is_null());
        assert_eq!(document["error"]["message"], "mcp.request-limit");
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
