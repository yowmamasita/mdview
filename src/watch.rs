//! Watching the open document for changes.
//!
//! Editors rarely write a file in place — most write a temporary file and rename
//! it over the original, which destroys any watch registered on the file itself.
//! So the *directory* is watched and events are filtered down to the file we
//! care about. Bursts are coalesced: saving once should reload once.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// How long to wait for a burst of filesystem events to settle.
const DEBOUNCE: Duration = Duration::from_millis(120);

/// Watches one file at a time and calls back when it changes.
pub struct FileWatcher {
    watcher: RecommendedWatcher,
    /// The file currently being watched, shared with the notify callback.
    target: Arc<Mutex<Option<PathBuf>>>,
    /// Directory registered with the watcher, so it can be swapped out.
    watched_dir: Option<PathBuf>,
}

impl FileWatcher {
    /// Create a watcher that calls `on_change` after each settled burst.
    pub fn new<F>(on_change: F) -> notify::Result<Self>
    where
        F: Fn() + Send + 'static,
    {
        let target: Arc<Mutex<Option<PathBuf>>> = Arc::new(Mutex::new(None));
        let (tx, rx) = mpsc::channel::<()>();

        thread::Builder::new()
            .name("mdview-watch".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    // Swallow the rest of the burst before reporting it.
                    loop {
                        match rx.recv_timeout(DEBOUNCE) {
                            Ok(()) => continue,
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    on_change();
                }
            })
            .expect("spawn watch thread");

        let watcher =
            RecommendedWatcher::new(handler(Arc::clone(&target), tx), notify::Config::default())?;

        Ok(Self {
            watcher,
            target,
            watched_dir: None,
        })
    }

    /// Switch to watching `path`. Passing `None` stops watching.
    pub fn watch(&mut self, path: Option<&Path>) -> notify::Result<()> {
        let path = path.map(|p| p.to_path_buf());
        let dir = path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .filter(|d| d.is_dir());

        *self.target.lock().expect("watch target") = path;

        if dir == self.watched_dir {
            return Ok(());
        }
        if let Some(old) = self.watched_dir.take() {
            // The old directory may be gone already; that is not an error here.
            let _ = self.watcher.unwatch(&old);
        }
        if let Some(new) = dir {
            self.watcher.watch(&new, RecursiveMode::NonRecursive)?;
            self.watched_dir = Some(new);
        }
        Ok(())
    }
}

/// The notify callback: forward only events touching the watched file.
fn handler(
    target: Arc<Mutex<Option<PathBuf>>>,
    tx: Sender<()>,
) -> impl Fn(notify::Result<Event>) + Send + 'static {
    move |event| {
        let Ok(event) = event else { return };
        if !matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            return;
        }

        let guard = target.lock().expect("watch target");
        let Some(watched) = guard.as_deref() else {
            return;
        };
        if event.paths.iter().any(|p| same_file(p, watched)) {
            let _ = tx.send(());
        }
    }
}

/// Compare two paths, tolerating the symlinks and `/private` prefixes that
/// platforms add to temporary directories.
fn same_file(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.file_name() == b.file_name() && a.parent() == b.parent(),
    }
}
