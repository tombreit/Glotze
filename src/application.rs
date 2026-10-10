use std::path::PathBuf;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, gio, glib};

use crate::config;
use crate::download::download_dir_display;
use crate::window;

pub fn run(app_id: &str) -> glib::ExitCode {
    let app = adw::Application::builder().application_id(app_id).build();

    app.connect_startup(|app| {
        register_dev_icon_path();
        install_app_actions(app);
        app.set_accels_for_action("app.quit", &["<Primary>q"]);
        app.set_accels_for_action("app.about", &["<Primary>comma"]);
        app.set_accels_for_action("win.search-focus", &["<Primary>f"]);
        app.set_accels_for_action("win.show-search", &["<Primary>1"]);
        app.set_accels_for_action("win.show-downloads", &["<Primary>2"]);
    });

    app.connect_activate(|app| {
        // Glotze is single-window: a second launch just raises the existing one.
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        window::build(app).present();
        maybe_show_welcome(app);
    });

    app.run()
}

fn install_app_actions(app: &adw::Application) {
    // Close the windows rather than `app.quit()`, so the close-request handler
    // still gets to confirm discarding running downloads.
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| {
            for window in app.windows() {
                window.close();
            }
        })
        .build();
    let about = gio::ActionEntry::builder("about")
        .activate(|app: &adw::Application, _, _| show_about(app))
        .build();
    let welcome = gio::ActionEntry::builder("welcome")
        .activate(|app: &adw::Application, _, _| show_welcome(app, None))
        .build();
    app.add_action_entries([quit, about, welcome]);
}

fn show_about(app: &adw::Application) {
    let dialog = adw::AboutDialog::builder()
        .application_name(gettext("Glotze"))
        .application_icon(app.application_id().unwrap_or_default())
        .version(env!("CARGO_PKG_VERSION"))
        .developer_name("Thomas Breitner")
        .website("https://tombreit.github.io/Glotze/")
        .issue_url("https://github.com/tombreit/Glotze/issues")
        .copyright("© 2026 Thomas Breitner")
        .license_type(gtk::License::Custom)
        // Single physical line so xgettext's C parser extracts the same string
        // Rust sees at runtime — `\<newline>` line-continuation in Rust strips
        // the leading whitespace on the next line, but the C parser keeps it,
        // and that mismatch breaks msgid lookup against the catalogue.
        .license(gettext("Licensed under the [European Union Public Licence v1.2 (EUPL-1.2)](https://eupl.eu/). See the LICENSE file or visit [eupl.eu](https://eupl.eu/) for the full text."))
        // The welcome/intro text lives in the dedicated welcome dialog (the
        // header info button → `app.welcome`); don't duplicate it here.
        .build();

    dialog.add_link(&gettext("Repository"), "https://github.com/tombreit/Glotze");
    dialog.add_link(&gettext("Donate"), "https://ko-fi.com/thmsde");

    // AdwAboutDialog renders each entry as "Display Name URL", turning the name
    // into a link to the trailing URL. Only the descriptive name is translated;
    // the URL is appended separately so it stays out of the catalogue.
    let acknowledgements = [
        format!(
            "{} https://github.com/mediathekview/MediathekViewWeb",
            gettext("MediathekViewWeb - API provider")
        ),
        format!(
            "{} https://github.com/mediathekview/zapp",
            gettext("Zapp - Android app that inspired Glotze")
        ),
        format!(
            "{} https://commons.wikimedia.org/wiki/File:TestChart_similar_to_old_TV_testscreens.svg",
            gettext("App icon - inspired by TestChart")
        ),
        format!(
            "{} https://gtk-rs.org/",
            gettext("gtk-rs - Provides bindings for GTK-related libraries")
        ),
    ];
    let acknowledgements: Vec<&str> = acknowledgements.iter().map(String::as_str).collect();
    dialog.add_acknowledgement_section(None, &acknowledgements);

    dialog.present(app.active_window().as_ref());
}

/// Show the welcome dialog on launch until the user opts out via its
/// "Don't show this again" checkbox. The opt-out is a single marker file under
/// the user data dir — no `GSettings` schema/plumbing, and it works under
/// `cargo run` and Flatpak.
fn maybe_show_welcome(app: &adw::Application) {
    let Some(marker) = welcome_marker(app) else {
        return;
    };
    if marker.exists() {
        return;
    }
    show_welcome(app, Some(marker));
}

fn welcome_marker(app: &adw::Application) -> Option<PathBuf> {
    let app_id = app.application_id()?;
    Some(
        glib::user_data_dir()
            .join(app_id.as_str())
            .join("welcome-shown"),
    )
}

// Used by both the welcome dialog and About>Details. `.replace("{dir}", …)`
// (instead of Rust's `format!`) lets translators reorder the placeholder. The
// translatable string is one physical line: xgettext's C parser preserves the
// indentation on `\<newline>` continuations whereas Rust strips it, and the
// runtime msgid lookup would then miss the catalogue entry.
#[rustfmt::skip]
fn welcome_text(dir: &str) -> String {
    gettext("Glotze <b>downloads</b> episodes for you — there's no streaming and no built-in player.\n\nContent comes from the public broadcasters (DACH region), eg. ARD, ZDF, 3sat, arte,… via the MediathekViewWeb API; files are saved to <tt>{dir}</tt>. Some videos are geo-blocked to Germany, Austria or Switzerland.\n\n“Glotze” is affectionate German slang for a TV set — roughly “the box” or “the telly”.")
        .replace("{dir}", dir)
}

// `marker` is `Some` only for the automatic first-launch popup: it carries the
// "Don't show this again" opt-out and the path to record it. When invoked on
// demand (the header welcome button passes `None`), the welcome text is shown
// without the checkbox — there's nothing to opt out of.
fn show_welcome(app: &adw::Application, marker: Option<PathBuf>) {
    let body = welcome_text(&glib::markup_escape_text(&download_dir_display()));

    let dialog = adw::AlertDialog::builder()
        .heading(gettext("Welcome to Glotze"))
        .body(body)
        .body_use_markup(true)
        .build();
    dialog.add_response("ok", &gettext("_Got it"));
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("ok");

    if let Some(marker) = marker {
        let dont_show = gtk::CheckButton::builder()
            .label(gettext("Don't show this again"))
            .halign(gtk::Align::Center)
            .build();
        dialog.set_extra_child(Some(&dont_show));

        // Remember the opt-out only when the box is ticked; otherwise the dialog
        // greets the user again next launch.
        dialog.connect_response(None, move |_dialog: &adw::AlertDialog, _response: &str| {
            if !dont_show.is_active() {
                return;
            }
            if let Some(parent) = marker.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Err(e) = std::fs::write(&marker, b"") {
                log::warn!("could not record welcome marker {}: {e}", marker.display());
            }
        });
    }

    dialog.present(app.active_window().as_ref());
}

/// When running from `cargo run` the app's SVG icon isn't yet in any installed
/// hicolor theme, so the About dialog and window icon would be blank. Point the
/// icon theme at the source tree's `data/icons`.
fn register_dev_icon_path() {
    if !config::UNINSTALLED {
        return;
    }
    if let Some(display) = gdk::Display::default() {
        gtk::IconTheme::for_display(&display)
            .add_search_path(format!("{}/icons", config::PKGDATADIR));
    }
}
