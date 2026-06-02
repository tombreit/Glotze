mod api;
mod application;
mod config;
mod download;
mod i18n;
mod net;
mod ui;
mod window;

use application::Application;
use glib::GlibLogger;

// Route the `log` facade into GLib's structured logging (→ journald under
// Flatpak). Info/Debug are gated by GLib's `G_MESSAGES_DEBUG`; log::error! maps
// to GLib Critical (non-fatal), not the fatal Error level.
static GLIB_LOGGER: GlibLogger = GlibLogger::new(
    glib::GlibLoggerFormat::Structured,
    glib::GlibLoggerDomain::CrateTarget,
);

fn main() -> gtk::glib::ExitCode {
    log::set_logger(&GLIB_LOGGER).expect("logger already set");
    log::set_max_level(log::LevelFilter::Debug);
    i18n::init();
    Application::new(config::APP_ID).run()
}
