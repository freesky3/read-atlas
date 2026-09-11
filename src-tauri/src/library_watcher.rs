use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

pub type WatcherResult<T> = Result<T, String>;

#[derive(Debug, Clone, Copy)]
pub struct LibraryWatcherConfig {
    pub debounce: Duration,
    pub stability_probe: Duration,
}

impl Default for LibraryWatcherConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(750),
            stability_probe: Duration::from_millis(150),
        }
    }
}

pub struct LibraryWatcher {
    watcher: Option<RecommendedWatcher>,
    stop: Option<Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl LibraryWatcher {
    pub fn start<F>(
        watch_paths: impl IntoIterator<Item = PathBuf>,
        config: LibraryWatcherConfig,
        on_stable_change: F,
    ) -> WatcherResult<Self>
    where
        F: Fn(Vec<PathBuf>) + Send + 'static,
    {
        Self::start_with(watch_paths, std::iter::empty(), config, on_stable_change)
    }

    pub fn start_with<F>(
        watch_paths: impl IntoIterator<Item = PathBuf>,
        non_recursive_paths: impl IntoIterator<Item = PathBuf>,
        config: LibraryWatcherConfig,
        on_stable_change: F,
    ) -> WatcherResult<Self>
    where
        F: Fn(Vec<PathBuf>) + Send + 'static,
    {
        let watch_paths: Vec<PathBuf> = watch_paths.into_iter().collect();
        let non_recursive_paths: Vec<PathBuf> = non_recursive_paths.into_iter().collect();
        if watch_paths.is_empty() {
            return Err("Library watcher has no directories".to_string());
        }
        for path in watch_paths.iter().chain(non_recursive_paths.iter()) {
            if !path.is_dir() {
                return Err(format!(
                    "Library directory is unavailable: {}",
                    path.display()
                ));
            }
        }

        let (event_tx, event_rx) = mpsc::channel::<Vec<PathBuf>>();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let mut watcher = RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    let _ = event_tx.send(event.paths);
                }
            },
            Config::default(),
        )
        .map_err(|error| format!("Unable to create library watcher: {error}"))?;
        for path in &watch_paths {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .map_err(|error| format!("Unable to watch {}: {error}", path.display()))?;
        }
        for path in &non_recursive_paths {
            watcher
                .watch(path, RecursiveMode::NonRecursive)
                .map_err(|error| format!("Unable to watch {}: {error}", path.display()))?;
        }

        let worker = thread::Builder::new()
            .name("read-library-watcher".to_string())
            .spawn(move || {
                let mut pending = HashSet::<PathBuf>::new();
                let mut last_event: Option<Instant> = None;
                loop {
                    if stop_rx.try_recv().is_ok() {
                        break;
                    }
                    match event_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(paths) => {
                            pending.extend(paths);
                            last_event = Some(Instant::now());
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                    let ready =
                        last_event.is_some_and(|instant| instant.elapsed() >= config.debounce);
                    if !ready || pending.is_empty() {
                        continue;
                    }
                    let paths = pending.iter().cloned().collect::<Vec<_>>();
                    if paths_are_stable(&paths, config.stability_probe) {
                        pending.clear();
                        last_event = None;
                        on_stable_change(paths);
                    } else {
                        last_event = Some(Instant::now());
                    }
                }
            })
            .map_err(|error| format!("Unable to start library watcher worker: {error}"))?;

        Ok(Self {
            watcher: Some(watcher),
            stop: Some(stop_tx),
            worker: Some(worker),
        })
    }
}

impl Drop for LibraryWatcher {
    fn drop(&mut self) {
        self.watcher.take();
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    len: u64,
    modified: Option<SystemTime>,
}

fn paths_are_stable(paths: &[PathBuf], probe: Duration) -> bool {
    let before = paths
        .iter()
        .filter_map(|path| file_stamp(path).map(|stamp| (path.clone(), stamp)))
        .collect::<Vec<_>>();
    if before.is_empty() {
        return true;
    }
    thread::sleep(probe);
    before
        .into_iter()
        .all(|(path, stamp)| file_stamp(&path) == Some(stamp))
}

fn file_stamp(path: &Path) -> Option<FileStamp> {
    if !is_pdf(path) {
        return None;
    }
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(FileStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn coalesces_file_events_until_the_pdf_is_stable() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let papers = temporary.path().join("Papers");
        fs::create_dir(&papers).expect("Papers");
        let (tx, rx) = mpsc::channel();
        let watcher = LibraryWatcher::start(
            [papers.clone()],
            LibraryWatcherConfig {
                debounce: Duration::from_millis(80),
                stability_probe: Duration::from_millis(20),
            },
            move |paths| {
                let _ = tx.send(paths);
            },
        )
        .expect("watcher");

        let pdf = papers.join("paper.pdf");
        fs::write(&pdf, b"%PDF-1.7\nfirst").expect("first write");
        fs::write(&pdf, b"%PDF-1.7\nsecond stable value").expect("second write");

        let paths = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("stable watcher event");
        assert!(paths.iter().any(|path| path.ends_with("paper.pdf")));
        assert!(
            rx.recv_timeout(Duration::from_millis(250)).is_err(),
            "the burst should be coalesced into one callback"
        );
        drop(watcher);
    }

    #[test]
    fn rejects_a_missing_papers_directory() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let result = LibraryWatcher::start(
            [temporary.path().join("missing")],
            LibraryWatcherConfig::default(),
            |_| {},
        );
        assert!(result.is_err());
    }
}
