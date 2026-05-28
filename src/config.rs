// Values injected by Meson at build time (see ../build.rs and ../meson.build).
// `cargo run` outside Meson leaves these unset; the fallbacks let the app still
// start in dev, just without finding installed translations.

pub const APP_ID: &str = match option_env!("APP_ID") {
    Some(v) => v,
    None => "io.github.tombreit.Glotze",
};

pub const GETTEXT_PACKAGE: &str = match option_env!("GETTEXT_PACKAGE") {
    Some(v) => v,
    None => "glotze",
};

pub const LOCALEDIR: &str = match option_env!("LOCALEDIR") {
    Some(v) => v,
    None => "/usr/local/share/locale",
};
