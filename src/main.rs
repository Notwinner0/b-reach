use std::{
    error::Error,
    fs,
    io,
    path::PathBuf,
    sync::{Arc, RwLock},
    thread,
};

use fxhash::hash64;
use may_minihttp::{HttpServer, HttpService, Request, Response};
use notify::{Config, Error as NotifyError, Event, EventHandler, EventKind, RecursiveMode, Watcher, RecommendedWatcher};
use crossbeam_channel::{unbounded, Sender};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ParsedContent {
    html: Option<String>,
    js: Option<String>,
    css: Option<String>,
    ts: Option<String>, // new: hold TypeScript source if section present
}

#[derive(Clone, Debug)]
struct PreparedContent {
    /// Original parsed parts
    parsed: ParsedContent,
    /// HTML with <link>/<script> injected once, not per request
    html_injected: Option<String>,
    /// Lightweight fingerprint to avoid noisy reloads
    fingerprint: u64,
}

impl Default for PreparedContent {
    fn default() -> Self {
        Self {
            parsed: ParsedContent::default(),
            html_injected: None,
            fingerprint: 0,
        }
    }
}

fn normalize_newlines(s: &str) -> String {
    // Preserve UTF-8; normalize CRLF and CR to LF.
    // Single-pass character processing for better performance than multiple replacements.
    // Handles multi-byte UTF-8 characters correctly through char iteration.
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\r' {
            // Check if next character is \n (CRLF case)
            if chars.as_str().starts_with('\n') {
                chars.next(); // consume the \n
            }
            result.push('\n');
        } else {
            result.push(c);
        }
    }
    result
}

fn starts_with_section_marker(line: &str, name: &str) -> bool {
    // Accept either the Unicode broken-bar '¦' (U+00A6) or the ASCII pipe '|' as a marker.
    // Trim leading whitespace first.
    let line = line.trim_start();
    let mut chars = line.chars();
    let first = match chars.next() {
        Some(c) if c == '¦' || c == '|' => c,
        _ => return false,
    };
    let rest = &line[first.len_utf8()..];
    // collect alphanumeric identifier and compare case-insensitively
    let ident: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
    ident.eq_ignore_ascii_case(name)
}

fn parse_breach_content(content: &str) -> ParsedContent {
    let mut html_lines = Vec::new();
    let mut js_lines = Vec::new();
    let mut css_lines = Vec::new();
    let mut ts_lines = Vec::new();

    #[derive(Copy, Clone, PartialEq, Eq)]
    enum Section {
        None,
        Html,
        Js,
        Css,
        Ts,
    }
    let mut cur = Section::None;

    let normalized = normalize_newlines(content.trim_start_matches('\u{feff}'));

    for line in normalized.lines() {
        if starts_with_section_marker(line, "html") {
            cur = Section::Html;
            continue;
        }
        if starts_with_section_marker(line, "js") {
            cur = Section::Js;
            continue;
        }
        if starts_with_section_marker(line, "css") {
            cur = Section::Css;
            continue;
        }
        if starts_with_section_marker(line, "ts") || starts_with_section_marker(line, "typescript") {
            cur = Section::Ts;
            continue;
        }
        match cur {
            Section::Html => html_lines.push(line),
            Section::Js => js_lines.push(line),
            Section::Css => css_lines.push(line),
            Section::Ts => ts_lines.push(line),
            Section::None => { /* ignore preamble */ }
        }
    }

    let html = html_lines.join("\n");
    let js = js_lines.join("\n");
    let css = css_lines.join("\n");
    let ts = ts_lines.join("\n");

    ParsedContent {
        html: if html.trim().is_empty() { None } else { Some(html) },
        js: if js.trim().is_empty() { None } else { Some(js) },
        css: if css.trim().is_empty() { None } else { Some(css) },
        ts: if ts.trim().is_empty() { None } else { Some(ts) },
    }
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    'outer: for i in 0..=h.len() - n.len() {
        for j in 0..n.len() {
            let a = h[i + j];
            let b = n[j];
            let a = if a.is_ascii_lowercase() { a - 32 } else { a };
            let b = if b.is_ascii_lowercase() { b - 32 } else { b };
            if a != b {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

// FIX: Add a fingerprint to the injected links to bust the browser cache.
fn inject_links_once(html: &str, has_css: bool, has_js: bool, fingerprint: u64) -> String {
    let mut result = html.to_string();

    // Find title if present
    let title_start = find_case_insensitive(html, "<title>");
    let title_end = if title_start.is_some() { find_case_insensitive(html, "</title>") } else { None };
    let title_content = if let (Some(ts), Some(te)) = (title_start, title_end) {
        Some(html[ts + "<title>".len()..te].trim().to_string())
    } else {
        None
    };

    // Remove title if present (will be reinserted in head if creating new head)
    let mut removed_title = None;
    if let (Some(ts), Some(te)) = (title_start, title_end) {
        removed_title = title_content.clone();
        result.replace_range(ts..=te + "</title>".len() - 1, "");
    }

    if has_css {
        // Use a cache-busting query parameter derived from the fingerprint.
        let link_tag = format!(r#"<link rel="stylesheet" href="/style.css?v={}">"#, fingerprint);
        if let Some(head_end) = find_case_insensitive(&result, "</head>") {
            result.insert_str(head_end, &format!("\n    {}", link_tag));
        } else if let Some(html_start) = find_case_insensitive(&result, "<head>") {
            let insert_at = html_start + "<head>".len();
            result.insert_str(insert_at, &format!("\n    {}", link_tag));
        } else if let Some(html_open) = find_case_insensitive(&result, "<html>") {
            let insert_at = html_open + "<html>".len();
            let head_content = if let Some(ref tc) = removed_title {
                format!("<head>\n    {}\n    <title>{}</title>\n</head>", link_tag, tc)
            } else {
                format!("<head>\n    {}\n</head>", link_tag)
            };
            result.insert_str(insert_at, &format!("\n{}", head_content));
        } else {
            let head_content = if let Some(ref tc) = removed_title {
                format!("<head>\n    <meta charset=\"utf-8\">\n    {}\n    <title>{}</title>\n</head>\n{}", link_tag, tc, result)
            } else {
                format!("<head>\n    <meta charset=\"utf-8\">\n    {}\n</head>\n{}", link_tag, result)
            };
            result = head_content;
        }
    }

    if has_js {
        // Use a cache-busting query parameter.
        let script_tag = format!(r#"<script src="/script.js?v={}"></script>"#, fingerprint);
        if let Some(body_end) = find_case_insensitive(&result, "</body>") {
            result.insert_str(body_end, &format!("\n    {}", script_tag));
        } else {
            result.push_str(&format!("\n{}", script_tag));
        }
    }

    result
}

fn fingerprint_of(s: &str) -> u64 {
    hash64(s.as_bytes())
}

// Optional compilation: compile TS -> JS using Oxc when available
#[cfg(feature = "oxc_transformer")]
fn compile_typescript_with_oxc(filename: &str, ts: &str) -> Result<String, String> {
    // The Rust oxc API evolves; this code assumes the "oxc" umbrella crate with "transformer" feature.
    // It uses the transformer entry point to transpile TS -> JS. If the function names/args change,
    // update this block to match the oxc crate version in your Cargo.toml.

    use oxc_transformer::{TransformerOptions, transform};

    let opts = TransformerOptions::default();
    // Example: set options to transpile TypeScript -> ES2020 (or esnext)
    // opts.target = oxc_transformer::ESTarget::Es2020;

    match transform(filename, ts, &opts) {
        Ok(result) => {
            // `result` should contain transformed code; adapt this depending on the exact API you have.
            // We try to return `result.code` or `result` directly.
            // The exact field names differ across versions; adjust as necessary.
            if let Some(code) = result.code() {
                Ok(code.to_string())
            } else {
                // try debug string fallback
                Ok(format!("{}", result))
            }
        }
        Err(e) => Err(format!("oxc transform error: {:?}", e)),
    }
}

#[cfg(not(feature = "oxc_transformer"))]
fn compile_typescript_with_oxc(_filename: &str, ts: &str) -> Result<String, String> {
    // When the feature is not enabled just return the TypeScript source as-is
    // (the server will still serve it as JS; browsers will error). This keeps code compiling
    // without the optional dependency.
    Ok(ts.to_string())
}

// Small minify hook (no-op by default). You can replace this with oxc minifier call.
fn minify_js(js: &str) -> String {
    // Very small whitespace trim/minify to keep things simple
    let out = js.lines().map(|l| l.trim()).collect::<Vec<_>>().join("\n");
    out
}

fn prepare(parsed: ParsedContent) -> PreparedContent {
    // If there's TS present, transpile it to JS and place into parsed.js
    let mut parsed = parsed.clone();

    if parsed.js.is_none() && parsed.ts.is_some() {
        if let Some(ref ts_src) = parsed.ts {
            match compile_typescript_with_oxc("inline.ts", ts_src) {
                Ok(transpiled) => {
                    parsed.js = Some(minify_js(&transpiled));
                }
                Err(e) => {
                    eprintln!("TypeScript -> JS transpile failed: {}\nServing raw TypeScript as JS fallback.", e);
                    // fallback: serve original TS (not ideal for browsers)
                    parsed.js = Some(ts_src.clone());
                }
            }
        }
    } else if let Some(ref js_src) = parsed.js {
        // Minify JS by default (lightweight)
        parsed.js = Some(minify_js(js_src));
    }

    let mut to_fingerprint = String::new();
    if let Some(h) = &parsed.html {
        to_fingerprint.push_str(h);
    }
    if let Some(c) = &parsed.css {
        to_fingerprint.push_str(c);
    }
    if let Some(j) = &parsed.js {
        to_fingerprint.push_str(j);
    }

    let fingerprint = fingerprint_of(&to_fingerprint);

    let html_injected = parsed
        .html
        .as_deref()
        // FIX: Pass the fingerprint to the injection function
        .map(|h| inject_links_once(h, parsed.css.is_some(), parsed.js.is_some(), fingerprint));

    PreparedContent {
        fingerprint,
        parsed,
        html_injected,
    }
}

// FIX: Simplify file reading. `fs::read` is sufficient given the RwLock.
fn load_prepared_from_file(path: &PathBuf) -> Result<PreparedContent, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let s = String::from_utf8_lossy(&bytes).to_string();
    let parsed = parse_breach_content(&s);
    Ok(prepare(parsed))
}

#[derive(Clone)]
struct Page {
    content: Arc<RwLock<PreparedContent>>,
}

impl HttpService for Page {
    fn call(&mut self, req: Request, res: &mut Response) -> io::Result<()> {
        let path = req.path();
        // Strip query parameters to handle cache-busting URLs
        let path = path.split('?').next().unwrap_or(path);

        // Read the prepared content without cloning to avoid race conditions
        let prepared = self.content.read().unwrap_or_else(|poisoned| poisoned.into_inner());

        // FIX: The path matching is already correct. The cache-busting parameter
        // is ignored by `req.path()` which returns the base path, which is what we want.
        match path {
            "/" | "/index.html" => {
                if let Some(ref html) = prepared.html_injected {
                    res.header("Content-Type: text/html; charset=utf-8");
                    res.body_vec(html.as_bytes().to_vec());
                } else {
                    res.status_code(404, "Not Found");
                    res.body_vec(b"No HTML content found".to_vec());
                }
                res.header("Cache-Control: no-cache, no-store, must-revalidate, max-age=0");
                res.header("Pragma: no-cache");
                res.header("Expires: 0");
            }
            "/style.css" => {
                if let Some(ref css) = prepared.parsed.css {
                    res.header("Content-Type: text/css; charset=utf-8");
                    res.body_vec(css.as_bytes().to_vec());
                } else {
                    res.status_code(404, "Not Found");
                    res.body_vec(b"CSS not found".to_vec());
                }
                res.header("Cache-Control: public, max-age=31536000");
            }
            "/script.js" => {
                if let Some(ref js) = prepared.parsed.js {
                    res.header("Content-Type: application/javascript; charset=utf-8");
                    res.body_vec(js.as_bytes().to_vec());
                } else {
                    res.status_code(404, "Not Found");
                    res.body_vec(b"JavaScript not found".to_vec());
                }
                res.header("Cache-Control: public, max-age=31536000");
            }
            "/favicon.ico" => {
                res.status_code(204, "No Content");
                res.body_vec(Vec::new());
                res.header("Cache-Control: public, max-age=31536000");
            }
            _ => {
                res.status_code(404, "Not Found");
                res.body_vec(b"Page not found".to_vec());
                res.header("Cache-Control: no-cache, no-store, must-revalidate, max-age=0");
                res.header("Pragma: no-cache");
                res.header("Expires: 0");
            }
        }

        Ok(())
    }
}

struct EventForwarder {
    tx: Sender<Event>,
}

impl EventHandler for EventForwarder {
    fn handle_event(&mut self, event: Result<Event, NotifyError>) {
        if let Ok(event) = event {
            let _ = self.tx.send(event);
        }
    }
}

// Find the first `.breach` file in the current directory
fn get_breach() -> io::Result<Option<PathBuf>> {
    let paths = fs::read_dir("./")?;
    for entry in paths {
        let path = entry?.path();
        if path.is_file() && path.extension().map(|e| e == "breach").unwrap_or(false) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn main() -> Result<(), Box<dyn Error>> {
    let breach_path = match get_breach()? {
        Some(p) => p,
        None => {
            eprintln!("No .breach file found in the current directory.");
            return Ok(());
        }
    };

    let prepared = load_prepared_from_file(&breach_path)?;
    let content = Arc::new(RwLock::new(prepared));

    {
        let content_for_watch = Arc::clone(&content);
        let path_for_watch = breach_path.clone();

        thread::spawn(move || {
            let mut last_fingerprint: u64 = {
                let c = content_for_watch.read().ok();
                c.map(|g| g.fingerprint).unwrap_or(0)
            };

            let (tx, rx) = unbounded();

            let forwarder = EventForwarder { tx };

            let config = Config::default();

            let mut watcher = match RecommendedWatcher::new(forwarder, config) {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("Failed to create file watcher: {}", e);
                    return;
                }
            };

            if let Err(e) = watcher.watch(&path_for_watch, RecursiveMode::NonRecursive) {
                eprintln!("Failed to watch file: {}", e);
                return;
            }

            while let Ok(event) = rx.recv() {
                if let EventKind::Modify(_) = event.kind {
                    if event.paths.contains(&path_for_watch) {
                        match load_prepared_from_file(&path_for_watch) {
                            Ok(new_prepared) => {
                                let new_fingerprint = new_prepared.fingerprint;
                                if new_fingerprint != last_fingerprint {
                                    // Use a block to ensure the write lock is released immediately after assignment
                                    {
                                        let mut guard = content_for_watch.write().unwrap_or_else(|poisoned| poisoned.into_inner());
                                        *guard = new_prepared;
                                    }
                                    last_fingerprint = new_fingerprint;
                                    println!("Breach file updated and content refreshed.");
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to load updated breach file: {}", e);
                            }
                        }
                    }
                }
            }
        });
    }

    let page = Page {
        content: Arc::clone(&content),
    };
    let server = HttpServer(page).start("0.0.0.0:8080")?;

    println!(
        "Server running on http://0.0.0.0:8080 serving {:?}",
        breach_path
    );
    println!("Edit the .breach file while the server is running (live reload).");

    if let Err(e) = server.join() {
        eprintln!("Server error: {:?}", e);
    }

    Ok(())
}
