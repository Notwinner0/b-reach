use std::{fs, path::PathBuf, sync::{Arc, RwLock}, io, error::Error, thread, time::Duration};
use may_minihttp::{HttpServer, HttpService, Request, Response};

// Find the first `.breach` file in the current directory
fn get_breach() -> io::Result<Option<PathBuf>> {
    let paths = fs::read_dir("./")?;

    for path_result in paths {
        let path = path_result?.path();

        if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("breach")) {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

// Read the file contents on demand
fn read_file_content(path: &PathBuf) -> Result<Vec<u8>, Box<dyn Error>> {
    let content = fs::read(path)?;
    Ok(content)
}

#[derive(Clone)]
struct Page {
    /// Shared, up-to-date contents of the breach file
    content: Arc<RwLock<Vec<u8>>>,
    /// Path to the breach file for getting metadata
    file_path: PathBuf,
}

impl HttpService for Page {
    fn call(&mut self, _req: Request, res: &mut Response) -> io::Result<()> {
        // Read the current bytes and return them as UTF-8 (lossy) text.
        // If you prefer to return raw bytes, adapt to your response API.
        let data = self.content.read().unwrap();
        let body_string = String::from_utf8_lossy(&*data).into_owned();

        // Get file metadata for cache headers
        let _metadata = fs::metadata(&self.file_path).ok();

        res.header("Content-Type: text/plain; charset=utf-8");

        // Aggressive cache control headers for live reload
        res.header("Cache-Control: no-cache, no-store, must-revalidate, max-age=0");
        res.header("Pragma: no-cache");
        res.header("Expires: 0");

        // Note: Dynamic headers would be added here for Last-Modified and ETag
        // but may_minihttp requires &'static str, so we're focusing on cache control for now

        res.body_vec(body_string.into_bytes());
        Ok(())
    }
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

    // Read initial contents
    let initial = read_file_content(&breach_path)?;
    let content = Arc::new(RwLock::new(initial));

    // Start file watching thread
    let content_for_watch = content.clone();
    let path_for_watch = breach_path.clone();

    thread::spawn(move || {
        let mut last_modified = match fs::metadata(&path_for_watch) {
            Ok(metadata) => metadata.modified().ok(),
            Err(_) => None,
        };
        let mut last_len = match fs::metadata(&path_for_watch) {
            Ok(metadata) => Some(metadata.len()),
            Err(_) => None,
        };

        loop {
            thread::sleep(Duration::from_millis(100)); // Poll every 100ms

            let metadata = match fs::metadata(&path_for_watch) {
                Ok(m) => m,
                Err(_) => continue, // File might be temporarily unavailable
            };

            let current_len = metadata.len();
            let current_modified = metadata.modified().ok();

            // Check for changes
            if current_modified != last_modified || Some(current_len) != last_len {
                match read_file_content(&path_for_watch) {
                    Ok(new_bytes) => {
                        let mut w = content_for_watch.write().unwrap();
                        *w = new_bytes;
                        println!("Breach file updated and content refreshed.");
                    }
                    Err(e) => eprintln!("Failed to read updated breach file: {}", e),
                }

                last_modified = current_modified;
                last_len = Some(current_len);
            }
        }
    });

    // Start HTTP server that serves the current content
    let page = Page { content: content.clone(), file_path: breach_path.clone() };
    let server = HttpServer(page).start("0.0.0.0:8080")?;

    println!("Server running on http://0.0.0.0:8080 serving {:?}", breach_path);
    println!("File can be edited while the server is running (live reload)");

    // Block here; server runs until interrupted
    if let Err(e) = server.join() {
        eprintln!("Server error: {:?}", e);
    }

    Ok(())
}
