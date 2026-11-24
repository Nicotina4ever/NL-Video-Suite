//! A tiny HTTP/1.1 client for the local AI services.
//!
//! The endpoints are loopback by design (`http://` only), so this speaks plain HTTP without TLS
//! and without pulling in a network stack. It is used for reading status and model lists; media
//! generation and transcription keep their own clients next to their features.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// HTTP methods this client needs.
#[derive(Clone, Copy)]
pub enum Method {
    Get,
    Post,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

/// One request to `url` (`http://authority/path`), returning the body of a 2xx response.
pub fn request(method: Method, url: &str, body: Option<&str>, timeout: Duration) -> Result<Vec<u8>, String> {
    request_with_headers(method, url, body, &[], timeout)
}

/// [`request`] with extra request headers (for example `Authorization` on an OpenAI-compatible
/// server).
pub fn request_with_headers(method: Method, url: &str, body: Option<&str>, headers: &[(&str, String)], timeout: Duration) -> Result<Vec<u8>, String> {
    let content_type = if method.as_str() == "POST" { Some("application/json") } else { None };
    send(method, url, body.unwrap_or("").as_bytes(), content_type, headers, timeout)
}

/// POST a raw body with an explicit content type (multipart uploads for the speech endpoint).
pub fn post_bytes_with_headers(url: &str, body: &[u8], content_type: &str, headers: &[(&str, String)], timeout: Duration) -> Result<Vec<u8>, String> {
    send(Method::Post, url, body, Some(content_type), headers, timeout)
}

fn send(method: Method, url: &str, body: &[u8], content_type: Option<&str>, headers: &[(&str, String)], timeout: Duration) -> Result<Vec<u8>, String> {
    let rest = url.trim().trim_end_matches('/');
    let rest = rest.strip_prefix("http://").ok_or_else(|| format!("{url}: only http:// endpoints are supported"))?;
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    let address = authority.to_socket_addrs().map_err(|e| format!("{authority}: {e}"))?.next().ok_or_else(|| format!("{authority}: resolved to no address"))?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(|e| format!("connect {authority}: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let mut head = format!("{} {path} HTTP/1.1\r\nHost: {authority}\r\nAccept: application/json\r\nConnection: close\r\n", method.as_str());
    if method.as_str() == "POST" {
        head.push_str(&format!("Content-Type: {}\r\n", content_type.unwrap_or("application/octet-stream")));
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).map_err(|e| format!("write: {e}"))?;
    stream.write_all(body).map_err(|e| format!("write: {e}"))?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|e| format!("read: {e}"))?;
    split_response(&raw).and_then(|(head, body)| {
        let status = head.lines().next().unwrap_or_default();
        if status.contains(" 2") {
            Ok(if head.to_ascii_lowercase().contains("transfer-encoding: chunked") { dechunk(body)? } else { body.to_vec() })
        } else {
            Err(format!("{status}: {}", String::from_utf8_lossy(body).trim().chars().take(400).collect::<String>()))
        }
    })
}

fn split_response(raw: &[u8]) -> Result<(String, &[u8]), String> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| "malformed HTTP response (no header end)".to_string())?;
    Ok((String::from_utf8_lossy(&raw[..split]).to_string(), &raw[split + 4..]))
}

fn dechunk(mut body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line = body.windows(2).position(|w| w == b"\r\n").ok_or_else(|| "malformed chunked response".to_string())?;
        let size = String::from_utf8_lossy(&body[..line]);
        let size = usize::from_str_radix(size.trim().split(';').next().unwrap_or("0").trim(), 16).map_err(|_| "bad chunk size".to_string())?;
        body = &body[line + 2..];
        if size == 0 {
            return Ok(out);
        }
        if body.len() < size + 2 {
            return Err("truncated chunked response".to_string());
        }
        out.extend_from_slice(&body[..size]);
        body = &body[size + 2..];
    }
}

/// GET `base + path` and parse the body as JSON.
pub fn get_json(base: &str, path: &str, timeout: Duration) -> Result<serde_json::Value, String> {
    let url = format!("{}/{path}", base.trim().trim_end_matches('/'));
    let body = request(Method::Get, &url, None, timeout)?;
    serde_json::from_slice(&body).map_err(|e| format!("{url}: not JSON: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_split_from_its_head() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        let (head, body) = split_response(raw).unwrap();
        assert!(head.starts_with("HTTP/1.1 200"));
        assert_eq!(body, b"{}");
    }

    #[test]
    fn only_http_endpoints_are_accepted() {
        let err = request(Method::Get, "https://127.0.0.1:11434/api/version", None, Duration::from_millis(50)).unwrap_err();
        assert!(err.contains("http://"));
    }
}
