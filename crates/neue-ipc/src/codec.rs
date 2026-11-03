//! Newline-delimited JSON framing: one UTF-8 JSON object per line, at most
//! [`MAX_LINE_BYTES`] per line.
//!
//! [`LineBuffer`] is for non-blocking readers (the compositor): feed it whatever a read
//! returned and pop complete lines. [`read_line`] is the blocking equivalent for clients.

use std::io::{self, BufRead};

use crate::types::{ErrorBody, ErrorCode, Outcome, Reply, RequestEnvelope};

/// Longest line (without the newline) either side accepts.
pub const MAX_LINE_BYTES: usize = 1024 * 1024;

/// A line exceeded [`MAX_LINE_BYTES`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("line longer than {MAX_LINE_BYTES} bytes")]
pub struct LineTooLong;

/// Accumulates bytes and yields complete lines.
#[derive(Debug, Default)]
pub struct LineBuffer {
    buf: Vec<u8>,
}

impl LineBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append bytes from a read.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Pop the next complete line, without its newline (and without a trailing `\r`).
    /// `Ok(None)` means more bytes are needed; an error means the peer sent an oversized
    /// line and the connection should be dropped.
    pub fn pop_line(&mut self) -> Result<Option<Vec<u8>>, LineTooLong> {
        match self.buf.iter().position(|&b| b == b'\n') {
            Some(end) if end <= MAX_LINE_BYTES => {
                let mut line: Vec<u8> = self.buf.drain(..=end).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                Ok(Some(line))
            }
            Some(_) => Err(LineTooLong),
            None if self.buf.len() > MAX_LINE_BYTES => Err(LineTooLong),
            None => Ok(None),
        }
    }
}

/// Read one line from a blocking reader. `Ok(None)` is end of stream. Fails with
/// `InvalidData` if the line exceeds [`MAX_LINE_BYTES`].
pub fn read_line<R: BufRead>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let available = match reader.fill_buf() {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        if available.is_empty() {
            // End of stream; a final unterminated line is dropped.
            return Ok(None);
        }
        let (used, done) = match available.iter().position(|&b| b == b'\n') {
            Some(i) => {
                line.extend_from_slice(&available[..i]);
                (i + 1, true)
            }
            None => {
                line.extend_from_slice(available);
                (available.len(), false)
            }
        };
        reader.consume(used);
        if line.len() > MAX_LINE_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, LineTooLong));
        }
        if done {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            return Ok(Some(line));
        }
    }
}

/// Serialize `value` as one line, newline included.
pub fn encode_line<T: serde::Serialize>(value: &T) -> Vec<u8> {
    // These types contain only strings, numbers and maps with string keys, so
    // serialization cannot fail; fall back to an empty object just in case.
    let mut line = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    line.push(b'\n');
    line
}

/// Parse a request line. On failure returns the reply to send: it echoes the `id` when the
/// line had one.
pub fn parse_request(line: &[u8]) -> Result<RequestEnvelope, Reply> {
    let fail = |id: Option<u64>, code: ErrorCode, message: String| Reply {
        id,
        outcome: Outcome::Error(ErrorBody::new(code, message)),
    };
    let value: serde_json::Value = serde_json::from_slice(line).map_err(|err| {
        fail(
            None,
            ErrorCode::InvalidRequest,
            format!("not valid JSON: {err}"),
        )
    })?;
    let Some(id) = value.get("id").and_then(|id| id.as_u64()) else {
        return Err(fail(
            None,
            ErrorCode::InvalidRequest,
            "missing or non-integer `id`".into(),
        ));
    };
    let Some(name) = value.get("request").and_then(|r| r.as_str()) else {
        return Err(fail(
            Some(id),
            ErrorCode::InvalidRequest,
            "missing `request`".into(),
        ));
    };
    let name = name.to_owned();
    serde_json::from_value::<RequestEnvelope>(value).map_err(|err| {
        if KNOWN_REQUESTS.contains(&name.as_str()) {
            fail(
                Some(id),
                ErrorCode::InvalidParams,
                format!("bad params for `{name}`: {err}"),
            )
        } else {
            fail(
                Some(id),
                ErrorCode::UnknownRequest,
                format!("unknown request `{name}`"),
            )
        }
    })
}

/// Request names this version understands, as they appear on the wire.
const KNOWN_REQUESTS: &[&str] = &[
    "version",
    "windows",
    "outputs",
    "focused-window",
    "actions",
    "action",
    "subscribe",
    "quit",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Request;
    use std::io::Cursor;

    #[test]
    fn line_buffer_splits_lines_across_pushes() {
        let mut buf = LineBuffer::new();
        buf.push(b"{\"a\":1}\n{\"b\"");
        assert_eq!(buf.pop_line().unwrap().unwrap(), b"{\"a\":1}");
        assert_eq!(buf.pop_line().unwrap(), None);
        buf.push(b":2}\r\n");
        assert_eq!(buf.pop_line().unwrap().unwrap(), b"{\"b\":2}");
        assert_eq!(buf.pop_line().unwrap(), None);
    }

    #[test]
    fn line_buffer_rejects_oversized_lines() {
        let mut buf = LineBuffer::new();
        buf.push(&vec![b'x'; MAX_LINE_BYTES + 1]);
        assert_eq!(buf.pop_line(), Err(LineTooLong));

        // Exactly at the limit is fine.
        let mut buf = LineBuffer::new();
        buf.push(&vec![b'x'; MAX_LINE_BYTES]);
        buf.push(b"\n");
        assert_eq!(buf.pop_line().unwrap().unwrap().len(), MAX_LINE_BYTES);
    }

    #[test]
    fn read_line_reads_until_eof() {
        let mut reader = Cursor::new(b"one\ntwo\r\nthree".to_vec());
        assert_eq!(read_line(&mut reader).unwrap().unwrap(), b"one");
        assert_eq!(read_line(&mut reader).unwrap().unwrap(), b"two");
        // A trailing line without a newline is dropped.
        assert_eq!(read_line(&mut reader).unwrap(), None);
    }

    #[test]
    fn read_line_rejects_oversized_lines() {
        let mut reader = Cursor::new(vec![b'x'; MAX_LINE_BYTES + 10]);
        let err = read_line(&mut reader).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    fn parse(text: &str) -> Result<RequestEnvelope, Reply> {
        parse_request(text.as_bytes())
    }

    fn code(reply: &Reply) -> ErrorCode {
        match &reply.outcome {
            Outcome::Error(body) => body.code,
            Outcome::Ok(_) => panic!("expected an error"),
        }
    }

    #[test]
    fn parses_valid_requests() {
        let env = parse(r#"{"id": 4, "request": "version"}"#).unwrap();
        assert_eq!((env.id, env.request), (4, Request::Version));
    }

    #[test]
    fn bad_lines_get_error_replies() {
        let reply = parse("not json").unwrap_err();
        assert_eq!((reply.id, code(&reply)), (None, ErrorCode::InvalidRequest));

        let reply = parse(r#"{"request": "version"}"#).unwrap_err();
        assert_eq!((reply.id, code(&reply)), (None, ErrorCode::InvalidRequest));

        let reply = parse(r#"{"id": 5}"#).unwrap_err();
        assert_eq!(
            (reply.id, code(&reply)),
            (Some(5), ErrorCode::InvalidRequest)
        );

        let reply = parse(r#"{"id": 6, "request": "spaces"}"#).unwrap_err();
        assert_eq!(
            (reply.id, code(&reply)),
            (Some(6), ErrorCode::UnknownRequest)
        );

        let reply = parse(r#"{"id": 7, "request": "action"}"#).unwrap_err();
        assert_eq!(
            (reply.id, code(&reply)),
            (Some(7), ErrorCode::InvalidParams)
        );
    }

    #[test]
    fn known_requests_list_matches_the_enum() {
        // Every name in the list must parse (with params where needed) to a request.
        for name in KNOWN_REQUESTS {
            let params = match *name {
                "action" => r#","params":{"name":"x"}"#,
                "subscribe" => r#","params":{"topics":[]}"#,
                _ => "",
            };
            let line = format!(r#"{{"id":1,"request":"{name}"{params}}}"#);
            assert!(parse(&line).is_ok(), "{name} should parse");
        }
    }

    #[test]
    fn encoded_lines_end_with_one_newline() {
        let line = encode_line(&crate::types::Event::FocusChanged { window: None });
        assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1);
        assert_eq!(line.last(), Some(&b'\n'));
    }
}
