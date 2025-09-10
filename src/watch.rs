use crate::parser;
use arc_swap::ArcSwap;
use crossbeam_channel::{unbounded, Sender};
use notify::{
    Config, Error as NotifyError, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use std::{
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use tracing::{error, info};

pub struct EventForwarder {
    tx: Sender<Event>,
}

impl notify::EventHandler for EventForwarder {
    fn handle_event(&mut self, event: Result<Event, NotifyError>) {
        if let Ok(event) = event {
            let _ = self.tx.send(event);
        }
    }
}

pub fn watch_file(content: Arc<ArcSwap<parser::PreparedContent>>, path: PathBuf) {
    thread::spawn(move || {
        let mut last_fingerprint: u64 = content.load().fingerprint;

        let (tx, rx) = unbounded();
        let forwarder = EventForwarder { tx };

        let config = Config::default();

        let mut watcher = match RecommendedWatcher::new(forwarder, config) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&path, RecursiveMode::NonRecursive) {
            error!("Failed to watch file: {}", e);
            return;
        }

        let mut last_event_time: Option<Instant> = None;

        loop {
            crossbeam_channel::select! {
                recv(rx) -> event => {
                    if let Ok(event) = event {
                        if let EventKind::Modify(_) = event.kind {
                            if event.paths.contains(&path) {
                                last_event_time = Some(Instant::now());
                            }
                        }
                    }
                }
                default(Duration::from_millis(50)) => {
                    // Check if we have a pending event and enough time has passed
                    if let Some(event_time) = last_event_time {
                        if event_time.elapsed() >= Duration::from_millis(100) {
                            match parser::load_prepared_from_file(&path) {
                                Ok(new_prepared) => {
                                    let new_fingerprint = new_prepared.fingerprint;
                                    if new_fingerprint != last_fingerprint {
                                        content.store(Arc::new(new_prepared));
                                        last_fingerprint = new_fingerprint;
                                        info!("Breach file updated and content refreshed.");
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to load updated breach file: {}", e);
                                }
                            }
                            last_event_time = None;
                        }
                    }
                }
            }
        }
    });
}
