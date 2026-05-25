project(
  'glotze',
  version: '0.0.7',
  meson_version: '>= 0.59.0',
  license: 'EUPL-1.2',
)

prefix  = get_option('prefix')
bindir  = prefix / get_option('bindir')
datadir = prefix / get_option('datadir')

application_id = 'io.github.tombreit.Glotze'
bin_name       = 'glotze'

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

gnome = import('gnome')
gnome.post_install(
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
