#!/usr/bin/env bash
# Regenerate cargo-sources.json from Cargo.lock.
#
# Run this after every change to Cargo.toml / Cargo.lock so the Flathub build
# and the CI flatpak bundle keep working offline.
#
# The generator requires a few Python dependencies that aren't in the standard
# library. On a system Python this script creates a venv at /tmp/cargo-gen-venv
# and reuses it across runs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
GENERATOR="$SCRIPT_DIR/flatpak-cargo-generator.py"
VENV="${CARGO_GEN_VENV:-/tmp/cargo-gen-venv}"

if [[ ! -f "$GENERATOR" ]]; then
    echo "Fetching flatpak-cargo-generator.py…"
    curl -sL -o "$GENERATOR" \
        https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
fi

if [[ ! -x "$VENV/bin/python" ]]; then
    echo "Creating venv at $VENV…"
    python3 -m venv "$VENV"
    "$VENV/bin/pip" install --quiet \
        'aiohttp>=3.9.5,<4' \
        'tomlkit>=0.13.3,<1' \
        'PyYAML>=6.0.2,<7'
fi

echo "Generating cargo-sources.json…"
"$VENV/bin/python" "$GENERATOR" \
    "$PROJECT_DIR/Cargo.lock" \
    -o "$PROJECT_DIR/cargo-sources.json"

# Modern cargo prefers config.toml over the legacy "config" filename.
sed -i 's/"dest-filename": "config"/"dest-filename": "config.toml"/' \
    "$PROJECT_DIR/cargo-sources.json"

echo "Done. Commit cargo-sources.json alongside the Cargo.lock change."
