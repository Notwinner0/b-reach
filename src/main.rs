use mmap_io::{MemoryMappedFile, ChangeEvent};
use std::{fs, path::PathBuf, sync::{Arc, RwLock}, io, error::Error};
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

// Read the file contents initially
fn read_initial(path: &PathBuf) -> io::Result<Vec<u8>> {
    fs::read(path)
}

#[derive(Clone)]
struct Page {
    /// Shared, up-to-date contents of the breach file
    content: Arc<RwLock<Vec<u8>>>,
}

impl HttpService for Page {
    fn call(&mut self, _req: Request, res: &mut Response) -> io::Result<()> {
        // Read the current bytes and return them as UTF-8 (lossy) text.
        // If you prefer to return raw bytes, adapt to your response API.
        let data = self.content.read().unwrap();
        let body_string = String::from_utf8_lossy(&*data).into_owned();
        res.header("Content-Type: text/plain; charset=utf-8");
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
    let initial = read_initial(&breach_path)?;
    let content = Arc::new(RwLock::new(initial));

    // Open a memory-mapped file (read-only) so we can watch it
    let mmap = MemoryMappedFile::open_ro(breach_path.clone())?;

    // Keep the watch handle alive for the lifetime of the program so the watch remains active.
    // The closure updates the shared [`content`] when the file changes.
    let content_for_watch = content.clone();
    let path_for_watch = breach_path.clone();

    let _watch_handle = mmap.watch(move |_event: ChangeEvent| {
        match fs::read(&path_for_watch) {
            Ok(new_bytes) => {
                let mut w = content_for_watch.write().unwrap();
                *w = new_bytes;
                println!("Breach file updated and content refreshed.");
            }
            Err(e) => eprintln!("Failed to read updated breach file: {}", e),
        }
    })?;

    // Start HTTP server that serves the current content
    let page = Page { content: content.clone() };
    let server = HttpServer(page).start("0.0.0.0:8080")?;

    println!("Server running on http://0.0.0.0:8080 serving {:?}", breach_path);

    // Block here; watch handle and mmap stay alive until program exit
    if let Err(e) = server.join() {
        eprintln!("Server error: {:?}", e);
    }

    Ok(())
}
