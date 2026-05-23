# Glotze

A GNOME-native desktop client for searching and downloading videos from German
public broadcaster Mediatheken (ARD, ZDF, 3sat, arte, …). Built with GTK 4 +
libadwaita 1.7+ in Rust. Streaming and playback are out of scope — Glotze hands
you the file and steps out of the way.

The data source is the public [MediathekViewWeb](https://mediathekviewweb.de/)
JSON API, which already aggregates every German public broadcaster, so Glotze
does not need to scrape per-channel sites.

> Status: milestone 1 — search + download. Filters, pagination, subtitles,
> and history are next.

---

## Quick start (development)

System packages (Debian/Ubuntu):

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev libssl-dev
```

Minimum versions verified against: GTK 4.14, libadwaita 1.7, Rust 1.92.

Build and run:

```sh
cargo run
```

Type a query (e.g. `Tagesschau`) into the search bar. After ~300 ms a list
appears. Click a row → choose a quality → switch to **Downloads** to see the
progress bar. Files land in your XDG `Videos` directory (`~/Videos`).

### Code quality

Standard Rust tooling, all configured at the repo root:

- **`cargo fmt`** — formatter (`rustfmt.toml`). Stable settings only.
- **`cargo clippy`** — linter at `pedantic` level with a narrow allowlist
  (`[lints.clippy]` in `Cargo.toml`).
- **`pre-commit`** (optional) — runs both at `git commit` time
  (`.pre-commit-config.yaml`). Install with
  `pipx install pre-commit && pre-commit install`.

CI (`.github/workflows/lint.yml`) runs `cargo fmt --check`, `cargo clippy
-- -D warnings`, and `cargo test` on every push and pull request. Install
the missing components locally with:

```sh
rustup component add rustfmt clippy
```

---

## Flatpak

Tooling:

```sh
sudo apt install flatpak-builder
flatpak install flathub \
    org.gnome.Sdk//49 \
    org.gnome.Platform//49 \
    org.freedesktop.Sdk.Extension.rust-stable//49
```

The extension branch tracks the runtime branch — when you bump the manifest
to GNOME 50 later, install `…rust-stable//50` to match.

Build + install for the current user:

```sh
flatpak-builder --user --install --force-clean build-dir \
    data/io.github.tombreit.Glotze.yml
flatpak run io.github.tombreit.Glotze
```

The manifest builds fully offline using the vendored `cargo-sources.json` at
the repo root. After any `Cargo.lock` change, refresh it with
`./scripts/update-cargo-sources.sh` and commit both files together. See
[`PUBLISHING.md`](PUBLISHING.md) for the Flathub-release workflow this
unlocks.

---

## Project layout

```
glotze/
├── Cargo.toml
├── COPYING                              # EUPL-1.2 license text
├── PUBLISHING.md                        # Flathub submission guide
├── cargo-sources.json                   # vendored crate manifest (offline builds)
├── rustfmt.toml                         # formatter settings
├── .pre-commit-config.yaml              # optional local hooks: fmt + clippy + file checks
├── .github/workflows/
│   ├── flatpak.yml                      # CI: build .flatpak bundle on tags
│   └── lint.yml                         # CI: cargo fmt + clippy + test on push/PR
├── scripts/
│   └── update-cargo-sources.sh          # regenerate cargo-sources.json from Cargo.lock
├── src/
│   ├── main.rs                          # env_logger setup, application launch
│   ├── application.rs                   # AdwApplication: actions, accels, About dialog
│   ├── window.rs                        # AppWindow: toolbar, view stack, all wiring
│   ├── api/
│   │   ├── mod.rs                       # Client: blocking POST to MediathekViewWeb
│   │   └── models.rs                    # Show, Quality, request/response (serde)
│   ├── download/
│   │   ├── mod.rs                       # Manager: spawns one streaming worker per download
│   │   └── progress.rs                  # Progress / State enum + slugify
│   └── ui/
│       ├── results_page.rs              # search results — ListBox in PreferencesGroup
│       ├── downloads_page.rs            # downloads — HashMap<id, row> with ProgressBar
│       ├── row.rs                       # search row: revealer + ToggleGroup quality picker
│       ├── logo.rs                      # per-channel SVG logo lookup
│       └── format.rs                    # human dates (jiff, Europe/Berlin) and durations
└── data/
    ├── io.github.tombreit.Glotze.desktop
    ├── io.github.tombreit.Glotze.metainfo.xml
    ├── io.github.tombreit.Glotze.yml    # flatpak manifest
    ├── channels/                        # per-broadcaster SVG logos
    └── icons/hicolor/scalable/apps/io.github.tombreit.Glotze.svg
```

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

## Concepts in this codebase (for the curious)

This is a deliberately small example of an idiomatic gtk-rs app. If you are
new to GTK / GNOME / Rust on the desktop, these are the load-bearing pieces:

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
  - `AdwPreferencesGroup` + `boxed-list` ListBox for the rounded result list
  - `AdwToggleGroup` for the per-row video quality picker
  - `AdwAboutDialog` for the About entry in the hamburger menu
  - `AdwToastOverlay` + `AdwToast` for transient feedback

- **GResource is intentionally absent.** UI is built in Rust code, with no
  Blueprint or `.ui` templates yet, so every widget is visible in the source.
  This is fine at this scale; for a larger app you'd compile UI XML into a
  resource bundle.

The
[`gtk4-rs` book](https://gtk-rs.org/gtk4-rs/stable/latest/book/) and the
[GNOME developer docs](https://developer.gnome.org/documentation/) are the two
references that cover everything above in depth.

---

## Key files to read first

If you want to follow how a search flows end-to-end through the code, read in
this order:

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
| `reqwest` (blocking) | HTTP client, streaming response body |
| `serde` + `serde_json` | MediathekViewWeb request/response models |
| `async-channel`      | progress fan-in from workers to UI |
| `jiff`               | timezone-aware date formatting (Europe/Berlin) |
| `anyhow`             | application-level error type |
| `log` + `env_logger` | logging (filterable with `RUST_LOG`) |

There is no `tokio`. `reqwest::blocking` brings its own per-call runtime,
which is fine inside `gio::spawn_blocking`.

---

## Acknowledgments

Glotze stands on the shoulders of:

- **[MediathekViewWeb](https://github.com/mediathekview/MediathekViewWeb)** —
  the public JSON API aggregating every German public broadcaster's catalogue.
  Without it Glotze would need to scrape per-channel sites.
- **[MediathekView](https://github.com/mediathekview)** — the umbrella project
  that has maintained the broadcast index and tooling for over a decade.
- **[Zapp](https://github.com/mediathekview/zapp)** — sibling Android client
  from the same family. Its API service served as the reference for the
  `Content-Type: text/plain` request quirk (see `src/api/mod.rs`).
- **[gtk-rs](https://gtk-rs.org/)** and the **libadwaita** team — the Rust
  bindings and the widget library this app is built on.
- **[Fractal](https://gitlab.gnome.org/World/fractal)**,
  **[Loupe](https://gitlab.gnome.org/GNOME/loupe)**,
  **[Bustle](https://gitlab.gnome.org/World/bustle)** and
  **[Gitte](https://codeberg.org/ckruse/Gitte)** — modern GNOME Rust apps
  consulted as references for project layout, Cargo feature gates, and the
  Flathub publishing workflow.
- **App icon** — the test-card centerpiece is
  [*TestChart similar to old TV testscreens*](https://commons.wikimedia.org/wiki/File:TestChart_similar_to_old_TV_testscreens.svg)
  from Wikimedia Commons, released under
  [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) (public domain
  dedication). Wrapped in a bezel by Glotze.

---

## License

Licensed under the European Union Public Licence v1.2 (EUPL-1.2). The full
text lives in [`COPYING`](COPYING). The EUPL is copyleft and lists AGPL-3.0,
GPL-3.0, LGPL, MPL-2.0 and others in its compatibility appendix, so derivative
works can be combined with code under those licenses where needed.
