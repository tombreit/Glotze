# Nix derivation for Glotze.
#
# Glotze is built with Meson, which drives `cargo build --locked` through
# build-aux/cargo.sh. Cargo resolves crates from the vendor directory that
# rustPlatform.cargoSetupHook wires into its config. We vendor straight from the
# committed Cargo.lock — *not* the Flatpak cargo-sources.json.
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
  librsvg,
  hicolor-icon-theme,
}:

stdenv.mkDerivation (finalAttrs: {
  pname = "glotze";
  # Single source of truth: read [package] version straight from Cargo.toml,
  # the same value meson derives at build time. Bump Cargo.toml and everything
  # (binary, meson, this derivation, the release tag) follows automatically.
  version = (lib.importTOML ../Cargo.toml).package.version;

  src = lib.cleanSource ../.;

  # meson runs build-aux/cargo.sh during the build, but its `#!/usr/bin/env bash`
  # shebang can't resolve in the Nix sandbox (no /usr/bin/env), so the cargo-build
  # target dies with exit 127. patchShebangs only runs automatically on $out, not
  # on build-time scripts in the source tree — rewrite it to the store bash here.
  postPatch = ''
    patchShebangs build-aux/cargo.sh
  '';

  cargoDeps = rustPlatform.importCargoLock {
    lockFile = ../Cargo.lock;
  };

  # Cargo honours this natively: never touch the network, use the vendored crates.
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
    # gdk-pixbuf SVG loader: lets GTK rasterize the app's full-color hicolor SVG
    # icon. wrapGAppsHook4 detects it and wires GDK_PIXBUF_MODULE_FILE into the
    # wrapper; without it the window/taskbar icon falls back to a generic one.
    # (Symbolic UI icons render fine regardless — GTK4 draws those internally.)
    librsvg
    # Base hicolor index.theme so GTK recognizes the scalable/apps directory the
    # icon is installed into. Likely transitive, but explicit keeps the icon path
    # self-contained.
    hicolor-icon-theme
  ];

  meta = {
    description = "Search and download videos from public broadcaster Mediatheken (DACH region)";
    homepage = "https://tombreit.github.io/Glotze/";
    license = lib.licenses.eupl12;
    mainProgram = "glotze";
    platforms = lib.platforms.linux;
  };
})
