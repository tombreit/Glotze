mod api;
mod application;
mod download;
mod ui;
mod window;

use application::Application;

const APP_ID: &str = "io.github.tombreit.Glotze";

fn main() -> gtk::glib::ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    Application::new(APP_ID).run()
}
