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
    /// Per-download cancel flags. The worker checks this once per chunk.
    cancellers: RefCell<HashMap<u64, Arc<AtomicBool>>>,
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
    Done { bytes_total: u64, path: PathBuf },
    Cancelled,
}

impl Manager {
    pub fn new() -> Rc<Self> {
        let (tx, rx) = async_channel::unbounded();
        Rc::new(Self {
            next_id: Cell::new(1),
            tx,
            rx,
            cancellers: RefCell::new(HashMap::new()),
        })
    }

    pub fn progress_rx(&self) -> Receiver<Progress> {
        self.rx.clone()
    }

    pub fn enqueue(&self, show: &Show, quality: Quality) -> Option<EnqueueInfo> {
        let url = show.url_for(quality)?.to_string();
        let id = self.next_id.get();
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
        self.cancellers.borrow_mut().insert(id, Arc::clone(&cancel));

        gtk::gio::spawn_blocking(move || {
            // Announce immediately so the UI shows the row before the network warms up.
            let _ = tx.send_blocking(Progress::running(id, title.clone(), 0, 0));

            match download_to_disk(id, &title, &url, &tx, &cancel) {
                Ok(Outcome::Done { bytes_total, path }) => {
                    let _ = tx.send_blocking(Progress::done(id, title, bytes_total, path));
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
        if let Some(flag) = self.cancellers.borrow().get(&id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Drop the cancellation flag for a download once it has reached a terminal
    /// state. Called by the progress consumer in `window.rs`.
    pub fn forget(&self, id: u64) {
        self.cancellers.borrow_mut().remove(&id);
    }
}

fn download_to_disk(
    id: u64,
    title: &str,
    url: &str,
    tx: &Sender<Progress>,
    cancel: &AtomicBool,
) -> Result<Outcome> {
    let dir = download_dir().ok_or_else(|| anyhow!("could not resolve Videos directory"))?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;

    let ext = guess_extension(url).unwrap_or("mp4");
    // Slug fixes the final filename; the `.part` file embeds the download id
    // as well so two concurrent downloads of the same title can't clobber
    // each other.
    let slug = slugify(title);
    let final_path = dir.join(format!("{slug}.{ext}"));
    let part_path = dir.join(format!("{slug}.{id}.{ext}.part"));

    let http = reqwest::blocking::Client::builder()
        .user_agent(concat!("Glotze/", env!("CARGO_PKG_VERSION")))
        .timeout(None)
        .connect_timeout(Duration::from_secs(15))
        .build()?;

    let mut resp = http.get(url).send()?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);

    let mut file =
        File::create(&part_path).with_context(|| format!("creating {}", part_path.display()))?;
    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut done: u64 = 0;
    let mut last_emit = Instant::now();

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            cleanup_partial(&part_path);
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
    std::fs::rename(&part_path, &final_path).with_context(|| {
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
        bytes_total: done,
        path: final_path,
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
