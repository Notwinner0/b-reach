use std::{error::Error, fs, path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;
use ntex::web;
use tracing::{error, info};
use tracing_subscriber;

mod compiler;
mod parser;
mod server;
mod watch;

// Find the first `.breach` file in the current directory
fn get_breach() -> Result<Option<PathBuf>, Box<dyn Error>> {
    let paths = fs::read_dir("./")?;
    for entry in paths {
        let path = entry?.path();
        if path.is_file() && path.extension().map(|e| e == "breach").unwrap_or(false) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

#[ntex::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let breach_path = match get_breach()? {
        Some(p) => p,
        None => {
            error!("No .breach file found in the current directory.");
            return Ok(());
        }
    };

    let prepared = parser::load_prepared_from_file(&breach_path)?;
    let content = Arc::new(ArcSwap::from_pointee(prepared));

    // Start file watcher
    watch::watch_file(Arc::clone(&content), breach_path.clone());

    let state = server::AppState {
        content: Arc::clone(&content),
    };

    info!(
        "Server running on http://127.0.0.1:8080 serving {:?}",
        breach_path
    );
    info!("Edit the .breach file while the server is running (live reload).");

    web::server(move || {
        web::App::new()
            .state(state.clone())
            .service(
                web::resource("/")
                    .route(web::get().to(server::index))
            )
            .service(
                web::resource("/index.html")
                    .route(web::get().to(server::index_html))
            )
            .service(
                web::resource("/style.css")
                    .route(web::get().to(server::style_css))
            )
            .service(
                web::resource("/script.js")
                    .route(web::get().to(server::script_js))
            )
            .service(
                web::resource("/favicon.ico")
                    .route(web::get().to(server::favicon_ico))
            )
            .default_service(
                web::route().to(server::not_found)
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await?;

    Ok(())
}
