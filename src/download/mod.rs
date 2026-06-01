pub mod progress;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use async_channel::{Receiver, Sender};

use crate::api::models::{Quality, Show};
use progress::{Progress, slugify};

const CHUNK_BYTES: usize = 64 * 1024;
const PROGRESS_DEBOUNCE: Duration = Duration::from_millis(100);

pub struct Manager {
    next_id: Cell<u64>,
    tx: Sender<Progress>,
    rx: Receiver<Progress>,
    /// Currently-running downloads, keyed by id. Inserted in `enqueue`, removed
    /// by `forget` on every terminal state — so this map is exactly the set of
    /// in-flight downloads, and the source of truth for both `active_count` and
    /// `cleanup_partials`.
    active: RefCell<HashMap<u64, ActiveDownload>>,
}

/// What the Manager keeps for one in-flight download.
struct ActiveDownload {
    /// Cancel flag the worker checks once per chunk.
    cancel: Arc<AtomicBool>,
    /// Exact partial-file path this download writes to (constructed by
    /// `target_paths`). Held so `cleanup_partials` can delete precisely this
    /// file — and only this file — if the user closes mid-download.
    part_path: PathBuf,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Returned to callers; fields read in future cancel-by-id support.
pub struct EnqueueInfo {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub quality: Quality,
}

/// Worker outcome that the spawning closure maps into a `Progress` event.
enum Outcome {
    Done { path: PathBuf },
    Cancelled,
}

impl Manager {
    pub fn new() -> Rc<Self> {
        let (tx, rx) = async_channel::unbounded();
        Rc::new(Self {
            next_id: Cell::new(1),
            tx,
            rx,
            active: RefCell::new(HashMap::new()),
        })
    }

    pub fn progress_rx(&self) -> Receiver<Progress> {
        self.rx.clone()
    }

    pub fn enqueue(&self, show: &Show, quality: Quality) -> Option<EnqueueInfo> {
        let url = show.url_for(quality)?.to_string();
        let id = self.next_id.get();
        // Resolve the on-disk paths up front (main thread): the partial path is
        // tracked for cleanup, and both are handed to the worker so there's a
        // single source of truth. No Videos dir → nowhere to download.
        let (part_path, final_path) = target_paths(&show.title, &url, id)?;
        self.next_id.set(id + 1);

        let info = EnqueueInfo {
            id,
            title: show.title.clone(),
            url: url.clone(),
            quality,
        };

        let tx = self.tx.clone();
        let title = show.title.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.active.borrow_mut().insert(
            id,
            ActiveDownload {
                cancel: Arc::clone(&cancel),
                part_path: part_path.clone(),
            },
        );

        gtk::gio::spawn_blocking(move || {
            // Announce immediately so the UI shows the row before the network warms up.
            let _ = tx.send_blocking(Progress::running(id, title.clone(), 0, 0));

            match download_to_disk(id, &title, &url, &part_path, &final_path, &tx, &cancel) {
                Ok(Outcome::Done { path }) => {
                    let _ = tx.send_blocking(Progress::done(id, title, path));
                }
                Ok(Outcome::Cancelled) => {
                    let _ = tx.send_blocking(Progress::cancelled(id, title));
                }
                Err(e) => {
                    log::error!("download id={id} failed: {e:#}");
                    let _ = tx.send_blocking(Progress::failed(id, title, format!("{e:#}")));
                }
            }
        });

        Some(info)
    }

    /// Mark the given download for cancellation. The worker will notice on the
    /// next chunk boundary, delete the partial file, and emit
    /// `Progress::cancelled`.
    pub fn cancel(&self, id: u64) {
        if let Some(d) = self.active.borrow().get(&id) {
            d.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Forget a download once it has reached a terminal state (drops its cancel
    /// flag and tracked partial path). Called by the progress consumer in
    /// `window.rs`.
    pub fn forget(&self, id: u64) {
        self.active.borrow_mut().remove(&id);
    }

    /// How many downloads are still running.
    pub fn active_count(&self) -> usize {
        self.active.borrow().len()
    }

    /// Delete the partial files of all still-running downloads. Called when the
    /// user confirms closing the window mid-download.
    ///
    /// Deliberately conservative: it only touches the exact `.part` paths this
    /// Manager recorded for in-flight downloads, and re-checks each one is a
    /// `.part` file living directly in the resolved download directory before
    /// removing it. A path failing either check is logged and skipped. The
    /// final (renamed) file of a completed download is never in `active`, so it
    /// can't be reached here.
    pub fn cleanup_partials(&self) {
        let dir = download_dir();
        for d in self.active.borrow().values() {
            let p = &d.part_path;
            let safe = p.extension().is_some_and(|e| e == "part")
                && dir.as_deref().is_some_and(|root| p.parent() == Some(root));
            if safe {
                cleanup_partial(p);
            } else {
                log::warn!("refusing to delete unexpected partial path {}", p.display());
            }
        }
    }
}

/// Resolve `(part_path, final_path)` for a download: the `.part` file it writes
/// to and the file it's renamed to on success. `None` when no Videos directory
/// can be resolved (nowhere to download). The `id` keeps concurrent downloads of
/// the same title from sharing a `.part` file.
fn target_paths(title: &str, url: &str, id: u64) -> Option<(PathBuf, PathBuf)> {
    let dir = download_dir()?;
    let ext = guess_extension(url).unwrap_or("mp4");
    let slug = slugify(title);
    let part_path = dir.join(format!("{slug}.{id}.{ext}.part"));
    let final_path = dir.join(format!("{slug}.{ext}"));
    Some((part_path, final_path))
}

fn download_to_disk(
    id: u64,
    title: &str,
    url: &str,
    // Paths are resolved by `target_paths` in `enqueue` (the `.part` file is
    // also tracked there for cleanup), so the worker just writes and renames.
    part_path: &Path,
    final_path: &Path,
    tx: &Sender<Progress>,
    cancel: &AtomicBool,
) -> Result<Outcome> {
    let dir = part_path
        .parent()
        .ok_or_else(|| anyhow!("partial path has no parent directory"))?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    let http = reqwest::blocking::Client::builder()
        .user_agent(concat!("Glotze/", env!("CARGO_PKG_VERSION")))
        .timeout(None)
        .connect_timeout(Duration::from_secs(15))
        .build()?;

    let mut resp = http.get(url).send()?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);

    let mut file =
        File::create(part_path).with_context(|| format!("creating {}", part_path.display()))?;
    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut done: u64 = 0;
    let mut last_emit = Instant::now();

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            cleanup_partial(part_path);
            log::info!("download id={id} cancelled at {done} bytes");
            return Ok(Outcome::Cancelled);
        }

        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;

        if last_emit.elapsed() >= PROGRESS_DEBOUNCE {
            let _ = tx.send_blocking(Progress::running(id, title.to_string(), done, total));
            last_emit = Instant::now();
        }
    }

    file.flush()?;
    drop(file);
    std::fs::rename(part_path, final_path).with_context(|| {
        format!(
            "renaming {} -> {}",
            part_path.display(),
            final_path.display()
        )
    })?;

    log::info!(
        "download id={id} -> {} ({} bytes)",
        final_path.display(),
        done
    );
    Ok(Outcome::Done {
        path: final_path.to_path_buf(),
    })
}

fn cleanup_partial(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => log::debug!("removed partial {}", path.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => log::warn!("failed to remove partial {}: {e}", path.display()),
    }
}

/// The directory Glotze downloads into — `~/Videos/Glotze` — when a Videos (or
/// home) directory can be resolved. All downloads land in a dedicated subfolder
/// so they don't mingle with other things in `~/Videos`; created on demand by
/// `create_dir_all`.
pub fn download_dir() -> Option<PathBuf> {
    gtk::glib::user_special_dir(gtk::glib::UserDirectory::Videos)
        .map(|p| p.join("Glotze"))
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Videos").join("Glotze"))
        })
}

/// The download directory as a short display string, with `$HOME` collapsed to
/// `~` (e.g. `~/Videos/Glotze`). Falls back to a generic phrase if no Videos
/// directory can be resolved.
pub fn download_dir_display() -> String {
    let Some(dir) = download_dir() else {
        return "your Videos folder".to_string();
    };
    if let Some(home) = std::env::var_os("HOME")
        && let Ok(rel) = dir.strip_prefix(PathBuf::from(home))
    {
        return format!("~/{}", rel.display());
    }
    dir.display().to_string()
}

fn guess_extension(url: &str) -> Option<&str> {
    let path = url.split('?').next()?;
    let last = path.rsplit('/').next()?;
    let (_, ext) = last.rsplit_once('.')?;
    if ext.is_empty() || ext.len() > 5 {
        return None;
    }
    Some(ext)
}
