project(
  'glotze',
  # Single source of truth: read the [package] version from Cargo.toml. Only that
  # line starts with `version = ` (dependency versions are mid-line), so the sed
  # match is unambiguous and needs no backslash escaping. Keeps meson, the binary
  # (CARGO_PKG_VERSION) and the git tag from ever drifting.
  version: run_command(
    'sed', '-n', 's/^version = //p', 'Cargo.toml',
    check: true,
  ).stdout().strip().replace('"', ''),
  meson_version: '>= 1.4',
  license: 'EUPL-1.2',
)

i18n = import('i18n')

prefix    = get_option('prefix')
bindir    = prefix / get_option('bindir')
datadir   = prefix / get_option('datadir')
localedir = prefix / get_option('localedir')

application_id  = 'io.github.tombreit.Glotze'
bin_name        = 'glotze'
gettext_package = bin_name

cargo    = find_program('cargo', required: true)
cargo_sh = find_program('build-aux' / 'cargo.sh')

# Pick the cargo profile from the meson buildtype. flatpak-builder may pass
# --buildtype=plain, so treat anything that isn't explicitly a debug build as
# release — we must never ship an unoptimized binary to users.
if get_option('buildtype').startswith('debug')
  cargo_profile = 'dev'
  rust_subdir   = 'debug'
else
  cargo_profile = 'release'
  rust_subdir   = 'release'
endif

cargo_target_dir = meson.project_build_root() / 'cargo-target'

custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,  # meson can't track cargo's inputs; let cargo decide
  console: true,
  output: bin_name,
  install: true,
  install_dir: bindir,
  # Picked up by build.rs and re-exported as rustc env vars so src/config.rs
  # can read them via option_env!. See gtk-rs.org/.../i18n.html.
  env: {
    'GETTEXT_PACKAGE': gettext_package,
    'LOCALEDIR'      : localedir,
    'APP_ID'         : application_id,
  },
  command: [
    cargo_sh,
    cargo,
    meson.project_source_root(),
    cargo_target_dir,
    cargo_profile,
    rust_subdir,
    bin_name,
    '@OUTPUT@',
  ],
)

subdir('data')
subdir('po')

gnome = import('gnome')
gnome.post_install(
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
