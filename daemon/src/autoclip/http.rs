//! Just enough HTTP/1.1 to receive CS2's Game State Integration POSTs.
//!
//! The game is the only client: one small JSON body with a Content-Length,
//! from 127.0.0.1. A full HTTP stack would be a large dependency for that,
//! so this parses a request head, bounds everything, answers, and closes.

use anyhow::Result;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Largest request head accepted.
const MAX_HEAD: usize = 8 * 1024;
/// Largest body accepted. The sections the cfg asks for come to a few KiB;
/// even `allplayers_*` on a full server stays far below this.
pub const MAX_BODY: usize = 256 * 1024;
/// A client that stalls mid-request is dropped after this.
const READ_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq)]
pub enum Head {
    /// Need more bytes.
    Incomplete,
    /// The head ends at `body_start`; the body is `len` bytes.
    Post { body_start: usize, len: usize },
    /// Refuse with this status line.
    Reject(&'static str),
}

/// Parse what has arrived so far.
pub fn parse_head(buf: &[u8]) -> Head {
    let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if buf.len() > MAX_HEAD { Head::Reject("431 Request Header Fields Too Large") } else { Head::Incomplete };
    };
    if end > MAX_HEAD {
        return Head::Reject("431 Request Header Fields Too Large");
    }
    let Ok(head) = std::str::from_utf8(&buf[..end]) else {
        return Head::Reject("400 Bad Request");
    };
    let mut lines = head.split("\r\n");
    if !lines.next().unwrap_or("").starts_with("POST ") {
        return Head::Reject("405 Method Not Allowed");
    }
    let mut len = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let (name, value) = (name.trim(), value.trim());
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Head::Reject("411 Length Required");
        }
        if name.eq_ignore_ascii_case("content-length") {
            match value.parse::<usize>() {
                Ok(n) => len = Some(n),
                Err(_) => return Head::Reject("400 Bad Request"),
            }
        }
    }
    match len {
        None => Head::Reject("411 Length Required"),
        Some(n) if n > MAX_BODY => Head::Reject("413 Content Too Large"),
        Some(n) => Head::Post { body_start: end + 4, len: n },
    }
}

/// Read one request from `stream`. `Ok(None)` means the client went away or
/// was refused (and has been answered).
pub async fn read_post(stream: &mut TcpStream) -> Result<Option<Vec<u8>>> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    let (body_start, len) = loop {
        match parse_head(&buf) {
            Head::Post { body_start, len } => break (body_start, len),
            Head::Reject(status) => {
                respond(stream, status).await;
                return Ok(None);
            }
            Head::Incomplete => {}
        }
        let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk)).await??;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    while buf.len() < body_start + len {
        let n = tokio::time::timeout(READ_TIMEOUT, stream.read(&mut chunk)).await??;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    buf.truncate(body_start + len);
    Ok(Some(buf.split_off(body_start)))
}

/// Answer with an empty body and close. Failures are ignored: the client
/// has gone, and CS2 resends on its own.
pub async fn respond(stream: &mut TcpStream, status: &str) {
    let msg = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.write_all(msg.as_bytes()).await;
    let _ = stream.shutdown().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_what_cs2_sends() {
        let req = b"POST / HTTP/1.1\r\nHost: 127.0.0.1:37820\r\nContent-Type: application/json\r\ncontent-length: 2\r\n\r\n{}";
        let Head::Post { body_start, len } = parse_head(req) else { panic!() };
        assert_eq!(&req[body_start..body_start + len], b"{}");
    }

    #[test]
    fn waits_for_the_whole_head() {
        assert_eq!(parse_head(b"POST / HTTP/1.1\r\nContent-Len"), Head::Incomplete);
    }

    #[test]
    fn refuses_what_it_does_not_handle() {
        assert_eq!(parse_head(b"GET / HTTP/1.1\r\n\r\n"), Head::Reject("405 Method Not Allowed"));
        assert_eq!(parse_head(b"POST / HTTP/1.1\r\n\r\n"), Head::Reject("411 Length Required"));
        assert_eq!(
            parse_head(b"POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"),
            Head::Reject("411 Length Required")
        );
        let big = format!("POST / HTTP/1.1\r\nContent-Length: {}\r\n\r\n", MAX_BODY + 1);
        assert_eq!(parse_head(big.as_bytes()), Head::Reject("413 Content Too Large"));
        let endless = vec![b'a'; MAX_HEAD + 1];
        assert_eq!(parse_head(&endless), Head::Reject("431 Request Header Fields Too Large"));
    }
}
