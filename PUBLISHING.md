# Publishing & releasing Glotze

Glotze ships on Flathub. The app ID `io.github.tombreit.Glotze` is load-bearing —
it ties together the binary, the `.desktop` file, the AppStream metainfo, the
icon, and the Flatpak sandbox. Don't change it.

Most GNOME apps have GNOME Nightly CI and a separate `flathub/<app-id>` packaging
repo. Glotze has no nightly pipeline, so **this repo doubles as the Flathub-prep
repo**: the manifest, the vendored sources, and the validators all live here.
Flathub's own docs: <https://docs.flathub.org/docs/for-app-authors/submission>.

The day-to-day command cheat-sheet (run, build, validate, deps) is in the
README's *Maintainer commands*. This file covers the parts unique to
distribution: **how it builds → validate → release → submit → maintain**.

---

## How it builds

meson (`meson.build`) is the outer build system; it drives cargo through
`build-aux/cargo.sh` and installs the binary plus the data files. The Flatpak
build runs **offline** — Flathub forbids network access at build time — using the
vendored `cargo-sources.json` (generated from `Cargo.lock` by
`scripts/update-cargo-sources.sh`, pinned to a known generator commit). The
manifest (`build-aux/io.github.tombreit.Glotze.yml`) is `buildsystem: meson` and
adds `cargo-sources.json` as a second source. The version comes from `Cargo.toml`
(meson reads it at configure time; the binary uses `CARGO_PKG_VERSION`).

Refresh the vendored sources only when dependencies change:

```sh
./scripts/update-cargo-sources.sh   # regenerates cargo-sources.json from Cargo.lock
```

---

## Validate

Run these before tagging or submitting — CI (`.github/workflows/flatpak.yml`)
runs all of them too.

```sh
# Metadata — also wired as `meson test` targets and run during the Flatpak build:
meson setup build
meson test -C build                 # appstreamcli validate + desktop-file-validate

# Flathub's own linter (org.flatpak.Builder provides it; CI uses the bundled binary):
flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
    manifest build-aux/io.github.tombreit.Glotze.yml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
    appstream data/io.github.tombreit.Glotze.metainfo.xml
```

`appstreamcli` and the linter treat warnings as fatal for Flathub — fixing them
here saves a review round-trip. After an offline build into a local repo
(`flatpak-builder --user --force-clean --repo=repo build-dir <manifest>`) you can
also lint the result with `flatpak-builder-lint repo repo`.

---

## Cutting a release

`Cargo.toml`'s `version` is the source of truth; the git tag is `v` + that
version. Most releases don't change dependencies, so they don't touch
`cargo-sources.json`.

```sh
# 1. Bump the version — this one edit drives everything (meson + the binary follow):
#      Cargo.toml  ->  version = "X.Y.Z"
cargo update -p glotze --offline          # 2. sync Cargo.lock
# 3. ONLY if dependencies changed:  ./scripts/update-cargo-sources.sh
# 4. Prepend a <release> to data/io.github.tombreit.Glotze.metainfo.xml:
#      <release version="X.Y.Z" date="YYYY-MM-DD">
#        <url>https://github.com/tombreit/Glotze/releases/tag/vX.Y.Z</url>
#      </release>
git commit -am "Release X.Y.Z"            # 5. (cargo-sources.json too, if step 3 ran)
git tag vX.Y.Z                            #    the tag MUST equal v + the Cargo.toml version
git push origin main vX.Y.Z               # 6. push
```

Pushing the tag triggers `flatpak.yml`: it lints the manifest, builds the bundle
offline, runs the metadata tests, then **publishes** a GitHub release with
`glotze.flatpak` attached and notes auto-generated from the commit log. Testers
install that asset using the commands in the README's *Install* section. (Tag an
`-rc` like `v0.1.0-rc1` for a pre-release testflight; `flatpak.yml` fires on any
`v*` tag.)

The metainfo entry can stay minimal (version + date + url); add a `<description>`
only when a release is worth a changelog line — Flathub renders it on the listing.

**Shortcut:** `cargo release patch -x --no-confirm` does steps 1–6 in one command
(config in `Cargo.toml` under `[package.metadata.release]`; `cargo install
cargo-release` first). Run `cargo release patch` alone for a dry run. If a run
leaves files modified but uncommitted, just `git commit`/`tag`/`push` them — and
never move an existing remote tag (delete and re-tag instead; the release-asset
logic keys on the tag name).

---

## Submitting to Flathub (first time)

1. **Finish the metainfo** (`data/io.github.tombreit.Glotze.metainfo.xml`): at
   least one `<screenshot>` with a real HTTPS URL that resolves (see
   `data/screenshots/README.md`), plus `<update_contact>`, the `bugtracker` and
   `vcs-browser` URLs, and a `<release>` matching `Cargo.toml`. Validate (above).
2. **Open the PR** against <https://github.com/flathub/flathub> on the `new-pr`
   branch, in a branch named after the app ID, adding at the repo root:
   - the manifest — **with the `dir` source swapped for a `git` source pinned to
     the release tag** (see *Maintenance loop*); Flathub has no working tree.
   - `cargo-sources.json`
   - optionally a `flathub.json` (Flathub builds x86_64 + aarch64 by default).
3. The Flathub bot builds and lints your manifest; a human reviewer checks the
   `finish-args` and metainfo. On merge, `flathub/io.github.tombreit.Glotze` is
   created and you get write access — from then on it's the maintenance loop.

---

## Maintenance loop

Shipping an update is **cut a release** (above), then propagate the tag to the
Flathub repo: point the manifest's source at the new tag, drop in the matching
`cargo-sources.json`, and push to `master` — Flathub builds and publishes within
the hour. The source stanza is the only thing that differs between the two repos:

```yaml
# in THIS repo (builds the working tree)   |   # in the Flathub repo (reproducible)
sources:                                   |   sources:
  - type: dir                              |     - type: git
    path: ..                               |       url: https://github.com/tombreit/Glotze.git
                                           |       tag: vX.Y.Z
```

Regenerate `cargo-sources.json` only if dependencies changed since the previous
release; copy the current file alongside the manifest either way. Pin to a **tag**,
never `main`.

---

## Checklist before opening the first PR

- [ ] `meson test -C build` passes (appstreamcli + desktop-file-validate)
- [ ] `flatpak-builder-lint manifest …` and `… appstream …` are clean (or exceptioned)
- [ ] At least one `<screenshot>` with an HTTPS URL that resolves
- [ ] `<update_contact>` and the `bugtracker` + `vcs-browser` URLs set
- [ ] `<releases>` top entry matches the `Cargo.toml` version
- [ ] Offline `flatpak-builder` build succeeds and the app runs under `flatpak run`
- [ ] Tag `vX.Y.Z` equals `v` + the `Cargo.toml` version

## Caveats

- **Sandbox permissions are the most-reviewed thing.** Glotze's set
  (`--share=network`/`ipc`, `--socket=wayland`/`fallback-x11`, `--device=dri`,
  `--filesystem=xdg-videos`) maps 1:1 to what the app does. Don't add more —
  `--filesystem=home` is flagged instantly.
- **Never put `--share=network` in `build-args`.** Flathub builds are offline by
  policy; the vendored `cargo-sources.json` is how cargo works without the network.
- **Screenshots must resolve** over HTTPS at review time — Flathub mirrors them
  but doesn't host them. PNG, ≥1280×720, 16:9, light theme.
- **First-time submitters get extra scrutiny.** Be responsive and follow the
  reviewer's links to the spec.
