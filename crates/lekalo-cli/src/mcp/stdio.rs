//! The stdio transport for the MCP server (issue #33): LF-delimited
//! UTF-8 JSON-RPC frames, bounded admission, and serialized writes.
//!
//! Stdin is one JSON-RPC message per LF-delimited frame; stdout carries
//! protocol frames only and is written under one lock so notifications
//! and concurrent replies can never interleave bytes; sanitized bounded
//! logging goes to stderr. Malformed JSON is `-32700`, unknown methods
//! `-32601`, unknown tools and invalid call parameters `-32602`; private
//! operands are never echoed.

use std::io::{BufRead, Write};

use lekalo_core::mcp::version::{MAX_FRAME_BYTES, MAX_NESTING_DEPTH};

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
/// frame cap. A missing final LF at EOF still yields the frame.
pub fn read_frame(reader: &mut impl BufRead) -> Result<String, FrameError> {
    let mut buffer = Vec::with_capacity(1024);
    let read = reader.read_until(b'\n', &mut buffer);
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
    if buffer.len() > MAX_FRAME_BYTES {
        return Err(FrameError::OverLimit);
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
pub struct Writer {
    sink: std::sync::Mutex<Box<dyn Write + Send>>,
}

impl Writer {
    /// Wrap one destination (stdout for the server lifetime).
    pub fn new(sink: Box<dyn Write + Send>) -> Self {
        Self {
            sink: std::sync::Mutex::new(sink),
        }
    }

    /// Write one complete JSON-RPC frame; the trailing LF is the frame
    /// delimiter and every write is flushed before returning.
    pub fn send(&self, frame: &str) -> bool {
        let mut sink = self.sink.lock().expect("writer lock");
        let written = sink
            .write_all(frame.as_bytes())
            .and_then(|()| sink.write_all(b"\n"))
            .and_then(|()| sink.flush());
        written.is_ok()
    }
}

/// A bounded stderr log line (sanitized: one closed event id, no payload
/// echo). Failures are silent: logging never breaks the protocol stream.
pub fn log_event(event: &str) {
    let mut sink = std::io::stderr().lock();
    let _ = writeln!(sink, "lekalo-mcp: {event}");
}

/// The JSON-RPC error response for a request id.
pub fn error_response(id: &serde_json::Value, code: i64, message: &str) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{},\"error\":{{\"code\":{code},\"message\":\"{message}\"}}}}",
        serde_json::to_string(id).unwrap_or_else(|_| "null".to_owned())
    )
}

/// The JSON-RPC success response for a request id.
pub fn result_response(id: &serde_json::Value, result: &str) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{},\"result\":{result}}}",
        serde_json::to_string(id).unwrap_or_else(|_| "null".to_owned())
    )
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
            let writer = Writer::new(Box::new(SharedBuffer(std::sync::Arc::clone(&shared))));
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
        assert_eq!(
            error_response(&id, -32601, "method-not-found"),
            "{\"jsonrpc\":\"2.0\",\"id\":7,\"error\":{\"code\":-32601,\"message\":\"method-not-found\"}}"
        );
        assert_eq!(
            result_response(&id, "{}"),
            "{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{}}"
        );
    }
}
