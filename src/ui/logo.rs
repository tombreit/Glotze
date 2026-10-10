use std::path::PathBuf;

use crate::config;

const LOGO_SIZE: i32 = 40;

/// Build an `Image` widget showing the channel logo at `LOGO_SIZE` × `LOGO_SIZE`.
/// Falls back to `default.svg` for unknown channels.
pub fn channel_logo(channel: &str) -> gtk::Image {
    // Match case-insensitively: the API's channel casing (e.g. "ONE", "ARD")
    // varies, but the asset filenames are canonical lowercase.
    let safe = channel.replace('/', "_").to_lowercase();
    let dir = PathBuf::from(config::PKGDATADIR).join("channels");
    let path = dir.join(format!("{safe}.svg"));
    let path = if path.exists() {
        path
    } else {
        dir.join("default.svg")
    };
    let img = gtk::Image::from_file(&path);
    img.set_pixel_size(LOGO_SIZE);
    img
}
