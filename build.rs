// Forward a small set of Meson-supplied env vars to rustc so `option_env!` in
// src/config.rs picks them up. When Cargo runs standalone (no Meson), the env
// vars are absent and `option_env!` yields None, which config.rs falls back
// for — that keeps `cargo run` working for local development.
fn main() {
    for var in ["GETTEXT_PACKAGE", "LOCALEDIR", "APP_ID"] {
        println!("cargo:rerun-if-env-changed={var}");
        if let Ok(val) = std::env::var(var) {
            println!("cargo:rustc-env={var}={val}");
        }
    }
}
