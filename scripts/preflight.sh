#!/usr/bin/env bash
# Run the checks CI runs — locally, before you push or cut a release.
#
# Why this exists: lint.yml (fmt/clippy/test) does NOT run on tags, and
# flatpak.yml runs only *after* a tag is pushed. `cargo release` bumps, commits,
# tags and pushes in one shot, so a vX.Y.Z tag can be burned on code that then
# fails CI. Wiring this script into cargo-release's pre-release-hook (see
# Cargo.toml [package.metadata.release]) makes a failing release abort *before*
# the commit/tag/push. Run it by hand anytime, too.
#
# Modes:
#   ./scripts/preflight.sh            fast: fmt, clippy, test, manifest+appstream lint
#   ./scripts/preflight.sh --full     also: offline flatpak build + `flatpak-builder-lint repo`
#                                     (with screenshot mirroring — reproduces the CI "Lint repo"
#                                     step, including the appstream-*screenshot* checks)
#   ./scripts/preflight.sh --release  like --full, but first regenerates cargo-sources.json
#                                     (this is what cargo-release calls)
#
# --full/--release need the flatpak toolchain (flatpak-builder, the org.flatpak.Builder
# flatpak for the linter, a `flathub` remote, and the GNOME 50 runtime). The first full
# run downloads the runtime + Rust SDK extension and can take several minutes.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_DIR"

MANIFEST="build-aux/io.github.tombreit.Glotze.yml"
METAINFO="data/io.github.tombreit.Glotze.metainfo.xml"
MIRROR_URL="https://dl.flathub.org/media"

mode="fast"
case "${1:-}" in
    ""|--fast)  mode="fast" ;;
    --full)     mode="full" ;;
    --release)  mode="release" ;;
    -h|--help)
        sed -n '2,30p' "$0" | sed 's/^# \{0,1\}//'
        exit 0 ;;
    *)
        echo "preflight: unknown argument '$1' (use --fast, --full, --release or --help)" >&2
        exit 2 ;;
esac

step() { printf '\n\033[1m▶ %s\033[0m\n' "$*"; }
die()  { printf '\033[31mpreflight: %s\033[0m\n' "$*" >&2; exit 1; }

# Resolve the Flathub linter: native binary if present, else the org.flatpak.Builder
# flatpak (matches README's documented invocation).
if command -v flatpak-builder-lint >/dev/null 2>&1; then
    fbl() { flatpak-builder-lint "$@"; }
elif command -v flatpak >/dev/null 2>&1 && flatpak info org.flatpak.Builder >/dev/null 2>&1; then
    fbl() { flatpak run --command=flatpak-builder-lint org.flatpak.Builder "$@"; }
else
    die "need flatpak-builder-lint — install it with: flatpak install -y flathub org.flatpak.Builder"
fi

# ---- Rust + metadata (fast; always run) ------------------------------------

if [ "$mode" = "release" ]; then
    step "Regenerating cargo-sources.json (release)"
    ./scripts/update-cargo-sources.sh
fi

step "cargo fmt --check"
cargo fmt --all -- --check

step "cargo clippy (-D warnings)"
cargo clippy --all-targets --locked -- -D warnings

step "cargo test"
cargo test --all-targets --locked

step "flatpak-builder-lint manifest"
fbl manifest "$MANIFEST"

step "flatpak-builder-lint appstream"
fbl appstream "$METAINFO"

# The upstream validators that data/meson.build wires as `meson test` targets and
# that flatpak.yml runs in-sandbox via `run-tests: true`. Running them directly is
# version-independent (local flatpak-builder lacks a portable --run-tests) and fast.
DESKTOP="data/io.github.tombreit.Glotze.desktop"
if command -v appstreamcli >/dev/null 2>&1; then
    step "appstreamcli validate"
    appstreamcli validate --no-net --explain "$METAINFO"
else
    echo "preflight: appstreamcli not found — skipping metainfo validation" >&2
fi
if command -v desktop-file-validate >/dev/null 2>&1; then
    step "desktop-file-validate"
    desktop-file-validate "$DESKTOP"
else
    echo "preflight: desktop-file-validate not found — skipping .desktop validation" >&2
fi

if [ "$mode" = "fast" ]; then
    step "Fast preflight passed ✓  (run with --full to also build + lint the OSTree repo)"
    exit 0
fi

# ---- Offline build + repo lint (full/release; slow) ------------------------

command -v flatpak-builder >/dev/null 2>&1 || die "flatpak-builder not found (install flatpak-builder)"
flatpak remotes --columns=name 2>/dev/null | grep -qx flathub \
    || die $'no \'flathub\' remote. Add it with:\n  flatpak remote-add --if-not-exists --user flathub https://flathub.org/repo/flathub.flatpakrepo'

step "flatpak-builder: offline build + screenshot mirroring (slow; pulls the GNOME 50 runtime on first run)"
# Mirror screenshots+icons into the repo exactly like CI does. The two flags must
# go together: --mirror-screenshots-url fetches the metainfo URLs and commits a
# screenshots/<arch> ostree ref (auto since flatpak-builder 1.4.5), while
# --compose-url-policy=full rewrites the catalog to absolute dl.flathub.org/media
# URLs. With only the first, the catalog keeps relative paths and the repo lint
# still fails appstream-external-screenshot-url / appstream-remote-icon-not-mirrored.
# The flatpak-github-actions builder adds both for us in flatpak.yml; here we pass
# them by hand. (The in-sandbox meson tests CI runs via `run-tests: true` are
# covered above by the direct appstreamcli/desktop-file-validate calls — local
# flatpak-builder has no portable --run-tests flag.)
rm -rf build-dir repo .flatpak-builder
flatpak-builder --force-clean --repo=repo \
    --install-deps-from=flathub \
    --mirror-screenshots-url="$MIRROR_URL" \
    --compose-url-policy=full \
    build-dir "$MANIFEST"

step "flatpak-builder-lint repo  (the exact CI 'Lint repo' check)"
fbl repo repo

step "Preflight passed ✓"
