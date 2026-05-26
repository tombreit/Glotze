#!/usr/bin/env bash
# Regenerate cargo-sources.json from Cargo.lock.
#
# Run this after every change to Cargo.toml / Cargo.lock so the Flathub build
# and the CI flatpak bundle keep working offline.
#
# Why is cargo-sources.json committed here at all? Flathub builds run with NO
# network, so every crate must be vendored ahead of time — this file is that
# vendoring manifest (generated from Cargo.lock). GNOME World apps on
# gitlab.gnome.org don't carry it because their nightly CI builds cargo online;
# their vendored file instead lives in the separate flathub/<app-id> repo (e.g.
# Fractal's flathub repo has a same-named update-cargo-sources.sh). Glotze has no
# nightly, so this repo doubles as its Flathub-prep repo and tracks the file here.
#
# The generator is pinned to a specific flatpak-builder-tools commit
# (GENERATOR_REF below) for reproducible output — bump it deliberately, then
# re-run this script and re-verify the offline flatpak build.
#
# The generator requires a few Python dependencies that aren't in the standard
# library. On a system Python this script creates a venv at /tmp/cargo-gen-venv
# and reuses it across runs.

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

# The generator emits no trailing newline, but the repo's end-of-file-fixer
# pre-commit hook wants one. Add it here so regenerating is a no-op diff rather
# than fighting the hook on every refresh.
[ -n "$(tail -c1 "$PROJECT_DIR/cargo-sources.json")" ] && echo >> "$PROJECT_DIR/cargo-sources.json"

echo "Done. Commit cargo-sources.json alongside the Cargo.lock change."
