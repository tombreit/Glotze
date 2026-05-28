// Standard gettext bootstrap as recommended by the gtk4-rs book and used by
// every GNOME Rust app. Must run before any user-facing string is constructed.

use gettextrs::{LocaleCategory, bind_textdomain_codeset, bindtextdomain, setlocale, textdomain};

use crate::config;

pub fn init() {
    setlocale(LocaleCategory::LcAll, "");
    if let Err(e) = bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR) {
        log::warn!("bindtextdomain failed: {e}");
    }
    if let Err(e) = bind_textdomain_codeset(config::GETTEXT_PACKAGE, "UTF-8") {
        log::warn!("bind_textdomain_codeset failed: {e}");
    }
    if let Err(e) = textdomain(config::GETTEXT_PACKAGE) {
        log::warn!("textdomain failed: {e}");
    }
}
