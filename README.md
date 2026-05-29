# Glotze

A GNOME-native desktop client for searching and downloading videos from German
public broadcaster Mediatheken (ARD, ZDF, 3sat, arte, …). Built with GTK 4 +
libadwaita 1.7+ in Rust. Streaming and playback are out of scope — Glotze hands
you the file and steps out of the way.

The data source is the public [MediathekViewWeb](https://mediathekviewweb.de/)
JSON API, which already aggregates every German public broadcaster, so Glotze
does not need to scrape per-channel sites.

---

## Install

Each [GitHub release](https://github.com/tombreit/Glotze/releases) attaches a
`glotze.flatpak` bundle. The `/releases/latest/download/` URL is stable, so you
can fetch the most recent build without visiting the page:

```sh
# Download, install, run.
curl -LO https://github.com/tombreit/Glotze/releases/latest/download/glotze.flatpak
flatpak install --user ./glotze.flatpak
flatpak run io.github.tombreit.Glotze
```

To force a specific UI language, pass `LANGUAGE` into the sandbox — e.g.
`--env=LANGUAGE=C` for English (the source strings) or `--env=LANGUAGE=de` for
German: `flatpak run --env=LANGUAGE=de io.github.tombreit.Glotze`.

---

## Quick start (development)

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev libssl-dev
cargo run
```

Minimum versions verified against: GTK 4.14, libadwaita 1.7, Rust 1.92.

Type a query (e.g. `Tagesschau`) into the search bar. After ~300 ms a list
appears. Click a row → choose a quality → switch to **Downloads** to see the
progress bar. Files land in your XDG `Videos` directory (`~/Videos/Glotze`).

For the build system, code conventions, project layout, and an architecture
walkthrough, see [`CONTRIBUTING.md`](CONTRIBUTING.md). For cutting releases and
the Flathub submission workflow, see [`PUBLISHING.md`](PUBLISHING.md).

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

Licensed under the European Union Public Licence v1.2 (EUPL-1.2). The full text
lives in [`COPYING`](COPYING). The EUPL is copyleft and lists AGPL-3.0, GPL-3.0,
LGPL, MPL-2.0 and others in its compatibility appendix, so derivative works can
be combined with code under those licenses where needed.
