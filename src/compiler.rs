/// Compiles TypeScript code to JavaScript.
/// This is a fallback implementation when OXC transformer is not available.
pub fn compile_typescript_with_oxc(_filename: &str, ts: &str) -> Result<String, String> {
    Ok(ts.to_string())
}

/// Minifies JavaScript code.
/// This is a fallback implementation when OXC minifier is not available.
pub fn minify_js(js: &str) -> Result<String, String> {
    Ok(js.lines().map(|l| l.trim()).collect::<Vec<_>>().join("\n"))
}
