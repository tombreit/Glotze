#!/usr/bin/env bash
#
# Generate the Flathub submission manifest from the local build manifest.
#
# The repo's own manifest (build-aux/io.github.tombreit.Glotze.yml) builds from
# the working tree (`type: dir`, `path: ..`). Flathub has no working tree — it
# builds from a pinned git tag. This script takes the local manifest and rewrites
# only its `sources:` block into the `type: git` form Flathub requires, pinned to
# a release tag + its commit. Everything else (finish-args, build-options and all
# the explanatory comments a reviewer reads) is copied through verbatim.
#
# It writes a ready-to-submit directory containing the two files the Flathub PR
# needs at its repo root: the manifest and cargo-sources.json.
#
# Usage:
#   ./scripts/gen-flathub-manifest.sh [TAG]
#       TAG defaults to the most recent tag (`git describe --tags --abbrev=0`).
#
# Then copy build-aux/flathub/* into your fork of github.com/flathub/flathub
# (branch off `new-pr`, name the branch after the app ID). See PUBLISHING.md.

set -euo pipefail

APP_ID="io.github.tombreit.Glotze"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
SRC_MANIFEST="$REPO_ROOT/build-aux/$APP_ID.yml"
OUT_DIR="$REPO_ROOT/build-aux/flathub"

[ -f "$SRC_MANIFEST" ] || { echo "error: $SRC_MANIFEST not found" >&2; exit 1; }
[ -f "$REPO_ROOT/cargo-sources.json" ] || {
    echo "error: cargo-sources.json missing — run ./scripts/update-cargo-sources.sh" >&2
    exit 1
}

# Resolve the release tag and the commit it points at.
TAG="${1:-$(git -C "$REPO_ROOT" describe --tags --abbrev=0)}"
git -C "$REPO_ROOT" rev-parse -q --verify "refs/tags/$TAG" >/dev/null 2>&1 || {
    echo "error: tag '$TAG' does not exist locally — create/fetch it first" >&2
    exit 1
}
COMMIT="$(git -C "$REPO_ROOT" rev-parse "$TAG^{commit}")"

# Warn (don't fail) if the tag was never pushed — Flathub clones from the remote.
if ! git -C "$REPO_ROOT" ls-remote --tags origin "$TAG" 2>/dev/null | grep -q "$TAG"; then
    echo "warning: tag '$TAG' not found on origin — push it before submitting" >&2
fi

# Derive the HTTPS clone URL from origin (git@host:user/repo.git -> https://host/user/repo.git).
ORIGIN="$(git -C "$REPO_ROOT" remote get-url origin)"
URL="$(printf '%s' "$ORIGIN" | sed -E 's#^git@([^:]+):#https://\1/#')"
case "$URL" in *.git) ;; *) URL="$URL.git" ;; esac

mkdir -p "$OUT_DIR"
OUT_MANIFEST="$OUT_DIR/$APP_ID.yml"

# Copy the manifest through up to and including the module's `sources:` line, then
# emit the pinned git source in place of the working-tree `dir` source. `sources:`
# is the manifest's final block, so stopping there drops the old source verbatim.
awk -v url="$URL" -v tag="$TAG" -v commit="$COMMIT" '
  /^[[:space:]]+sources:[[:space:]]*$/ {
    print
    print "      # Flathub builds from a pinned release tag (no working tree)."
    print "      # Regenerate with scripts/gen-flathub-manifest.sh on each release."
    print "      - type: git"
    print "        url: " url
    print "        tag: " tag
    print "        commit: " commit
    print "      - cargo-sources.json"
    found = 1
    exit
  }
  { print }
  END { if (!found) { print "error: no sources: block found in manifest" > "/dev/stderr"; exit 1 } }
' "$SRC_MANIFEST" > "$OUT_MANIFEST"

cp -f "$REPO_ROOT/cargo-sources.json" "$OUT_DIR/cargo-sources.json"

echo "Generated Flathub submission files in build-aux/flathub/ (pinned to $TAG @ ${COMMIT:0:12}):"
echo "  - $APP_ID.yml"
echo "  - cargo-sources.json"
echo
echo "Next: copy both into your fork of github.com/flathub/flathub"
echo "      (branch off 'new-pr', branch named '$APP_ID'). See PUBLISHING.md."
