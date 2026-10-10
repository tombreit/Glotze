pub mod progress;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use async_channel::{Receiver, Sender};
use gettextrs::gettext;

use crate::api::models::{Quality, Show};
use crate::ui::format::format_iso_date;
use progress::{Progress, State};

const CHUNK_BYTES: usize = 64 * 1024;
/// How often a running download reports progress; a progress bar needs no more.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

/// Runs downloads on their own threads and reports every state change on one
/// channel. Lives on the main thread.
pub struct Manager {
    next_id: Cell<u64>,
    tx: Sender<Progress>,
    /// In-flight downloads by id: inserted by `enqueue`, removed by `forget` once
    /// the progress consumer has seen a terminal state.
    active: RefCell<HashMap<u64, ActiveDownload>>,
}

struct ActiveDownload {
    title: String,
    /// Checked by the worker before each chunk.
    cancel: Arc<AtomicBool>,
    part_path: PathBuf,
}

impl Manager {
    pub fn new() -> (Self, Receiver<Progress>) {
        let (tx, rx) = async_channel::unbounded();
        let manager = Self {
            next_id: Cell::new(1),
            tx,
            active: RefCell::new(HashMap::new()),
        };
        (manager, rx)
    }

    /// Start downloading `show` in `quality` into `~/Videos/Glotze`; returns
    /// the download id that all its `Progress` events carry.
    pub fn enqueue(&self, show: &Show, quality: Quality) -> Result<u64> {
        let url = show
            .url_for(quality)
            .ok_or_else(|| anyhow!(gettext("no video in this quality")))?
            .to_string();
        let dir = download_dir().ok_or_else(|| anyhow!(gettext("no Videos folder found")))?;
        let id = self.next_id.get();
        self.next_id.set(id + 1);

        // The id in the `.part` name keeps concurrent downloads apart.
        let ext = guess_extension(&url).unwrap_or("mp4");
        let stem = file_stem(show, quality);
        let part_path = dir.join(format!("{stem}.{id}.{ext}.part"));
        let final_path = dir.join(format!("{stem}.{ext}"));

        let cancel = Arc::new(AtomicBool::new(false));
        self.active.borrow_mut().insert(
            id,
            ActiveDownload {
                title: show.title.clone(),
                cancel: Arc::clone(&cancel),
                part_path: part_path.clone(),
            },
        );

        // A plain thread rather than `gio::spawn_blocking`: a download can run
        // for many minutes and shouldn't occupy GIO's shared worker pool.
        let tx = self.tx.clone();
        let title = show.title.clone();
        std::thread::spawn(move || {
            let send = |state| {
                let _ = tx.send_blocking(Progress {
                    id,
                    title: title.clone(),
                    state,
                });
            };
            // Announce immediately so the UI flips to "downloading" before the
            // network warms up.
            send(State::Running {
                bytes_done: 0,
                bytes_total: 0,
            });
            let on_progress = |bytes_done, bytes_total| {
                send(State::Running {
                    bytes_done,
                    bytes_total,
                });
            };
            let state = download_to_disk(&url, &part_path, &final_path, &cancel, on_progress)
                .unwrap_or_else(|e| {
                    log::error!("download id={id} failed: {e:#}");
                    cleanup_partial(&part_path);
                    State::Failed {
                        reason: format!("{e:#}"),
                    }
                });
            send(state);
        });

        Ok(id)
    }

    /// Cancel a download. Takes effect right away: the `.part` file is removed
    /// and `Cancelled` is emitted from here, so a worker stuck in a stalled read
    /// can't keep the row busy. The worker itself stops at its next chunk; its
    /// late events are dropped because the id is no longer active by then.
    pub fn cancel(&self, id: u64) {
        let active = self.active.borrow();
        let Some(d) = active.get(&id) else {
            return;
        };
        d.cancel.store(true, Ordering::Relaxed);
        cleanup_partial(&d.part_path);
        let _ = self.tx.try_send(Progress {
            id,
            title: d.title.clone(),
            state: State::Cancelled,
        });
    }

    /// Drop a download after its terminal state has been handled. Any event
    /// arriving for it afterwards is stale (see `cancel`).
    pub fn forget(&self, id: u64) {
        self.active.borrow_mut().remove(&id);
    }

    pub fn is_active(&self, id: u64) -> bool {
        self.active.borrow().contains_key(&id)
    }

    pub fn active_count(&self) -> usize {
        self.active.borrow().len()
    }

    /// Delete the partial files of all running downloads. Called when the user
    /// confirms closing the window mid-download; the process exits right after,
    /// taking the workers with it.
    pub fn cleanup_partials(&self) {
        for d in self.active.borrow().values() {
            cleanup_partial(&d.part_path);
        }
    }
}

/// Filename stem from series, episode and broadcast date, plus a quality tag:
/// `tagesschau-20-00-uhr-2026-05-23.q-hd`. The date keeps daily shows with a
/// constant title from overwriting each other; the quality tag keeps different
/// qualities of one episode apart. The series is left out when the episode
/// title already starts with it.
fn file_stem(show: &Show, quality: Quality) -> String {
    let topic = show.topic.trim();
    let title = show.title.trim();
    let name = if topic.is_empty() || title.to_lowercase().starts_with(&topic.to_lowercase()) {
        title.to_string()
    } else {
        format!("{topic} {title}")
    };
    let mut stem = slugify(&name);
    if let Some(ts) = show.timestamp.filter(|t| *t > 0) {
        stem.push('-');
        stem.push_str(&format_iso_date(ts));
    }
    format!("{stem}.q-{}", quality.tag())
}

/// Stream `url` into `part_path`, reporting `(bytes_done, bytes_total)` every
/// `PROGRESS_INTERVAL`, then rename it to `final_path`. Returns the terminal
/// state: `Done`, or `Cancelled` if the cancel flag was raised.
fn download_to_disk(
    url: &str,
    part_path: &Path,
    final_path: &Path,
    cancel: &AtomicBool,
    on_progress: impl Fn(u64, u64),
) -> Result<State> {
    let dir = part_path
        .parent()
        .ok_or_else(|| anyhow!("partial path has no parent directory"))?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    // No global timeout: a download can legitimately run for a long time. ureq
    // treats a non-2xx status as an error.
    let agent = crate::net::agent(None, Some(Duration::from_secs(15)));
    let body = agent.get(url).call()?.into_body();
    let total = body.content_length().unwrap_or(0);
    let mut reader = body.into_reader();

    let mut file =
        File::create(part_path).with_context(|| format!("creating {}", part_path.display()))?;
    let mut buf = vec![0u8; CHUNK_BYTES];
    let mut done: u64 = 0;
    let mut last_emit = Instant::now();

    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(file);
            cleanup_partial(part_path);
            log::info!(
                "download cancelled at {done} bytes: {}",
                part_path.display()
            );
            return Ok(State::Cancelled);
        }

        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;

        if last_emit.elapsed() >= PROGRESS_INTERVAL {
            on_progress(done, total);
            last_emit = Instant::now();
        }
    }

    drop(file);
    std::fs::rename(part_path, final_path).with_context(|| {
        format!(
            "renaming {} -> {}",
            part_path.display(),
            final_path.display()
        )
    })?;

    log::info!("downloaded {} ({done} bytes)", final_path.display());
    Ok(State::Done {
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
/// so they don't mingle with other things in `~/Videos`; created on demand.
fn download_dir() -> Option<PathBuf> {
    glib::user_special_dir(glib::UserDirectory::Videos)
        .map(|p| p.join("Glotze"))
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Videos").join("Glotze"))
        })
}

/// The download directory as a short display string, with `$HOME` collapsed to
/// `~` (e.g. `~/Videos/Glotze`).
pub fn download_dir_display() -> String {
    let Some(dir) = download_dir() else {
        return gettext("your Videos folder");
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

/// Turn an arbitrary show title into a kebab-case, ASCII-safe filename slug:
/// nothing weird (umlauts, spaces, punctuation) ever lands in `~/Videos`.
fn slugify(input: &str) -> String {
    const MAX_LEN: usize = 100;
    let mut out = String::with_capacity(input.len());
    // Start as "just emitted a dash" so any leading non-alphanumerics are
    // suppressed without an explicit trim afterwards.
    let mut last_was_dash = true;

    for ch in input.chars() {
        let german: Option<&str> = match ch {
            'ä' | 'Ä' => Some("ae"),
            'ö' | 'Ö' => Some("oe"),
            'ü' | 'Ü' => Some("ue"),
            'ß' => Some("ss"),
            _ => None,
        };
        if let Some(s) = german {
            out.push_str(s);
            last_was_dash = false;
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > MAX_LEN {
        out.truncate(MAX_LEN);
        while out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        "download".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::slugify;

    #[test]
    fn umlauts_get_ascii_equivalents() {
        assert_eq!(slugify("Tagesschau für Kinder"), "tagesschau-fuer-kinder");
        assert_eq!(slugify("Straße der Lieder"), "strasse-der-lieder");
    }

    #[test]
    fn punctuation_and_spaces_collapse_to_one_dash() {
        assert_eq!(slugify("heute-show: Spezial!"), "heute-show-spezial");
        assert_eq!(
            slugify("Tagesschau in 100 Sekunden – 23.05.2026"),
            "tagesschau-in-100-sekunden-23-05-2026"
        );
    }

    #[test]
    fn leading_and_trailing_junk_stripped() {
        assert_eq!(slugify("  …Hallo Welt…  "), "hallo-welt");
    }

    #[test]
    fn empty_or_unmappable_falls_back() {
        assert_eq!(slugify(""), "download");
        assert_eq!(slugify("———"), "download");
    }
}
