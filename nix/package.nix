# Nix derivation for Glotze.
#
# Glotze is built with Meson, which drives Cargo through build-aux/cargo.sh.
# That wrapper goes `--offline --locked` whenever CARGO_NET_OFFLINE=true (set
# below), at which point Cargo resolves crates from the vendor directory that
# rustPlatform.cargoSetupHook wires into $CARGO_HOME/config.toml. We vendor
# straight from the committed Cargo.lock — *not* the Flatpak cargo-sources.json.
#
# Kept as a standalone callPackage-able file so it can be reused as-is should
# Glotze ever be submitted to nixpkgs.
{
  lib,
  stdenv,
  rustPlatform,
  meson,
  ninja,
  pkg-config,
  rustc,
  cargo,
  wrapGAppsHook4,
  gettext,
  desktop-file-utils,
  appstream,
  gtk4,
  libadwaita,
  glib,
  openssl,
}:

stdenv.mkDerivation (finalAttrs: {
  pname = "glotze";
  # Single source of truth: read [package] version straight from Cargo.toml,
  # the same value meson derives at build time. Bump Cargo.toml and everything
  # (binary, meson, this derivation, the release tag) follows automatically.
  version = (lib.importTOML ../Cargo.toml).package.version;

  src = lib.cleanSource ../.;

  cargoDeps = rustPlatform.importCargoLock {
    lockFile = ../Cargo.lock;
  };

  # Flip build-aux/cargo.sh onto its vendored/offline path.
  env.CARGO_NET_OFFLINE = "true";

  mesonBuildType = "release";

  nativeBuildInputs = [
    meson
    ninja
    pkg-config
    rustc
    cargo
    rustPlatform.cargoSetupHook
    wrapGAppsHook4
    gettext # msgfmt, for compiling po/
    desktop-file-utils # desktop-file-validate (optional meson test)
    appstream # appstreamcli (optional meson test)
  ];

  buildInputs = [
    gtk4
    libadwaita
    glib
    # ureq uses rustls, so OpenSSL is likely unused at runtime; kept to mirror
    # the CI lint deps and cover any transitive openssl-sys linkage. Safe to
    # drop if a build confirms it's unneeded.
    openssl
  ];

  meta = {
    description = "Search and download videos from public broadcaster Mediatheken (DACH region)";
    homepage = "https://github.com/tombreit/Glotze";
    license = lib.licenses.eupl12;
    mainProgram = "glotze";
    platforms = lib.platforms.linux;
  };
})
