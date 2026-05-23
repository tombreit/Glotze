use std::path::Path;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

use crate::window::AppWindow;

pub struct Application(adw::Application);

impl Application {
    pub fn new(app_id: &str) -> Self {
        let app = adw::Application::builder().application_id(app_id).build();

        app.connect_startup(|app| {
            register_dev_icon_path();
            install_app_actions(app);
            // Keyboard accelerators. Held centrally so users can override them
            // via GSettings later (gtk-application-prefer-dark-theme style).
            app.set_accels_for_action("app.quit", &["<Primary>q"]);
            app.set_accels_for_action("app.about", &["<Primary>comma"]);
            app.set_accels_for_action("win.search-focus", &["<Primary>f"]);
            app.set_accels_for_action("win.show-search", &["<Primary>1"]);
            app.set_accels_for_action("win.show-downloads", &["<Primary>2"]);
        });

        app.connect_activate(|app| {
            let window = AppWindow::new(app);
            window.present();
        });

        Self(app)
    }

    pub fn run(&self) -> glib::ExitCode {
        self.0.run()
    }
}

fn install_app_actions(app: &adw::Application) {
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| app.quit())
        .build();
    let about = gio::ActionEntry::builder("about")
        .activate(|app: &adw::Application, _, _| show_about(app))
        .build();
    app.add_action_entries([quit, about]);
}

fn show_about(app: &adw::Application) {
    let dialog = adw::AboutDialog::builder()
        .application_name("Glotze")
        .application_icon(app.application_id().unwrap_or_default())
        .version(env!("CARGO_PKG_VERSION"))
        .developer_name("tombreit")
        .website("https://thms.de")
        .issue_url("https://github.com/tombreit/Glotze/issues")
        .copyright("© 2026 tombreit")
        .license_type(gtk::License::Custom)
        .license(
            "Licensed under the \
             [European Union Public Licence v1.2 (EUPL-1.2)](https://eupl.eu/). \
             See the COPYING file or visit [eupl.eu](https://eupl.eu/) for the \
             full text.",
        )
        .comments(
            "Search and download episodes from German public broadcaster \
             Mediatheken (ARD, ZDF, 3sat, arte, …) via the MediathekViewWeb API.",
        )
        .build();

    dialog.add_link("Repository", "https://github.com/tombreit/Glotze");

    dialog.add_acknowledgement_section(
        Some("Stands on the shoulders of"),
        &[
            "MediathekViewWeb https://github.com/mediathekview/MediathekViewWeb",
            "MediathekView https://github.com/mediathekview",
            "Zapp https://github.com/mediathekview/zapp",
            "gtk-rs https://gtk-rs.org/",
            "Fractal https://gitlab.gnome.org/World/fractal",
            "Loupe https://gitlab.gnome.org/GNOME/loupe",
            "Bustle https://gitlab.gnome.org/World/bustle",
            "Gitte https://codeberg.org/ckruse/Gitte",
            "App icon: TestChart, CC0 https://commons.wikimedia.org/wiki/File:TestChart_similar_to_old_TV_testscreens.svg",
        ],
    );

    dialog.present(app.active_window().as_ref());
}

/// When running from `cargo run` the app's SVG icon isn't yet in any installed
/// hicolor theme, so the About dialog and window icon would be blank. Point
/// the icon theme at `data/icons/` if it exists relative to the cwd.
fn register_dev_icon_path() {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let theme = gtk::IconTheme::for_display(&display);
    for candidate in ["data/icons", "../data/icons"] {
        if Path::new(candidate).join("hicolor").is_dir() {
            theme.add_search_path(candidate);
            return;
        }
    }
}
