//! The live-reload watcher.
//!
//! Only built with the `gui` feature, since that is what pulls in `notify`.
#![cfg(feature = "gui")]

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use mdview::watch::FileWatcher;

/// Long enough for a debounce plus filesystem latency, short enough to fail fast.
const SETTLE: Duration = Duration::from_secs(5);

/// A scratch directory that cleans itself up.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("mdview-watch-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        // Resolve symlinks now (macOS puts temp dirs behind `/private`), so the
        // watcher and the test agree on what the path is.
        Scratch(fs::canonicalize(&dir).expect("canonicalize"))
    }

    fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, contents).expect("write fixture");
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A counter the watcher bumps, plus a way to wait for it.
#[derive(Clone, Default)]
struct Counter(Arc<AtomicUsize>);

impl Counter {
    fn get(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    /// Wait until the count reaches `target`, returning whether it did.
    fn wait_for(&self, target: usize) -> bool {
        let deadline = Instant::now() + SETTLE;
        while Instant::now() < deadline {
            if self.get() >= target {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// Give the watcher a chance to fire, then report whether the count is
    /// still what it was.
    fn stayed_quiet_since(&self, baseline: usize) -> bool {
        std::thread::sleep(Duration::from_millis(600));
        self.get() == baseline
    }
}

/// Let a freshly registered watch settle, and return the resulting count.
///
/// macOS delivers FSEvents from a shared kernel queue, and a stream opened
/// "from now" can still replay an event that was already in flight — such as
/// the write that created the fixture a moment earlier. Waiting past that and
/// taking the count as a baseline keeps the tests about what they mean to
/// test: that a *subsequent* edit does or does not reach the watcher.
fn settle(counter: &Counter) -> usize {
    std::thread::sleep(Duration::from_millis(400));
    counter.get()
}

fn watcher_with_counter() -> (FileWatcher, Counter) {
    let counter = Counter::default();
    let bump = counter.clone();
    let watcher = FileWatcher::new(move || {
        bump.0.fetch_add(1, Ordering::SeqCst);
    })
    .expect("create watcher");
    (watcher, counter)
}

#[test]
fn an_edit_fires_a_change() {
    let scratch = Scratch::new("edit");
    let path = scratch.write("doc.md", "# One\n");
    let (mut watcher, counter) = watcher_with_counter();
    watcher.watch(Some(&path)).expect("watch");
    let baseline = settle(&counter);

    fs::write(&path, "# Two\n").expect("edit");
    assert!(counter.wait_for(baseline + 1), "no change reported");
}

#[test]
fn an_atomic_replacement_fires_a_change() {
    // Most editors write a temporary file and rename it over the original,
    // which is why the watcher follows the directory rather than the file.
    let scratch = Scratch::new("atomic");
    let path = scratch.write("doc.md", "# One\n");
    let (mut watcher, counter) = watcher_with_counter();
    watcher.watch(Some(&path)).expect("watch");
    let baseline = settle(&counter);

    let temp = scratch.0.join("doc.md.tmp");
    fs::write(&temp, "# Two\n").expect("write temp");
    fs::rename(&temp, &path).expect("rename over");

    assert!(
        counter.wait_for(baseline + 1),
        "no change reported after rename"
    );
}

#[test]
fn a_burst_of_writes_is_coalesced() {
    let scratch = Scratch::new("burst");
    let path = scratch.write("doc.md", "# One\n");
    let (mut watcher, counter) = watcher_with_counter();
    watcher.watch(Some(&path)).expect("watch");
    let baseline = settle(&counter);

    for i in 0..10 {
        fs::write(&path, format!("# {i}\n")).expect("edit");
        std::thread::sleep(Duration::from_millis(5));
    }

    assert!(counter.wait_for(baseline + 1), "no change reported");
    std::thread::sleep(Duration::from_millis(400));
    let fired = counter.get() - baseline;
    assert!(fired <= 3, "10 rapid writes produced {fired} reloads");
}

#[test]
fn edits_to_a_sibling_file_are_ignored() {
    let scratch = Scratch::new("sibling");
    let path = scratch.write("doc.md", "# One\n");
    let (mut watcher, counter) = watcher_with_counter();
    watcher.watch(Some(&path)).expect("watch");

    let baseline = settle(&counter);

    scratch.write("other.md", "# Other\n");
    assert!(
        counter.stayed_quiet_since(baseline),
        "a sibling edit fired a reload"
    );
}

#[test]
fn switching_documents_stops_watching_the_old_one() {
    let scratch = Scratch::new("switch");
    let first = scratch.write("first.md", "# First\n");
    let second = scratch.write("second.md", "# Second\n");
    let (mut watcher, counter) = watcher_with_counter();

    watcher.watch(Some(&first)).expect("watch first");
    watcher.watch(Some(&second)).expect("watch second");
    let baseline = settle(&counter);

    fs::write(&first, "# Changed\n").expect("edit first");
    assert!(
        counter.stayed_quiet_since(baseline),
        "the old document still fires"
    );

    fs::write(&second, "# Changed\n").expect("edit second");
    assert!(
        counter.wait_for(baseline + 1),
        "the new document does not fire"
    );
}

#[test]
fn watching_nothing_is_allowed() {
    let scratch = Scratch::new("none");
    let path = scratch.write("doc.md", "# One\n");
    let (mut watcher, counter) = watcher_with_counter();

    watcher.watch(Some(&path)).expect("watch");
    watcher.watch(None).expect("unwatch");
    let baseline = settle(&counter);

    fs::write(&path, "# Two\n").expect("edit");
    assert!(
        counter.stayed_quiet_since(baseline),
        "unwatched file still fires"
    );
}

#[test]
fn watching_the_same_file_twice_is_harmless() {
    let scratch = Scratch::new("twice");
    let path = scratch.write("doc.md", "# One\n");
    let (mut watcher, counter) = watcher_with_counter();

    watcher.watch(Some(&path)).expect("first");
    watcher.watch(Some(&path)).expect("second");
    let baseline = settle(&counter);

    fs::write(&path, "# Two\n").expect("edit");
    assert!(counter.wait_for(baseline + 1), "no change reported");
    std::thread::sleep(Duration::from_millis(400));
    let fired = counter.get() - baseline;
    assert!(fired <= 2, "registered twice: {fired} reloads for one edit");
}

#[test]
fn a_file_in_a_missing_directory_is_not_an_error() {
    let (mut watcher, _counter) = watcher_with_counter();
    let path = PathBuf::from("/definitely/not/here/doc.md");
    assert!(watcher.watch(Some(&path)).is_ok());
}

#[test]
fn deleting_the_document_fires_a_change() {
    let scratch = Scratch::new("delete");
    let path = scratch.write("doc.md", "# One\n");
    let (mut watcher, counter) = watcher_with_counter();
    watcher.watch(Some(&path)).expect("watch");
    let baseline = settle(&counter);

    fs::remove_file(&path).expect("delete");
    assert!(counter.wait_for(baseline + 1), "deletion was not reported");
}
