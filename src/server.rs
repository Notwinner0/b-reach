use crate::parser;
use arc_swap::ArcSwap;
use ntex::web::{self, HttpResponse, Error};
use ntex::ws;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub content: Arc<ArcSwap<parser::PreparedContent>>,
    pub reload_tx: broadcast::Sender<()>,
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

/// WebSocket handler for live reload functionality
pub async fn ws_livereload(
    req: web::HttpRequest,
    data: web::types::State<AppState>,
) -> Result<HttpResponse, Error> {
    let reload_tx = data.reload_tx.clone();

    // Create a WebSocket service factory with reload notification support
    let factory = ntex::service::fn_factory_with_config(move |sink: ws::WsSink| {
        let reload_rx = reload_tx.subscribe();

        async move {
            // Clone sink for the reload notification task
            let sink_clone = sink.clone();

            // Spawn a background task to handle reload notifications
            ntex::rt::spawn(async move {
                let mut reload_rx = reload_rx;
                while let Ok(_) = reload_rx.recv().await {
                    tracing::info!("Sending reload notification to client");
                    if let Err(e) = sink_clone.send(ws::Message::Text("reload".into())).await {
                        tracing::error!("Failed to send reload message: {}", e);
                        break;
                    }
                }
            });

            // Create the main service that handles WebSocket frames
            let service = ntex::service::fn_service(move |frame: ws::Frame| {
                async move {
                    // Handle incoming frames
                    match frame {
                        ws::Frame::Close(_) => {
                            tracing::info!("WebSocket connection closed by client");
                            Ok::<Option<ws::Message>, std::io::Error>(None)
                        }
                        _ => {
                            // Other frames don't need responses
                            Ok::<Option<ws::Message>, std::io::Error>(None)
                        }
                    }
                }
            });

            Ok::<_, std::io::Error>(service)
        }
    });

    // Start the WebSocket service
    ntex::web::ws::start(req, factory).await
}
