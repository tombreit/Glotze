#!/usr/bin/env bash

# Regenerate cargo-sources.json from Cargo.lock — the offline vendoring manifest
# Flathub's no-network build needs. Run after any Cargo.toml / Cargo.lock change.
#
# Wraps the standard flatpak-cargo-generator.py (flatpak-builder-tools), pinned
# to GENERATOR_REF for reproducible output and run from a cached venv. GNOME
# World apps keep this file in their flathub/<app-id> repo; Glotze has no
# nightly, so it lives here. Bump GENERATOR_REF deliberately, then re-verify the
# offline flatpak build.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
# Pinned flatpak-builder-tools commit (2026-05-22). Bump deliberately to update
# the generator; the cache filename embeds the ref so a bump forces a re-download.
GENERATOR_REF="96e2fe8bf7d2e5791ca1bdce2dba373f1e27c425"
GENERATOR="$SCRIPT_DIR/flatpak-cargo-generator-$GENERATOR_REF.py"
VENV="${CARGO_GEN_VENV:-/tmp/cargo-gen-venv}"

if [[ ! -f "$GENERATOR" ]]; then
    echo "Fetching flatpak-cargo-generator.py @ ${GENERATOR_REF:0:12}…"
    curl -sL -o "$GENERATOR" \
        "https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$GENERATOR_REF/cargo/flatpak-cargo-generator.py"
fi

if [[ ! -x "$VENV/bin/python" ]]; then
    echo "Creating venv at $VENV…"
    python3 -m venv "$VENV"
    "$VENV/bin/pip" install --quiet \
        'aiohttp<4.0.0,>=3.9.5' \
        'tomlkit>=0.13.3,<1.0'
fi

echo "Generating cargo-sources.json…"
"$VENV/bin/python" "$GENERATOR" \
    "$PROJECT_DIR/Cargo.lock" \
    -o "$PROJECT_DIR/cargo-sources.json"

# The generator writes the vendored-sources cargo config under the deprecated
# name `config`; patch that one dest-filename field so the build creates it as
# the modern `config.toml`.
sed -i 's/"dest-filename": "config"/"dest-filename": "config.toml"/' \
    "$PROJECT_DIR/cargo-sources.json"

echo "Done. Updated cargo-sources.json to match Cargo.lock change."
