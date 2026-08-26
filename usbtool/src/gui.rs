//! A tiny local web GUI for the USB tool — pure Rust, no Node/Tauri/toolchain.
//! `remboot-usb gui` starts a localhost server, opens the browser, and drives
//! the same `disk`/`provision` logic the CLI uses.

use crate::disk;
use crate::provision;
use crate::util::{human, try_run};
use crate::CreateArgs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

pub fn serve(port: u16) -> Result<(), String> {
    // Fall back to any free port if the preferred one is taken.
    let listener = TcpListener::bind(("127.0.0.1", port))
        .or_else(|_| TcpListener::bind(("127.0.0.1", 0)))
        .map_err(|e| format!("cannot start the local server: {e}"))?;
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    let url = format!("http://127.0.0.1:{port}/");
    println!("RemBoot USB is running.");
    println!("If your browser didn't open, go to: {url}");
    println!("(Close this window to quit.)");
    open_browser(&url);
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if handle(s) {
                    break; // /api/quit
                }
            }
            Err(_) => continue,
        }
    }
    Ok(())
}

/// Handle one request; returns true if the server should stop.
fn handle(mut stream: TcpStream) -> bool {
    let Some(reqline) = read_request_line(&mut stream) else {
        return false;
    };
    let mut parts = reqline.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let (path, query) = target.split_once('?').unwrap_or((target, ""));

    match (method, path) {
        ("GET", "/") => send(&mut stream, 200, "text/html; charset=utf-8", PAGE),
        ("GET", "/api/disks") => send(&mut stream, 200, "application/json", &disks_json()),
        ("POST", "/api/create") => {
            let body = create(query);
            send(&mut stream, 200, "application/json", &body);
        }
        ("POST", "/api/quit") => {
            send(&mut stream, 200, "application/json", "{\"ok\":true}");
            return true;
        }
        _ => send(&mut stream, 404, "text/plain", "not found"),
    }
    false
}

fn disks_json() -> String {
    let disks = disk::list().unwrap_or_default();
    let items: Vec<String> = disks
        .iter()
        .map(|d| {
            format!(
                "{{\"id\":{},\"model\":{},\"human\":{},\"removable\":{},\"system\":{}}}",
                js(&d.id),
                js(&d.model),
                js(&human(d.size)),
                d.removable,
                d.system
            )
        })
        .collect();
    format!("{{\"disks\":[{}]}}", items.join(","))
}

fn create(query: &str) -> String {
    let q = Query::parse(query);
    let id = q.get("disk");
    if id.is_empty() {
        return err("no disk selected");
    }
    let disk = match disk::find(&id) {
        Ok(Some(d)) => d,
        Ok(None) => return err(&format!("disk '{id}' not found")),
        Err(e) => return err(&e),
    };
    let allow_internal = q.get("allow_internal") == "1";
    if disk.system {
        return err("that looks like a system disk — refusing");
    }
    if !disk.removable && !allow_internal {
        return err("disk is not removable (enable the override if you're sure)");
    }

    let efi = q.get("efi");
    let args = CreateArgs {
        disk: id.clone(),
        efi: if efi.is_empty() { PathBuf::from("dist/EFI/BOOT/BOOTX64.EFI") } else { PathBuf::from(efi) },
        isos: non_empty(q.get("isos")).map(PathBuf::from),
        config: non_empty(q.get("config")).map(PathBuf::from),
        simple: q.get("simple") == "1",
        esp_mb: q.get("esp_mb").parse().unwrap_or(512),
        yes: true,
        allow_internal,
    };
    if !args.efi.is_file() {
        return err(&format!("BOOTX64.EFI not found at {}", args.efi.display()));
    }
    match provision::create(&disk, &args) {
        Ok(()) => "{\"ok\":true,\"message\":\"Done — the USB is ready.\"}".to_string(),
        Err(e) => err(&e),
    }
}

fn err(msg: &str) -> String {
    format!("{{\"ok\":false,\"message\":{}}}", js(msg))
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

// --------------------------------------------------------------- plumbing --

fn read_request_line(stream: &mut TcpStream) -> Option<String> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 16384 {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buf);
    text.lines().next().map(|s| s.to_string())
}

fn send(stream: &mut TcpStream, status: u16, content_type: &str, body: &str) {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "OK",
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}

/// Escape a string as a JSON string literal.
fn js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Query<'a>(Vec<(&'a str, String)>);

impl<'a> Query<'a> {
    fn parse(q: &'a str) -> Self {
        let mut v = Vec::new();
        for pair in q.split('&').filter(|s| !s.is_empty()) {
            let (k, val) = pair.split_once('=').unwrap_or((pair, ""));
            v.push((k, pct_decode(val)));
        }
        Query(v)
    }
    fn get(&self, key: &str) -> String {
        self.0.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone()).unwrap_or_default()
    }
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    try_run("cmd", &["/C", "start", "", url]);
    #[cfg(target_os = "macos")]
    try_run("open", &[url]);
    #[cfg(target_os = "linux")]
    try_run("xdg-open", &[url]);
}

const PAGE: &str = include_str!("gui.html");
