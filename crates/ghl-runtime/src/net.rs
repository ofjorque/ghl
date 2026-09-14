//! Lightweight Native HTTP & TCP Microservice Primitives (RFC 04 §6).
//!
//! Provides zero-overhead HTTP client and async microservice server built directly
//! on standard library `std::net::{TcpListener, TcpStream}` without external crates.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::Duration;

use crate::eval::Interpreter;
use crate::value::Value;
use ghl_diagnostics::Diagnostic;

static SERVER_REGISTRY: LazyLock<Mutex<std::collections::HashMap<i64, (Arc<AtomicBool>, String)>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// `http::stop(port_or_server)`: Stops a running HTTP microservice server.
pub fn native_http_stop(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let port = match args.first() {
        Some(Value::I64(p)) => *p,
        Some(Value::Record(map)) => map.get("port").and_then(|v| v.as_i64()).unwrap_or(0),
        _ => 0,
    };

    if let Ok(mut reg) = SERVER_REGISTRY.lock() {
        if let Some((is_running, addr)) = reg.remove(&port) {
            is_running.store(false, Ordering::Relaxed);
            if let Ok(mut wake) = TcpStream::connect(&addr) {
                let _ = wake.write_all(b"QUIT");
            }
        }
    }
    Ok(Value::Unit)
}

/// `http::response(status, body, [content_type])`
/// Constructs a standard HTTP response record for microservices.
pub fn native_http_response(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.is_empty() {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`http::response(status, body, [content_type])` requires at least a status code",
        ));
    }

    let status = args[0].as_i64().unwrap_or(200);
    let body = match args.get(1) {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    let content_type = args
        .get(2)
        .and_then(|v| v.as_str())
        .unwrap_or("application/json")
        .to_string();

    let mut record = BTreeMap::new();
    record.insert("status".to_string(), Value::I64(status));
    record.insert("body".to_string(), Value::String(body));
    record.insert("content_type".to_string(), Value::String(content_type));

    Ok(Value::Record(Arc::new(record)))
}

/// `http::serve(addr, handler)`
/// Starts an HTTP microservice server in a background thread.
/// Returns a server handle Record with `addr`, `port`, and `stop()`.
pub fn native_http_serve(interp: &mut Interpreter, args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`http::serve(addr, handler)` requires an address and a request handler function",
        ));
    }

    let addr_str = args[0].as_str().ok_or_else(|| {
        Diagnostic::compute_error("C0202", "`http::serve` address must be a string (e.g. \"127.0.0.1:8080\")")
    })?;
    let handler = args[1].clone();

    let listener = TcpListener::bind(addr_str).map_err(|e| {
        Diagnostic::compute_error("C0301", format!("Failed to bind HTTP server to `{addr_str}`: {e}"))
    })?;

    let local_addr = listener.local_addr().map_err(|e| {
        Diagnostic::compute_error("C0301", format!("Failed to retrieve local address: {e}"))
    })?;
    let bound_ip_port = local_addr.to_string();
    let bound_port = local_addr.port() as i64;

    let _ = listener.set_nonblocking(false);

    let is_running = Arc::new(AtomicBool::new(true));
    let running_thread = is_running.clone();

    // Register server in global registry for clean stop
    if let Ok(mut reg) = SERVER_REGISTRY.lock() {
        reg.insert(bound_port, (is_running.clone(), bound_ip_port.clone()));
    }

    // Spawn server background worker thread
    let _ = interp; // interpreter context captured if needed
    thread::spawn(move || {
        while running_thread.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _client_addr)) => {
                    if !running_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                    let mut buffer = [0u8; 4096];
                    let n = stream.read(&mut buffer).unwrap_or(0);
                    if n == 0 {
                        continue;
                    }

                    let raw_req = String::from_utf8_lossy(&buffer[..n]);
                    let (method, path, body) = parse_http_request(&raw_req);

                    let mut req_record = BTreeMap::new();
                    req_record.insert("method".to_string(), Value::String(method));
                    req_record.insert("path".to_string(), Value::String(path));
                    req_record.insert("body".to_string(), Value::String(body));

                    // Execute user handler in a fresh interpreter instance with prelude
                    let mut req_interp = Interpreter::new();
                    let res_val = req_interp.call_value(handler.clone(), vec![Value::Record(Arc::new(req_record))]);

                    let (status, content_type, resp_body) = match res_val {
                        Ok(Value::Record(map)) => {
                            let s = map.get("status").and_then(|v| v.as_i64()).unwrap_or(200);
                            let ct = map.get("content_type").and_then(|v| v.as_str()).unwrap_or("application/json").to_string();
                            let b = match map.get("body") {
                                Some(Value::String(s)) => s.clone(),
                                Some(v) => v.to_string(),
                                None => String::new(),
                            };
                            (s, ct, b)
                        }
                        Ok(Value::String(s)) => (200, "text/plain".to_string(), s),
                        Ok(val) => (200, "text/plain".to_string(), val.to_string()),
                        Err(err) => (500, "text/plain".to_string(), format!("Internal Error: {}", err.message)),
                    };

                    let response = format!(
                        "HTTP/1.1 {} OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        status,
                        content_type,
                        resp_body.len(),
                        resp_body
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
                Err(_) => {
                    if !running_thread.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }
        }
    });

    let mut record = BTreeMap::new();
    record.insert("addr".to_string(), Value::String(bound_ip_port));
    record.insert("port".to_string(), Value::I64(bound_port));
    record.insert("stop".to_string(), Value::NativeFn(native_http_stop));

    Ok(Value::Record(Arc::new(record)))
}

/// `http::get(url)`: Performs an HTTP GET request and returns `{ status, body, ok }`.
pub fn native_http_get(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let url = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`http::get(url)` requires a URL string")
    })?;

    let (host, port, path) = parse_url(url)?;
    let addr = format!("{}:{}", host, port);

    let mut stream = TcpStream::connect_timeout(
        &addr.to_socket_addrs().map_err(|e| Diagnostic::compute_error("C0302", format!("DNS lookup failed for `{host}`: {e}")))?
            .next().ok_or_else(|| Diagnostic::compute_error("C0302", "Could not resolve address"))?,
        Duration::from_secs(5),
    ).map_err(|e| Diagnostic::compute_error("C0302", format!("Failed to connect to `{addr}`: {e}")))?;

    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nUser-Agent: GHL/0.1\r\n\r\n",
        path, host
    );
    stream.write_all(request.as_bytes()).map_err(|e| {
        Diagnostic::compute_error("C0302", format!("Failed to write request: {e}"))
    })?;

    let mut resp_bytes = Vec::new();
    let _ = stream.read_to_end(&mut resp_bytes);
    let resp_str = String::from_utf8_lossy(&resp_bytes);

    let (status, body) = parse_http_response(&resp_str);
    let ok = (200..300).contains(&status);

    let mut record = BTreeMap::new();
    record.insert("status".to_string(), Value::I64(status));
    record.insert("body".to_string(), Value::String(body));
    record.insert("ok".to_string(), Value::Bool(ok));

    Ok(Value::Record(Arc::new(record)))
}

/// `http::post(url, body, [content_type])`: Performs an HTTP POST request.
pub fn native_http_post(args: Vec<Value>) -> Result<Value, Diagnostic> {
    if args.len() < 2 {
        return Err(Diagnostic::compute_error(
            "C0201",
            "`http::post(url, body, [content_type])` requires a URL and body string",
        ));
    }

    let url = args[0].as_str().ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`http::post` requires URL as first argument")
    })?;
    let body = match &args[1] {
        Value::String(s) => s.clone(),
        v => v.to_string(),
    };
    let content_type = args
        .get(2)
        .and_then(|v| v.as_str())
        .unwrap_or("application/json");

    let (host, port, path) = parse_url(url)?;
    let addr = format!("{}:{}", host, port);

    let mut stream = TcpStream::connect_timeout(
        &addr.to_socket_addrs().map_err(|e| Diagnostic::compute_error("C0302", format!("DNS lookup failed for `{host}`: {e}")))?
            .next().ok_or_else(|| Diagnostic::compute_error("C0302", "Could not resolve address"))?,
        Duration::from_secs(5),
    ).map_err(|e| Diagnostic::compute_error("C0302", format!("Failed to connect to `{addr}`: {e}")))?;

    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

    let request = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nUser-Agent: GHL/0.1\r\n\r\n{}",
        path, host, content_type, body.len(), body
    );
    stream.write_all(request.as_bytes()).map_err(|e| {
        Diagnostic::compute_error("C0302", format!("Failed to write request: {e}"))
    })?;

    let mut resp_bytes = Vec::new();
    let _ = stream.read_to_end(&mut resp_bytes);
    let resp_str = String::from_utf8_lossy(&resp_bytes);

    let (status, resp_body) = parse_http_response(&resp_str);
    let ok = (200..300).contains(&status);

    let mut record = BTreeMap::new();
    record.insert("status".to_string(), Value::I64(status));
    record.insert("body".to_string(), Value::String(resp_body));
    record.insert("ok".to_string(), Value::Bool(ok));

    Ok(Value::Record(Arc::new(record)))
}

/// `net::tcp_connect(addr)`: Basic TCP connection check/ping.
pub fn native_tcp_connect(args: Vec<Value>) -> Result<Value, Diagnostic> {
    let addr = args.first().and_then(|v| v.as_str()).ok_or_else(|| {
        Diagnostic::compute_error("C0201", "`net::tcp_connect(addr)` requires an address string")
    })?;

    match TcpStream::connect_timeout(
        &addr.to_socket_addrs().map_err(|e| Diagnostic::compute_error("C0302", format!("Invalid address `{addr}`: {e}")))?
            .next().ok_or_else(|| Diagnostic::compute_error("C0302", "Could not resolve address"))?,
        Duration::from_secs(2),
    ) {
        Ok(_) => Ok(Value::Bool(true)),
        Err(_) => Ok(Value::Bool(false)),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_url(url: &str) -> Result<(String, u16, String), Diagnostic> {
    let without_scheme = if let Some(stripped) = url.strip_prefix("http://") {
        stripped
    } else {
        url
    };

    let (host_port, path) = match without_scheme.find('/') {
        Some(idx) => (&without_scheme[..idx], without_scheme[idx..].to_string()),
        None => (without_scheme, "/".to_string()),
    };

    let (host, port) = match host_port.find(':') {
        Some(idx) => {
            let h = &host_port[..idx];
            let p = host_port[idx + 1..].parse::<u16>().map_err(|_| {
                Diagnostic::compute_error("C0302", format!("Invalid port in URL `{url}`"))
            })?;
            (h.to_string(), p)
        }
        None => (host_port.to_string(), 80),
    };

    Ok((host, port, path))
}

fn parse_http_request(raw: &str) -> (String, String, String) {
    let mut lines = raw.lines();
    let req_line = lines.next().unwrap_or_default();
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let body = match raw.find("\r\n\r\n") {
        Some(idx) => raw[idx + 4..].to_string(),
        None => match raw.find("\n\n") {
            Some(idx) => raw[idx + 2..].to_string(),
            None => String::new(),
        },
    };

    (method, path, body)
}

fn parse_http_response(raw: &str) -> (i64, String) {
    let mut lines = raw.lines();
    let status_line = lines.next().unwrap_or_default();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    let body = match raw.find("\r\n\r\n") {
        Some(idx) => raw[idx + 4..].to_string(),
        None => match raw.find("\n\n") {
            Some(idx) => raw[idx + 2..].to_string(),
            None => String::new(),
        },
    };

    (status, body)
}

