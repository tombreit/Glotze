use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const LOGO_SIZE: i32 = 40;

/// Build an `Image` widget showing the channel logo at `LOGO_SIZE` × `LOGO_SIZE`.
/// Falls back to `default.svg` for unknown channels.
pub fn channel_logo(channel: &str) -> gtk::Image {
    let safe = channel.replace('/', "_");
    let dir = channels_dir();
    let path = dir.join(format!("{safe}.svg"));

    let img = if path.exists() {
        gtk::Image::from_file(&path)
    } else {
        let fallback = dir.join("default.svg");
        if !fallback.exists() {
            log::warn!("logo fallback missing at {}", fallback.display());
        }
        gtk::Image::from_file(&fallback)
    };
    img.set_pixel_size(LOGO_SIZE);
    img
}

fn channels_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        // Explicit override wins (useful for tests and weird installs).
        if let Ok(d) = std::env::var("GLOTZE_DATA_DIR") {
            return PathBuf::from(d).join("channels");
        }
        // Dev: running `cargo run` from the crate root.
        let dev = PathBuf::from("data/channels");
        if dev.join("default.svg").exists() {
            return dev;
        }
        // Flatpak / installed.
        PathBuf::from("/app/share/glotze/channels")
    })
}
