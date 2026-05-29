# Contributing to Glotze

Glotze is a deliberately small, idiomatic GTK 4 + libadwaita app written in
Rust. This document is the developer's home: how to build it, the conventions it
follows, and how the code is laid out. For cutting releases and the Flathub
submission workflow, see [`PUBLISHING.md`](PUBLISHING.md).

The two upstream references that cover everything here in depth are the
[`gtk4-rs` book](https://gtk-rs.org/gtk4-rs/stable/latest/book/) and the
[GNOME developer documentation](https://developer.gnome.org/documentation/).
For UI/UX decisions, follow the [GNOME Human Interface
Guidelines](https://developer.gnome.org/hig/).

---

## Build & run

For day-to-day work just use cargo:

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev libssl-dev
cargo run
```

Minimum versions verified against: GTK 4.14, libadwaita 1.7, Rust 1.92.

The shippable build is driven by **meson** (`meson.build` → `build-aux/cargo.sh`
→ cargo), which installs the binary plus the data files and single-sources the
version from `Cargo.toml`. A local `meson`/`ninja` or `cargo run` build is
**online**; only the Flatpak path (`CARGO_NET_OFFLINE=true`) uses the vendored
`cargo-sources.json`. See the [gtk4-rs meson
chapter](https://gtk-rs.org/gtk4-rs/stable/latest/book/meson.html) for the
pattern this follows.

---

## Code quality

Standard Rust tooling, all configured at the repo root:

- **`cargo fmt`** — formatter (`rustfmt.toml`). Stable settings only.
- **`cargo clippy`** — linter at `pedantic` level with a narrow allowlist
  (`[lints.clippy]` in `Cargo.toml`).
- **`pre-commit`** (optional) — runs both at `git commit` time
  (`.pre-commit-config.yaml`). Install with
  `pipx install pre-commit && pre-commit install`.

CI (`.github/workflows/lint.yml`) runs `cargo fmt --check`, `cargo clippy
-- -D warnings`, and `cargo test` on every push and pull request. Install the
missing components locally with:

```sh
rustup component add rustfmt clippy
```

The test suite is intentionally small: pure-logic helpers (e.g. filename
slugification in `src/download/progress.rs`) have unit tests; the GTK widget
layer is exercised by manual smoke testing before each release.

---

## Testing translations

`cargo run` skips meson and so doesn't install the gettext `.mo` catalogues,
leaving the UI English. To see the German UI (or any other locale shipped under
`po/`), install through meson into a user-writable prefix and run from there —
the [gtk4-rs book i18n
chapter](https://gtk-rs.org/gtk4-rs/stable/latest/book/i18n.html) recommends
`~/.local`:

```sh
meson setup -Dprefix="$HOME/.local" build      # one-time
meson install -C build                          # no sudo
LANGUAGE=de glotze                              # ~/.local/bin is usually on $PATH
```

If `build/` was already configured with the default prefix, swap it in place
with `meson configure -Dprefix="$HOME/.local" build` — that preserves the cargo
target cache. `LANGUAGE=de` overrides the message language only;
`LC_ALL=de_DE.UTF-8 glotze` is the foolproof fallback if your system locale is
set to `C`.

---

## Maintainer commands

Every entrypoint for developing, validating, and shipping the app. The detailed
release and Flathub workflow lives in [`PUBLISHING.md`](PUBLISHING.md).

| Task | Command |
|---|---|
| Run (dev) | `cargo run` |
| Format · lint · test | `cargo fmt` · `cargo clippy` · `cargo test` (or `pre-commit run -a`) |
| Build via meson (online) | `meson setup build && ninja -C build` |
| Run installed locally (no sudo) | `meson install -C build` after a `--prefix="$HOME/.local"` setup — see [Testing translations](#testing-translations) |
| Validate metadata | `meson test -C build` (appstreamcli + desktop-file-validate) |
| Build the Flatpak (offline, like Flathub) | `flatpak-builder --user --install --force-clean build-dir build-aux/io.github.tombreit.Glotze.yml` |
| Lint for Flathub | `flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest build-aux/io.github.tombreit.Glotze.yml` |
| Refresh vendored deps (only when deps change) | `./scripts/update-cargo-sources.sh` |
| Preflight (full local CI before push/tag) | `./scripts/preflight.sh --full` (fmt·clippy·test + manifest/appstream lint + offline flatpak build + `flatpak-builder-lint repo`; drop `--full` for the fast checks only) |
| Cut a release | bump `Cargo.toml` + a metainfo `<release>`, tag `vX.Y.Z`, push → [PUBLISHING.md](PUBLISHING.md#cutting-a-release) |

### Flatpak prerequisites

One-time SDK install for local Flatpak builds and the Flathub linter:

```sh
sudo apt install flatpak-builder
flatpak install flathub \
    org.gnome.Sdk//50 \
    org.gnome.Platform//50 \
    org.freedesktop.Sdk.Extension.rust-stable \
    org.flatpak.Builder
```

The runtime/SDK branch (`50`) tracks the manifest; bump both together when a
newer GNOME runtime lands. `org.flatpak.Builder` provides `flatpak-builder-lint`.

---

## Project layout

```
glotze/
├── Cargo.toml
├── meson.build                              # outer build: drives cargo, installs files
├── COPYING                                  # EUPL-1.2 license text
├── PUBLISHING.md                            # Flathub submission guide
├── cargo-sources.json                       # vendored crate manifest (offline builds)
├── rustfmt.toml                             # formatter settings
├── .pre-commit-config.yaml                  # optional local hooks: fmt + clippy + file checks
├── .github/workflows/
│   ├── flatpak.yml                          # CI: build .flatpak bundle on tags
│   └── lint.yml                             # CI: cargo fmt + clippy + test on push/PR
├── build-aux/
│   ├── io.github.tombreit.Glotze.yml        # flatpak manifest (buildsystem: meson)
│   └── cargo.sh                             # meson → cargo wrapper (offline-aware)
├── scripts/
│   └── update-cargo-sources.sh              # regenerate cargo-sources.json from Cargo.lock
├── po/                                      # gettext catalogues (LINGUAS, POTFILES.in, de.po)
├── src/
│   ├── main.rs                              # env_logger setup, application launch
│   ├── application.rs                       # AdwApplication: actions, accels, About dialog
│   ├── window.rs                            # AppWindow: toolbar, view stack, all wiring
│   ├── api/
│   │   ├── mod.rs                           # Client: blocking POST to MediathekViewWeb
│   │   └── models.rs                        # Show, Quality, request/response (serde)
│   ├── download/
│   │   ├── mod.rs                           # Manager: spawns one streaming worker per download
│   │   └── progress.rs                      # Progress / State enum + slugify
│   └── ui/
│       ├── results_page.rs                  # search results — ListBox in PreferencesGroup
│       ├── downloads_page.rs                # downloads — HashMap<id, row> with ProgressBar
│       ├── row.rs                           # search row: revealer + ToggleGroup quality picker
│       ├── logo.rs                          # per-channel SVG logo lookup
│       └── format.rs                        # human dates (jiff, Europe/Berlin) and durations
└── data/
    ├── meson.build                          # installs the files below + validates metadata
    ├── io.github.tombreit.Glotze.desktop.in
    ├── io.github.tombreit.Glotze.metainfo.xml.in
    ├── channels/                            # per-broadcaster SVG logos
    └── icons/hicolor/scalable/apps/io.github.tombreit.Glotze.svg
```

The `.in` suffix on the desktop and metainfo files means they are templates:
meson's `i18n.merge_file` runs them through `po/` and produces the final
`.desktop` / `.metainfo.xml` under `build/` at install time. Always edit the
`.in` source, never a generated file.

---

## MediathekViewWeb API

Glotze talks to a single endpoint, `https://mediathekviewweb.de/api/query`. It's a
`POST` whose body is JSON but — quirk of the service — must be sent with
`Content-Type: text/plain`. Free-text queries match `title` and `topic`; an empty
`queries` array returns the most recent broadcasts (the cold-start view).

Request body Glotze sends (see `src/api/mod.rs`):

```jsonc
{
  "queries": [{ "fields": ["title", "topic"], "query": "Tatort" }],
  "sortBy": "timestamp",   // or "duration" — see the Sort enum
  "sortOrder": "desc",     // or "asc"
  "future": false,         // exclude not-yet-aired entries
  "offset": 0,
  "size": 30
}
```

Response (truncated to one result; `result.results[]` deserializes into `Show`):

```jsonc
{
  "result": {
    "results": [
      {
        "channel": "SWR",
        "topic": "Tatort",
        "title": "Die große Angst",
        "description": "Mitten in den Schwarzwald …",
        "timestamp": 1779912000,
        "duration": 5350,
        "size": 1638924288,
        "url_website": "https://www.ardmediathek.de/video/Y3JpZDov…",
        "url_subtitle": "https://api.ardmediathek.de/player-service/subtitle/…",
        "url_video": "https://swr-pd.ard-mcdn.de/swr/video/tatort/2212490.avc-720.mp4",
        "url_video_low": "https://…/2212490.avc-360.mp4",
        "url_video_hd": "https://…/2212490.avc-1080.mp4",
        "filmlisteTimestamp": 1779896100,
        "id": "bW40B5S0C5jiyUTaa9Jg25Ukrif65Mst9kInYpcS8Zg="
      }
    ],
    "queryInfo": { "resultCount": 1, "totalResults": 1822, "totalEntries": 696752 }
  },
  "err": null
}
```

`url_video` is the medium-quality progressive MP4; `_low`/`_hd` are the other two
rungs. Glotze skips HLS (`.m3u8`) variants — it hands you a file, not a stream.

---

## Architecture

```
+----------------------------------------------------------+
|  AdwApplication (GLib main loop, single UI thread)       |
|   ├─ app.about ──► AdwAboutDialog                        |
|   └─ app.quit, win.search-focus, win.show-{search,…}     |
|                                                          |
|   AdwApplicationWindow                                   |
|    AdwToolbarView                                        |
|     AdwHeaderBar  [ AdwViewSwitcher  |  MenuButton ]     |
|     AdwViewStack                                         |
|       ├── "search"    ──► ResultsPage (ListBox)          |
|       └── "downloads" ──► DownloadsPage (ListBox)        |
|    AdwToastOverlay  (errors and completion notices)      |
|                                                          |
|  api::Client     ── gio::spawn_blocking ──► reqwest      |
|  download::Mgr   ── gio::spawn_blocking ──► reqwest      |
|                     progress via async_channel           |
+----------------------------------------------------------+
```

One UI thread; blocking HTTP runs on GLib worker threads. Results and progress
flow back via `async_channel`, consumed by `MainContext::spawn_local` futures
on the main loop, which is the only place GTK widgets may be mutated.

---

## Concepts in this codebase

This is a deliberately small example of an idiomatic gtk-rs app. If you are new
to GTK / GNOME / Rust on the desktop, these are the load-bearing pieces:

- **`AdwApplication` and the activate signal** (`application.rs`). One process
  per app ID; `activate` fires once per launch and builds the window.

- **The application ID** (`io.github.tombreit.Glotze`, defined in `main.rs`).
  Reverse-DNS string that ties together the binary, the `.desktop` file, the
  AppStream metainfo, the icon, and the Flatpak sandbox.

- **`MainContext` and the async pattern** (`window.rs:run_search`,
  `download/mod.rs:enqueue`). All UI mutation happens on the main thread.
  Blocking I/O is moved to a worker thread with `gtk::gio::spawn_blocking(|| …)`
  — which returns a `Future` — and awaited from a `MainContext::spawn_local`
  task. Nothing else is needed; there is no `tokio` runtime in this project.

- **`async_channel` for fan-in** (`download/mod.rs`, `window.rs:wire_progress_consumer`).
  N download workers each push `Progress` events through one channel; a single
  consumer on the main loop drains it and updates the UI.

- **`glib::clone!` macro**. The way to capture state into a long-lived signal
  handler without leaking. Use `#[weak]` to avoid keeping the widget alive past
  its parent; `#[strong]` for owned data like `Rc<Manager>`.

- **GActions and accelerators** (`application.rs`, `window.rs:install_window_actions`).
  Behaviour is exposed as `gio::SimpleAction`s (`app.about`, `app.quit`,
  `win.search-focus`, `win.show-search`, `win.show-downloads`) and bound to
  keyboard shortcuts via `set_accels_for_action`. The hamburger menu and any
  future menubar entries point at the same action names.

- **libadwaita patterns used here**:
  - `AdwToolbarView` instead of a plain `GtkBox` for chrome layout
  - `AdwViewStack` + `AdwViewSwitcher` for the search/downloads switch
  - `GtkListBox` with the `.boxed-list` style class for the rounded result list
  - `AdwToggleGroup` for the per-row video quality picker
  - `AdwAboutDialog` for the About entry in the hamburger menu
  - `AdwToastOverlay` + `AdwToast` for transient feedback

- **GResource is intentionally absent.** UI is built in Rust code, with no
  Blueprint or `.ui` templates yet, so every widget is visible in the source.

### Key files to read first

To follow how a search flows end-to-end through the code, read in this order:

1. `src/main.rs` — process entry
2. `src/application.rs` — how the window is created on `activate`
3. `src/window.rs` — `wire_search` (debounce + spawn_blocking),
   `wire_row_action` (enqueue / cancel / open driven by a per-row state
   machine), and `wire_progress_consumer` (channel → page, with weak refs
   so the future doesn't outlive the window)
4. `src/api/mod.rs` — the single POST request to MediathekViewWeb
5. `src/download/mod.rs` — the streaming download worker

Everything else is widget plumbing.

---

## Dependencies (what each one is for)

| Crate                | Why |
|----------------------|-----|
| `gtk4`               | GTK 4 widget bindings |
| `libadwaita`         | GNOME HIG widget kit on top of GTK 4 |
| `reqwest` (blocking) | HTTP client, streaming response body (rustls TLS — no system OpenSSL) |
| `serde` + `serde_json` | MediathekViewWeb request/response models |
| `async-channel`      | progress fan-in from workers to UI |
| `jiff`               | timezone-aware date formatting (Europe/Berlin) |
| `anyhow`             | application-level error type |
| `log` + `env_logger` | logging (filterable with `RUST_LOG`) |
| `gettext-rs`         | i18n via system libintl (`gettext-system` feature) |

There is no `tokio`. `reqwest::blocking` brings its own per-call runtime,
which is fine inside `gio::spawn_blocking`.
