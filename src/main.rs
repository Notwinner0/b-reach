use std::{
    error::Error,
    fs,
    io,
    path::PathBuf,
    sync::{Arc, RwLock},
    thread,
    time::{Duration, SystemTime},
};

use may_minihttp::{HttpServer, HttpService, Request, Response};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ParsedContent {
    html: Option<String>,
    js: Option<String>,
    css: Option<String>,
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
    // Convert CRLF/CR to LF for simpler parsing
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if matches!(chars.peek(), Some('\n')) {
                // skip, '\n' will be pushed next loop
            } else {
                out.push('\n');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn starts_with_section_marker(line: &str, name: &str) -> bool {
    // Accept variations like "¦html", "¦html ", "¦HTML", etc.
    let line = line.trim_start();
    if !line.starts_with('¦') {
        return false;
    }
    let rest = &line['¦'.len_utf8()..];
    // take leading alpha chars
    let ident: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
    ident.eq_ignore_ascii_case(name)
}

fn parse_breach_content(content: &str) -> ParsedContent {
    let mut html = String::new();
    let mut js = String::new();
    let mut css = String::new();

    #[derive(Copy, Clone, PartialEq, Eq)]
    enum Section {
        None,
        Html,
        Js,
        Css,
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
        match cur {
            Section::Html => {
                html.push_str(line);
                html.push('\n');
            }
            Section::Js => {
                js.push_str(line);
                js.push('\n');
            }
            Section::Css => {
                css.push_str(line);
                css.push('\n');
            }
            Section::None => { /* ignore preamble */ }
        }
    }

    ParsedContent {
        html: if html.trim().is_empty() { None } else { Some(html.trim().to_string()) },
        js: if js.trim().is_empty() { None } else { Some(js.trim().to_string()) },
        css: if css.trim().is_empty() { None } else { Some(css.trim().to_string()) },
    }
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    // Simple, allocation-light case-insensitive search for ASCII tag tokens
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

fn inject_links_once(html: &str, has_css: bool, has_js: bool) -> String {
    let mut result = html.to_string();

    if has_css {
        // Prefer </head>, otherwise insert right after <html> or at start
        if let Some(head_end) = find_case_insensitive(&result, "</head>") {
            let link_tag = r#"<link rel="stylesheet" href="/style.css">"#;
            result.insert_str(head_end, &format!("\n    {}", link_tag));
        } else if let Some(html_start) = find_case_insensitive(&result, "<head>") {
            let insert_at = html_start + "<head>".len();
            result.insert_str(insert_at, "\n    <link rel=\"stylesheet\" href=\"/style.css\">");
        } else if let Some(html_open) = find_case_insensitive(&result, "<html>") {
            let insert_at = html_open + "<html>".len();
            result.insert_str(insert_at, "\n<head>\n    <link rel=\"stylesheet\" href=\"/style.css\">\n</head>");
        } else {
            result = format!(
                "<head>\n    <meta charset=\"utf-8\">\n    <link rel=\"stylesheet\" href=\"/style.css\">\n</head>\n{}",
                result
            );
        }
    }

    if has_js {
        // Prefer before </body>, else append
        if let Some(body_end) = find_case_insensitive(&result, "</body>") {
            result.insert_str(body_end, "\n    <script src=\"/script.js\"></script>");
        } else {
            result.push_str("\n<script src=\"/script.js\"></script>");
        }
    }

    result
}

fn fingerprint_of(s: &str) -> u64 {
    // Simple, fast hash (fxhash-like). Good enough for change detection.
    let mut hash: u64 = 0xcbf29ce484222325;
    let prime: u64 = 0x00000100000001B3;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(prime);
    }
    hash
}

fn prepare(parsed: ParsedContent) -> PreparedContent {
    let html_injected = parsed
        .html
        .as_deref()
        .map(|h| inject_links_once(h, parsed.css.is_some(), parsed.js.is_some()));

    let mut to_fingerprint = String::new();
    if let Some(h) = &html_injected {
        to_fingerprint.push_str(h);
    }
    if let Some(c) = &parsed.css {
        to_fingerprint.push_str(c);
    }
    if let Some(j) = &parsed.js {
        to_fingerprint.push_str(j);
    }

    PreparedContent {
        fingerprint: fingerprint_of(&to_fingerprint),
        parsed,
        html_injected,
    }
}

/// Read a file "atomically": ensure metadata (len + mtime) is stable before/after the read.
/// Retry a few times if it changes mid-read (common with editors writing temp files).
fn read_file_atomically(path: &PathBuf, max_retries: usize) -> io::Result<Vec<u8>> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        let meta_before = fs::metadata(path)?;
        let len_before = meta_before.len();
        let mtime_before = meta_before.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        let bytes = fs::read(path)?;

        // Quick sanity: if zero-length while we expected data, retry once
        if len_before > 0 && bytes.is_empty() && attempts <= max_retries {
            thread::sleep(Duration::from_millis(20));
            continue;
        }

        let meta_after = fs::metadata(path)?;
        let len_after = meta_after.len();
        let mtime_after = meta_after.modified().unwrap_or(SystemTime::UNIX_EPOCH);

        if len_before == len_after && mtime_before == mtime_after {
            return Ok(bytes);
        }

        if attempts >= max_retries {
            // Return the last read; better stale-but-valid than infinite loop
            return Ok(bytes);
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn load_prepared_from_file(path: &PathBuf) -> Result<PreparedContent, Box<dyn Error>> {
    let bytes = read_file_atomically(path, 5)?;
    let s = String::from_utf8_lossy(&bytes).to_string();
    let parsed = parse_breach_content(&s);
    Ok(prepare(parsed))
}

#[derive(Clone)]
struct Page {
    content: Arc<RwLock<PreparedContent>>,
    file_path: PathBuf,
}

impl HttpService for Page {
    fn call(&mut self, req: Request, res: &mut Response) -> io::Result<()> {
        let path = req.path();
        // Keep logs useful but not noisy
        // println!("{} {}", req.method(), path);

        // Snapshot the current prepared content under a read lock
        let prepared = match self.content.read() {
            Ok(g) => g.clone(),
            Err(poisoned) => poisoned.into_inner().clone(), // recover if writer panicked
        };

        match path {
            "/" | "/index.html" => {
                if let Some(ref html) = prepared.html_injected {
                    res.header("Content-Type: text/html; charset=utf-8");
                    res.body_vec(html.as_bytes().to_vec());
                } else {
                    res.status_code(404, "Not Found");
                    res.body_vec(b"No HTML content found".to_vec());
                }
            }
            "/style.css" => {
                if let Some(ref css) = prepared.parsed.css {
                    res.header("Content-Type: text/css; charset=utf-8");
                    res.body_vec(css.as_bytes().to_vec());
                } else {
                    res.status_code(404, "Not Found");
                    res.body_vec(b"CSS not found".to_vec());
                }
            }
            "/script.js" => {
                if let Some(ref js) = prepared.parsed.js {
                    res.header("Content-Type: application/javascript; charset=utf-8");
                    res.body_vec(js.as_bytes().to_vec());
                } else {
                    res.status_code(404, "Not Found");
                    res.body_vec(b"JavaScript not found".to_vec());
                }
            }
            "/favicon.ico" => {
                // Avoid 404 spam in logs/browsers
                res.status_code(204, "No Content");
                res.body_vec(Vec::new());
            }
            _ => {
                res.status_code(404, "Not Found");
                res.body_vec(b"Page not found".to_vec());
            }
        }

        // Live dev: disable caches aggressively
        res.header("Cache-Control: no-cache, no-store, must-revalidate, max-age=0");
        res.header("Pragma: no-cache");
        res.header("Expires: 0");

        Ok(())
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
    // Locate .breach file
    let breach_path = match get_breach()? {
        Some(p) => p,
        None => {
            eprintln!("No .breach file found in the current directory.");
            return Ok(());
        }
    };

    // Initial load (atomic)
    let prepared = load_prepared_from_file(&breach_path)?;
    let content = Arc::new(RwLock::new(prepared));

    // Watcher (polling + atomic read + debounce)
    {
        let content_for_watch = Arc::clone(&content);
        let path_for_watch = breach_path.clone();

        thread::spawn(move || {
            let mut last_fingerprint: u64 = {
                let c = content_for_watch.read().ok();
                c.map(|g| g.fingerprint).unwrap_or(0)
            };

            // We still poll, but with a slightly larger interval; the atomic read protects against partial writes.
            let poll_every = Duration::from_millis(150);

            // Track last metadata to avoid unnecessary reads
            let mut last_len: Option<u64> = None;
            let mut last_mtime: Option<SystemTime> = None;

            loop {
                thread::sleep(poll_every);

                let meta = match fs::metadata(&path_for_watch) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                let len = meta.len();
                let mtime = meta.modified().ok();

                if Some(len) == last_len && mtime == last_mtime {
                    continue; // no change detected
                }

                match load_prepared_from_file(&path_for_watch) {
                    Ok(new_prepared) => {
                        if new_prepared.fingerprint != last_fingerprint {
                            if let Ok(mut guard) = content_for_watch.write() {
                                *guard = new_prepared.clone();
                            } else {
                                // If lock poisoned, attempt to recover by replacing anyway
                                if let Err(e) = content_for_watch.write().map(|mut g| *g = new_prepared.clone()) {
                                    eprintln!("Lock poisoned and recovery failed: {:?}", e);
                                }
                            }
                            last_fingerprint = new_prepared.fingerprint;
                            println!("Breach file updated and content refreshed.");
                        }
                        last_len = Some(len);
                        last_mtime = mtime;
                    }
                    Err(e) => {
                        // Don’t clobber existing content on read errors
                        eprintln!("Failed to load updated breach file: {}", e);
                    }
                }
            }
        });
    }

    // Start HTTP server that serves the current content
    let page = Page {
        content: Arc::clone(&content),
        file_path: breach_path.clone(),
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
