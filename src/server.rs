use crate::parser;
use arc_swap::ArcSwap;
use ntex::web::{self, HttpResponse};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub content: Arc<ArcSwap<parser::PreparedContent>>,
}

/// Helper function to serve content with consistent response handling
fn serve_content<F>(
    data: &web::types::State<AppState>,
    content_getter: F,
    content_type: &str,
    path: &str,
) -> HttpResponse
where
    F: Fn(&parser::PreparedContent) -> Option<&String>,
{
    let prepared = data.content.load();
    match content_getter(&prepared) {
        Some(content) => {
            tracing::info!("Serving content for path: {}, MIME: {}; charset=utf-8. Content length: {}", path, content_type, content.len());
            HttpResponse::Ok()
                .content_type(&format!("{}; charset=utf-8", content_type))
                .header("Cache-Control", "no-cache")
                .header("X-Content-Type-Options", "nosniff")
                .header("Accept-Ranges", "bytes")
                .body(content.to_string())
        }
        None => {
            tracing::warn!("Resource not found for path: {}, MIME: {}; charset=utf-8. Data was None.", path, content_type);
            HttpResponse::NotFound()
                .content_type("text/plain")
                .header("Cache-Control", "no-cache")
                .header("X-Content-Type-Options", "nosniff")
                .body("Resource not found")
        }
    }
}

pub async fn index(data: web::types::State<AppState>) -> HttpResponse {
    serve_content(&data, |p| p.html_injected.as_ref(), "text/html", "/")
}

pub async fn index_html(data: web::types::State<AppState>) -> HttpResponse {
    index(data).await
}

pub async fn style_css(data: web::types::State<AppState>) -> HttpResponse {
    let prepared = data.content.load();
    tracing::info!("Request for /style.css. CSS content present: {}", prepared.parsed.css.is_some());
    serve_content(&data, |p| p.parsed.css.as_ref(), "text/css", "/style.css")
}

pub async fn script_js(data: web::types::State<AppState>) -> HttpResponse {
    serve_content(&data, |p| p.parsed.js.as_ref(), "application/javascript", "/script.js")
}

pub async fn favicon_ico() -> HttpResponse {
    HttpResponse::NoContent()
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .header("X-Content-Type-Options", "nosniff")
        .header("Accept-Ranges", "bytes")
        .finish()
}

pub async fn not_found() -> HttpResponse {
    HttpResponse::NotFound()
        .content_type("text/plain")
        .header("Cache-Control", "no-cache")
        .header("X-Content-Type-Options", "nosniff")
        .body("Page not found")
}
