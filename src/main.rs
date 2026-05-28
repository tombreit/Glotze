mod api;
mod application;
mod config;
mod download;
mod i18n;
mod ui;
mod window;

use application::Application;

fn main() -> gtk::glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    i18n::init();
    Application::new(config::APP_ID).run()
}
