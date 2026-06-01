# Glotze

A GNOME-native desktop client for searching and downloading videos from
public broadcaster Mediatheken (DACH region), eg. ARD, ZDF, 3sat, arte,…
Built with GTK 4 + libadwaita 1.7+ in Rust. Streaming and playback are out of
scope — Glotze hands you the file and steps out of the way.

The data source is the public [MediathekViewWeb](https://mediathekviewweb.de/)
JSON API, which already aggregates DACH public broadcaster, so Glotze
does not need to scrape per-channel sites.

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

### Permissions

Verify the - minimal - set of requested permissions:

```sh
flatpak permission-show io.github.tombreit.Glotze
```

### Languages

To force a specific UI language (currently supported: `EN` and `DE`),
pass `LANGUAGE` into the sandbox — e.g. `--env=LANGUAGE=C` for English
(the source strings) or `--env=LANGUAGE=de` for German:
`flatpak run --env=LANGUAGE=de io.github.tombreit.Glotze`.

## Usage

Type a query (e.g. `Tagesschau`) into the search bar. Click a result row →
choose a quality → switch to **Downloads** to see the progress bar.

Files land in your XDG `Videos` directory (`~/Videos/Glotze`) (created if not exist).

Notes:

- Some channels or shows may be geo-restricted.
- Glotze doesn't have any settings (yet). If you'd rather set the download
location yourself, please [file an issue](https://github.com/tombreit/Glotze/issues).

## Uninstall

Uninstall Glotze completly:

```sh
flatpak uninstall --delete-data io.github.tombreit.Glotze
```

Glotze currently only saves a semaphore if you would like the welcome screen to
be displayed on every app start or not:
`~/.var/app/io.github.tombreit.Glotze/data/io.github.tombreit.Glotze/welcome-shown`

Your downloaded files are not affected by uninstalling Glotze.

## Development

```sh
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev libssl-dev
cargo run
```

Minimum versions verified against: GTK 4.14, libadwaita 1.7, Rust 1.92.

## Acknowledgments

- The inspiration for this Glotze app was the Android app **[Zapp](https://github.com/mediathekview/zapp)**
- Glotze gratefully uses the API from **[MediathekViewWeb](https://github.com/mediathekview/MediathekViewWeb)**
- The Glotze **App icon** — a [*TestChart similar to old TV testscreens*](https://commons.wikimedia.org/wiki/File:TestChart_similar_to_old_TV_testscreens.svg)
  from Wikimedia Commons, released under
  [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/)
- Code, implementation and tooling inspiration:
  - **[Gitte](https://codeberg.org/ckruse/Gitte)**
  - **[Fractal](https://gitlab.gnome.org/World/fractal)**
  - **[Shortwave](https://gitlab.gnome.org/World/Shortwave)**
- Tech stack:
  - **[gtk-rs](https://gtk-rs.org/)**
  - **[Flatpak](https://flatpak.org/)**

## License

Licensed under the European Union Public Licence v1.2 (EUPL-1.2). The full text
lives in [`LICENSE`](LICENSE). The EUPL is copyleft and lists AGPL-3.0, GPL-3.0,
LGPL, MPL-2.0 and others in its compatibility appendix, so derivative works can
be combined with code under those licenses where needed.
