use crate::parser;
use arc_swap::ArcSwap;
use ntex::web::{self, HttpResponse};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub content: Arc<ArcSwap<parser::PreparedContent>>,
}

pub async fn index(data: web::types::State<AppState>) -> HttpResponse {
    let prepared = data.content.load();
    match prepared.html_injected.as_deref() {
        Some(content) => {
            tracing::info!("Serving content for path: /, MIME: text/html; charset=utf-8. Content length: {}", content.len());
            HttpResponse::Ok()
                .content_type("text/html; charset=utf-8")
                .header("Cache-Control", "no-cache")
                .header("X-Content-Type-Options", "nosniff")
                .header("Accept-Ranges", "bytes")
                .body(content.to_string())
        }
        None => {
            tracing::warn!("Resource not found for path: /, MIME: text/html; charset=utf-8. Data was None.");
            HttpResponse::NotFound()
                .content_type("text/plain")
                .header("Cache-Control", "no-cache")
                .header("X-Content-Type-Options", "nosniff")
                .body("Resource not found")
        }
    }
}

pub async fn index_html(data: web::types::State<AppState>) -> HttpResponse {
    index(data).await
}

pub async fn style_css(data: web::types::State<AppState>) -> HttpResponse {
    let prepared = data.content.load();
    let css_option = prepared.parsed.css.clone();
    tracing::info!("Request for /style.css. CSS content present: {}", css_option.is_some());

    match css_option.as_deref() {
        Some(content) => {
            tracing::info!("Serving content for path: /style.css, MIME: text/css; charset=utf-8. Content length: {}", content.len());
            HttpResponse::Ok()
                .content_type("text/css; charset=utf-8")
                .header("Cache-Control", "no-cache")
                .header("X-Content-Type-Options", "nosniff")
                .header("Accept-Ranges", "bytes")
                .body(content.to_string())
        }
        None => {
            tracing::warn!("Resource not found for path: /style.css, MIME: text/css; charset=utf-8. Data was None.");
            HttpResponse::NotFound()
                .content_type("text/plain")
                .header("Cache-Control", "no-cache")
                .header("X-Content-Type-Options", "nosniff")
                .body("Resource not found")
        }
    }
}

pub async fn script_js(data: web::types::State<AppState>) -> HttpResponse {
    let prepared = data.content.load();
    match prepared.parsed.js.as_deref() {
        Some(content) => {
            tracing::info!("Serving content for path: /script.js, MIME: application/javascript; charset=utf-8. Content length: {}", content.len());
            HttpResponse::Ok()
                .content_type("application/javascript; charset=utf-8")
                .header("Cache-Control", "no-cache")
                .header("X-Content-Type-Options", "nosniff")
                .header("Accept-Ranges", "bytes")
                .body(content.to_string())
        }
        None => {
            tracing::warn!("Resource not found for path: /script.js, MIME: application/javascript; charset=utf-8. Data was None.");
            HttpResponse::NotFound()
                .content_type("text/plain")
                .header("Cache-Control", "no-cache")
                .header("X-Content-Type-Options", "nosniff")
                .body("Resource not found")
        }
    }
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
