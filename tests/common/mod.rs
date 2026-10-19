//! Shared raw-HTTP helpers. Requests are written as raw HTTP/1.1 bytes so the
//! security matrix exercises the real hyper stack, not a test-only router path.

#![allow(dead_code)]

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Build a raw HTTP/1.1 request. The caller supplies every header, including
/// `Host`, so tests can forge or omit it deliberately.
pub fn raw_request(method: &str, target: &str, headers: &[(&str, String)], body: &str) -> String {
    let mut req = format!("{method} {target} HTTP/1.1\r\n");
    for (name, value) in headers {
        req.push_str(&format!("{name}: {value}\r\n"));
    }
    if !body.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    req.push_str("Connection: close\r\n\r\n");
    req.push_str(body);
    req
}

/// Convenience for well-formed GETs against the loopback server.
pub fn get(port: u16, target: &str) -> String {
    raw_request("GET", target, &[("Host", format!("127.0.0.1:{port}"))], "")
}

/// Convenience for well-formed POSTs with a token header.
pub fn post_json(port: u16, target: &str, token: &str, body: &str) -> String {
    raw_request(
        "POST",
        target,
        &[
            ("Host", format!("127.0.0.1:{port}")),
            ("Origin", format!("http://127.0.0.1:{port}")),
            ("Content-Type", "application/json".into()),
            ("X-Canvas-Token", token.into()),
        ],
        body,
    )
}

/// Send a request and return (status code, full response text).
pub async fn send(port: u16, request: &str) -> (u16, String) {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect to canvas");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("write request");
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.expect("read response");
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse().ok())
        .expect("status line");
    (status, text)
}

/// Response body after the header block.
pub fn body_of(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("")
}
