use fxhash::FxHasher64;
use std::hash::Hasher;
use std::{error::Error, fs, path::PathBuf};

/// Represents the parsed content sections from a .breach file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedContent {
    /// The markup section content (e.g., HTML, Pug, HAML), if present.
    pub markup: Option<String>,
    /// The styling section content with embedded type markers, if present.
    pub styling: Option<String>,
    /// The script section content (e.g., JavaScript, TypeScript, CoffeeScript), if present.
    pub script: Option<String>,
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

/// Checks if a line starts with a section marker (¦) followed by the given name.
pub fn starts_with_section_marker(line: &str, name: &str) -> bool {
    let line = line.trim_start();
    let mut chars = line.chars();
    let first = match chars.next() {
        Some(c) if c == '¦' => c, // accepts only the special one which is extremely rarely used in any kind of text
        _ => return false,
    };
    let rest = &line[first.len_utf8()..];
    let ident: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
    ident.eq_ignore_ascii_case(name)
}

/// Parses the content of a .breach file into structured sections using generic content types.
pub fn parse_breach_content(content: &str) -> ParsedContent {
    let mut markup_lines = Vec::new();
    let mut css_styling_lines = Vec::new();
    let mut scss_styling_lines = Vec::new();
    let mut script_lines = Vec::new();

    #[derive(Copy, Clone, PartialEq, Eq)]
    enum SectionType {
        None,
        Markup,
        CssStyling,
        ScssStyling,
        Script,
    }
    let mut cur = SectionType::None;

    let normalized = normalize_newlines(content.trim_start_matches('\u{feff}'));

    for line in normalized.lines() {
        if starts_with_section_marker(line, "html") {
            cur = SectionType::Markup;
            continue;
        }
        if starts_with_section_marker(line, "css") {
            cur = SectionType::CssStyling;
            continue;
        }
        if starts_with_section_marker(line, "scss") {
            cur = SectionType::ScssStyling;
            continue;
        }
        if starts_with_section_marker(line, "js") || starts_with_section_marker(line, "ts") || starts_with_section_marker(line, "typescript")
        {
            cur = SectionType::Script;
            continue;
        }
        match cur {
            SectionType::Markup => markup_lines.push(line),
            SectionType::CssStyling => css_styling_lines.push(line),
            SectionType::ScssStyling => scss_styling_lines.push(line),
            SectionType::Script => script_lines.push(line),
            SectionType::None => {}
        }
    }

    let markup = markup_lines.join("\n");
    let css_styling = css_styling_lines.join("\n");
    let scss_styling = scss_styling_lines.join("\n");
    let script = script_lines.join("\n");

    // Combine styling sections with markers
    let mut styling_sections = Vec::new();
    if !css_styling.trim().is_empty() {
        styling_sections.push(format!("/* CSS */\n{}\n/* EOF */", css_styling));
    }
    if !scss_styling.trim().is_empty() {
        styling_sections.push(format!("/* SCSS */\n{}\n/* EOF */", scss_styling));
    }

    let combined_styling = if styling_sections.is_empty() {
        None
    } else {
        Some(styling_sections.join("\n\n"))
    };

    let parsed_content = ParsedContent {
        markup: if markup.trim().is_empty() {
            None
        } else {
            Some(markup)
        },
        styling: combined_styling,
        script: if script.trim().is_empty() {
            None
        } else {
            Some(script)
        },
    };

    tracing::info!("ParsedContent: Markup present: {}, Styling present: {}, Script present: {}",
        parsed_content.markup.is_some(),
        parsed_content.styling.is_some(),
        parsed_content.script.is_some()
    );

    parsed_content
}

/// Compiles SCSS content to CSS using the grass compiler.
/// Returns the compiled CSS or an error if compilation fails.
pub fn compile_scss_to_css(scss_content: &str) -> Result<String, Box<dyn std::error::Error>> {
    let options = grass::Options::default();
    match grass::from_string(scss_content.to_string(), &options) {
        Ok(css) => {
            tracing::info!("SCSS compilation successful, {} bytes -> {} bytes",
                scss_content.len(), css.len());
            Ok(css)
        }
        Err(e) => {
            tracing::error!("SCSS compilation failed: {}", e);
            Err(e.into())
        }
    }
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
/// Also injects livereload WebSocket script.
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

    // Inject livereload WebSocket script
    let livereload_script = r#"<script>
(function() {
    console.log('B-REACH: Initializing live reload...');
    var ws = new WebSocket('ws://' + window.location.host + '/ws');
    console.log('B-REACH: Attempting to connect to WebSocket at:', 'ws://' + window.location.host + '/ws');

    ws.onopen = function(event) {
        console.log('B-REACH: Live reload WebSocket connection established');
    };

    ws.onmessage = function(event) {
        console.log('B-REACH: Received WebSocket message:', event.data);
        if (event.data === 'reload') {
            console.log('B-REACH: Reload signal received, refreshing page...');
            window.location.reload();
        } else {
            console.log('B-REACH: Unknown message received:', event.data);
        }
    };

    ws.onclose = function(event) {
        console.log('B-REACH: Live reload WebSocket connection closed', {
            code: event.code,
            reason: event.reason,
            wasClean: event.wasClean
        });
    };

    ws.onerror = function(error) {
        console.error('B-REACH: Live reload WebSocket connection error:', error);
        console.error('B-REACH: This may indicate the server is not running or WebSocket endpoint is unavailable');
    };

    // Log connection attempt every 5 seconds if not connected
    var connectionCheck = setInterval(function() {
        if (ws.readyState === WebSocket.CONNECTING) {
            console.log('B-REACH: Still attempting to connect to live reload WebSocket...');
        } else if (ws.readyState === WebSocket.CLOSED) {
            console.warn('B-REACH: WebSocket connection is closed, attempting to reconnect...');
            clearInterval(connectionCheck);
        } else {
            clearInterval(connectionCheck);
        }
    }, 5000);
})();
</script>"#;
    result = inject_js_script(&result, livereload_script);

    result
}





/// Processes styling content with markers and compiles as needed.
/// Returns the final CSS content.
fn process_styling_content(styling_content: &str) -> String {
    let mut final_css_sections = Vec::new();

    // Split by /* EOF */ markers to get individual sections
    let sections = styling_content.split("/* EOF */");

    for section in sections {
        let trimmed = section.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(css_content) = trimmed.strip_prefix("/* CSS */") {
            // CSS content - use as-is
            let css = css_content.trim();
            if !css.is_empty() {
                final_css_sections.push(css.to_string());
            }
        } else if let Some(scss_content) = trimmed.strip_prefix("/* SCSS */") {
            // SCSS content - compile it
            let scss = scss_content.trim();
            if !scss.is_empty() {
                match compile_scss_to_css(scss) {
                    Ok(compiled_css) => {
                        final_css_sections.push(compiled_css);
                    }
                    Err(e) => {
                        tracing::error!("Failed to compile SCSS, using original: {}", e);
                        final_css_sections.push(scss.to_string());
                    }
                }
            }
        } else {
            // Unknown marker or legacy content - treat as CSS
            if !trimmed.is_empty() {
                final_css_sections.push(trimmed.to_string());
            }
        }
    }

    final_css_sections.join("\n\n")
}

/// Prepares the parsed content for serving by compiling SCSS to CSS and injecting links.
/// Generates a fingerprint for cache busting.
pub fn prepare(parsed: ParsedContent) -> PreparedContent {
    let mut parsed = parsed;

    // Process styling content with markers
    let final_css = if let Some(styling_content) = &parsed.styling {
        let processed_css = process_styling_content(styling_content);
        if processed_css.trim().is_empty() {
            None
        } else {
            Some(processed_css)
        }
    } else {
        None
    };

    // Update parsed content with final CSS
    parsed.styling = final_css.clone();

    // Generate fingerprint including all content
    let mut hasher = FxHasher64::default();
    if let Some(m) = &parsed.markup {
        hasher.write(m.as_bytes());
    }
    if let Some(s) = &parsed.styling {
        hasher.write(s.as_bytes());
    }
    if let Some(s) = &parsed.script {
        hasher.write(s.as_bytes());
    } else {
        // Include a marker when no script is present to differentiate fingerprints
        hasher.write(b"NO_SCRIPT");
    }
    let fingerprint = hasher.finish();

    // Generate HTML with injected links
    let html_injected = parsed
        .markup
        .as_deref()
        .map(|m| inject_links_once(m, parsed.styling.is_some(), parsed.script.is_some(), fingerprint));

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
