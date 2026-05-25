#!/usr/bin/env bash
# Build the glotze binary with cargo and hand it to meson.
#
# meson can't drive Cargo directly (its native Rust backend doesn't understand
# crates.io), so the top-level meson.build invokes this wrapper from a
# custom_target. It builds the crate and copies the resulting binary to the
# path meson asks for (@OUTPUT@), which meson then installs into $bindir.
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

ARGS=(build --manifest-path "$SOURCE_ROOT/Cargo.toml")

if [ "$PROFILE" = "release" ]; then
    ARGS+=(--release)
fi

# Offline path (Flathub / any build with vendored sources present): honour the
# vendored crates and refuse the network. The vendor config shipped by
# cargo-sources.json uses a *relative* `directory = "cargo/vendor"`, resolved
# against cargo's cwd — which is the meson build dir here, not the flatpak build
# root. Pin it to an absolute path so the offline build resolves regardless of
# where meson runs us from.
if [ "${CARGO_NET_OFFLINE:-}" = "true" ] || [ -d "${CARGO_HOME:-/nonexistent}/vendor" ]; then
    ARGS+=(--offline --locked)
    if [ -n "${CARGO_HOME:-}" ] && [ -d "$CARGO_HOME/vendor" ]; then
        ARGS+=(--config "source.vendored-sources.directory=\"$CARGO_HOME/vendor\"")
    fi
fi

"$CARGO" "${ARGS[@]}"

cp -f "$TARGET_DIR/$TARGET_SUBDIR/$BIN_NAME" "$OUTPUT"
