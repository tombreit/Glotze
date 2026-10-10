#!/usr/bin/env bash
# Build the glotze binary with cargo and hand it to meson.
#
# meson can't drive Cargo directly (its native Rust backend doesn't understand
# crates.io), so the top-level meson.build invokes this wrapper from a
# custom_target. It builds the crate and copies the resulting binary to the
# path meson asks for (@OUTPUT@), which meson then installs into $bindir.
#
# Offline builds (Flatpak, Nix) need no special handling here: they set
# CARGO_NET_OFFLINE=true, which cargo honours itself, and their vendored-sources
# config under $CARGO_HOME redirects crates.io (its relative `directory` is
# resolved against the config file's location, not the cwd).
#
# Positional args (passed by meson.build, in order):
#   $1 CARGO         path to the cargo executable
#   $2 SOURCE_ROOT   crate root (directory containing Cargo.toml)
#   $3 TARGET_DIR    CARGO_TARGET_DIR (under the meson build dir)
#   $4 PROFILE       'release' or 'dev'
#   $5 TARGET_SUBDIR cargo's output subdir name ('release' or 'debug')
#   $6 BIN_NAME      'glotze'
#   $7 OUTPUT        meson @OUTPUT@ path to copy the built binary to

set -euo pipefail

CARGO="$1"
SOURCE_ROOT="$2"
TARGET_DIR="$3"
PROFILE="$4"
TARGET_SUBDIR="$5"
BIN_NAME="$6"
OUTPUT="$7"

export CARGO_TARGET_DIR="$TARGET_DIR"

ARGS=(build --manifest-path "$SOURCE_ROOT/Cargo.toml" --locked)
if [ "$PROFILE" = "release" ]; then
    ARGS+=(--release)
fi

"$CARGO" "${ARGS[@]}"

cp -f "$TARGET_DIR/$TARGET_SUBDIR/$BIN_NAME" "$OUTPUT"
