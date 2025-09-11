use fxhash::FxHasher64;
use std::hash::Hasher;
use std::{error::Error, fs, path::PathBuf};

/// Represents the parsed content sections from a .breach file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedContent {
    /// The HTML section content, if present.
    pub html: Option<String>,
    /// The JavaScript section content, if present.
    pub js: Option<String>,
    /// The CSS section content, if present.
    pub css: Option<String>,
    /// The TypeScript section content, if present.
    pub ts: Option<String>,
}

/// Represents the prepared content ready for serving, with injected links and fingerprint.
#[derive(Clone, Debug)]
pub struct PreparedContent {
    /// The original parsed content sections.
    pub parsed: ParsedContent,
    /// HTML content with CSS and JS links injected, prepared once for efficiency.
    pub html_injected: Option<String>,
    /// A hash-based fingerprint of the content for cache busting and change detection.
    pub fingerprint: u64,
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

/// Normalizes different newline styles to Unix-style newlines.
pub fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Checks if a line starts with a section marker (¦ or |) followed by the given name.
pub fn starts_with_section_marker(line: &str, name: &str) -> bool {
    let line = line.trim_start();
    let mut chars = line.chars();
    let first = match chars.next() {
        Some(c) if c == '¦' || c == '|' => c,
        _ => return false,
    };
    let rest = &line[first.len_utf8()..];
    let ident: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
    ident.eq_ignore_ascii_case(name)
}

/// Parses the content of a .breach file into structured sections (HTML, JS, CSS, TS).
pub fn parse_breach_content(content: &str) -> ParsedContent {
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
        if starts_with_section_marker(line, "ts") || starts_with_section_marker(line, "typescript")
        {
            cur = Section::Ts;
            continue;
        }
        match cur {
            Section::Html => html_lines.push(line),
            Section::Js => js_lines.push(line),
            Section::Css => css_lines.push(line),
            Section::Ts => ts_lines.push(line),
            Section::None => {}
        }
    }

    let html = html_lines.join("\n");
    let js = js_lines.join("\n");
    let css = css_lines.join("\n");
    let ts = ts_lines.join("\n");

    let parsed_content = ParsedContent {
        html: if html.trim().is_empty() {
            None
        } else {
            Some(html)
        },
        js: if js.trim().is_empty() { None } else { Some(js) },
        css: if css.trim().is_empty() {
            None
        } else {
            Some(css)
        },
        ts: if ts.trim().is_empty() { None } else { Some(ts) },
    };

    tracing::info!("ParsedContent: HTML present: {}, JS present: {}, CSS present: {}, TS present: {}",
        parsed_content.html.is_some(),
        parsed_content.js.is_some(),
        parsed_content.css.is_some(),
        parsed_content.ts.is_some()
    );

    parsed_content
}

/// Finds the case-insensitive position of a substring within a string.
pub fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let haystack_lower = haystack.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    haystack_lower.find(&needle_lower)
}

/// Extracts the title content from HTML and removes the title tags.
/// Returns the modified HTML and the extracted title content.
fn extract_and_remove_title(html: &str) -> (String, Option<String>) {
    let mut result = html.to_string();
    let title_start = find_case_insensitive(html, "<title>");
    let title_end = if title_start.is_some() {
        find_case_insensitive(html, "</title>")
    } else {
        None
    };
    let title_content = if let (Some(ts), Some(te)) = (title_start, title_end) {
        Some(html[ts + "<title>".len()..te].trim().to_string())
    } else {
        None
    };

    if let (Some(ts), Some(te)) = (title_start, title_end) {
        result.replace_range(ts..=te + "</title>".len() - 1, "");
    }

    (result, title_content)
}

/// Injects a CSS link tag into the HTML at the appropriate location.
/// Returns the modified HTML.
fn inject_css_link(html: &str, link_tag: &str, title_content: Option<&str>) -> String {
    if let Some(head_end) = find_case_insensitive(html, "</head>") {
        let mut result = html.to_string();
        result.insert_str(head_end, &format!("\n    {}", link_tag));
        result
    } else if let Some(head_start) = find_case_insensitive(html, "<head>") {
        let mut result = html.to_string();
        let insert_at = head_start + "<head>".len();
        result.insert_str(insert_at, &format!("\n    {}", link_tag));
        result
    } else if let Some(html_open) = find_case_insensitive(html, "<html>") {
        let mut result = html.to_string();
        let insert_at = html_open + "<html>".len();
        let head_content = if let Some(tc) = title_content {
            format!(
                "<head>\n    {}\n    <title>{}</title>\n</head>",
                link_tag, tc
            )
        } else {
            format!("<head>\n    {}\n</head>", link_tag)
        };
        result.insert_str(insert_at, &format!("\n{}", head_content));
        result
    } else {
        let head_content = if let Some(tc) = title_content {
            format!(
                "<head>\n    <meta charset=\"utf-8\">\n    {}\n    <title>{}</title>\n</head>\n{}",
                link_tag, tc, html
            )
        } else {
            format!(
                "<head>\n    <meta charset=\"utf-8\">\n    {}\n</head>\n{}",
                link_tag, html
            )
        };
        head_content
    }
}

/// Injects a JS script tag into the HTML at the appropriate location.
/// Returns the modified HTML.
fn inject_js_script(html: &str, script_tag: &str) -> String {
    if let Some(body_end) = find_case_insensitive(html, "</body>") {
        let mut result = html.to_string();
        result.insert_str(body_end, &format!("\n    {}", script_tag));
        result
    } else {
        format!("{}\n{}", html, script_tag)
    }
}

/// Injects CSS and JS link tags into the HTML content, handling various HTML structures.
/// Preserves the title if present and adds links in the appropriate locations.
pub fn inject_links_once(html: &str, has_css: bool, has_js: bool, fingerprint: u64) -> String {
    let (mut result, title_content) = extract_and_remove_title(html);

    if has_css {
        let link_tag = format!(
            r#"<link rel="stylesheet" href="/style.css?v={}">"#,
            fingerprint
        );
        result = inject_css_link(&result, &link_tag, title_content.as_deref());
    }

    if has_js {
        let script_tag = format!(r#"<script src="/script.js?v={}"></script>"#, fingerprint);
        result = inject_js_script(&result, &script_tag);
    }

    result
}





/// Prepares the parsed content for serving by compiling TypeScript, minifying JS, and injecting links.
/// Generates a fingerprint for cache busting.
pub fn prepare(parsed: ParsedContent) -> PreparedContent {
    let mut parsed = parsed.clone();

    if parsed.js.is_none() && parsed.ts.is_some() {
        if let Some(ref ts_src) = parsed.ts {
            match crate::compiler::compile_typescript_with_oxc("inline.ts", ts_src) {
                Ok(transpiled) => {
                    match crate::compiler::minify_js(&transpiled) {
                        Ok(minified) => parsed.js = Some(minified),
                        Err(e) => {
                            tracing::error!("JS minification failed: {}\nServing unminified JS.", e);
                            parsed.js = Some(transpiled);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("TypeScript -> JS transpile failed: {}\nServing raw TypeScript as JS fallback.", e);
                    parsed.js = Some(ts_src.clone());
                }
            }
        }
    } else if let Some(ref js_src) = parsed.js {
        match crate::compiler::minify_js(js_src) {
            Ok(minified) => parsed.js = Some(minified),
            Err(e) => {
                tracing::error!("JS minification failed: {}\nServing unminified JS.", e);
                // Keep original
            }
        }
    }

    let mut hasher = FxHasher64::default();
    if let Some(h) = &parsed.html {
        hasher.write(h.as_bytes());
    }
    if let Some(c) = &parsed.css {
        hasher.write(c.as_bytes());
    }
    if let Some(j) = &parsed.js {
        hasher.write(j.as_bytes());
    }
    let fingerprint = hasher.finish();

    let html_injected = parsed
        .html
        .as_deref()
        .map(|h| inject_links_once(h, parsed.css.is_some(), parsed.js.is_some(), fingerprint));

    PreparedContent {
        fingerprint,
        parsed,
        html_injected,
    }
}

/// Loads and prepares content from a .breach file at the given path.
pub fn load_prepared_from_file(path: &PathBuf) -> Result<PreparedContent, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    let s = String::from_utf8_lossy(&bytes).to_string();
    let parsed = parse_breach_content(&s);
    Ok(prepare(parsed))
}
