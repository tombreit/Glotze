// Values injected by Meson at build time (see ../build.rs and ../meson.build).
// `cargo run` outside Meson leaves these unset; the fallbacks let the app still
// start in dev, reading data straight from the source tree.

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

/// Where the channel logos live (`$PKGDATADIR/channels`).
pub const PKGDATADIR: &str = match option_env!("PKGDATADIR") {
    Some(v) => v,
    None => concat!(env!("CARGO_MANIFEST_DIR"), "/data"),
};

/// True for a plain `cargo run`, i.e. not installed by Meson.
pub const UNINSTALLED: bool = option_env!("PKGDATADIR").is_none();
