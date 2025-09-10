use std::{
    error::Error,
    fs,
    path::PathBuf,
    sync::Arc,
};

use may_minihttp::HttpServer;
use arc_swap::ArcSwap;
use tracing::{info, error};
use tracing_subscriber;
use ctrlc;

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

fn main() -> Result<(), Box<dyn Error>> {
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

    let page = server::Page {
        content: Arc::clone(&content),
    };
    let server = HttpServer(page).start("0.0.0.0:8080")?;

    info!(
        "Server running on http://0.0.0.0:8080 serving {:?}",
        breach_path
    );
    info!("Edit the .breach file while the server is running (live reload).");

    // Handle graceful shutdown
    let server_handle = server;

    ctrlc::set_handler(|| {
        info!("Received shutdown signal, stopping server...");
        std::process::exit(0);
    })?;

    if let Err(e) = server_handle.join() {
        error!("Server error: {:?}", e);
    }

    Ok(())
}
