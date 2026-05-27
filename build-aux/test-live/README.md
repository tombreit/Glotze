# `test-live.sh` — local "live environment" smoke test

On-demand, local-only sanity check for the Glotze Flatpak across the environment
axes that actually break GTK4 apps on other people's machines. No CI, no VMs, no
cloud cost — just run it on your notebook before a release.

The script lives one level up: [`../test-live.sh`](../test-live.sh).

## Why this exists (and why it's so small)

A Flatpak bundles the entire GUI userspace — `org.gnome.Platform//50` carries GTK4,
libadwaita, and the Mesa GL loaders. So the host **distribution** barely changes
anything: Ubuntu and Fedora run the *same* GTK from our runtime. What genuinely
differs across a live host, and what actually crashes GTK4 apps, is a narrow set of
axes — and those are all reproducible locally with env vars + a headless X server:

1. **Display server** — Wayland vs X11 (our `--socket=fallback-x11`).
2. **GSK renderer / GPU** — GTK ≥4.14 defaults to the **Vulkan** renderer, which is
   the #1 "works on my machine" crash on llvmpipe / old Mesa / VMs. We sweep
   `vulkan`, `ngl`, `gl`, and `cairo` (pure software), plus an llvmpipe run that
   mimics a weak / emulated GPU.
3. **Display scaling** — HiDPI and fractional scaling commonly clip or misalign
   widgets. We sweep `GDK_SCALE` (integer buffer scale) × `GDK_DPI_SCALE`
   (fractional text/DPI scale).

Glotze only **downloads** (no video decode/playback), so the whole risk surface is
"does the UI come up, render, and lay out without crashing." That's exactly what each
cell asserts.

## Usage

```sh
# Test the currently installed --user app:
build-aux/test-live.sh

# Install a specific bundle into the --user installation, then test it:
build-aux/test-live.sh path/to/glotze.flatpak

# Build + install from the manifest, then test (compiles Rust — slow):
build-aux/test-live.sh --build

# Options:
#   --settle N   seconds each launch runs before being judged (default 6)
#   -h | --help
```

Requirements (all already present on a typical GNOME dev box):
`flatpak`, `xvfb-run` (pkg `xvfb`), ImageMagick `import`/`convert`, `awk`.

## How a cell is judged

Each cell launches the app with the relevant `flatpak run --env=…` / `--socket`
overrides, lets it run `--settle` seconds, then tears it down:

- **Crash gate** — if the process dies before the settle window, that's a startup
  crash → **FAIL** (the exact renderer/backend failures we're hunting).
- **Log scan** — the app's stderr is scanned for `CRITICAL`, GLib/Gtk `ERROR`,
  failed assertions, segfaults/aborts, and display-connection errors → **FAIL**.
- **Non-blank render** (isolated X11 cells only) — the screenshot's grayscale
  standard deviation must be above a small threshold; an all-black frame → **FAIL**.

The script prints a PASS/FAIL table and exits non-zero if anything failed. Per-cell
logs and screenshots land in `test-live-out/<timestamp>/` (git-ignored).

### Why X11 cells get screenshots and Wayland cells don't

X11 cells run in an **isolated `Xvfb`** server, so `import -window root` captures a
clean frame of just the app (no DRI3 there — GL transparently falls back to llvmpipe,
which is itself useful coverage). Wayland cells run against your **live GNOME/Mutter
session** (the real compositor users run); there's no clean per-app capture there, so
those cells are judged by the crash gate + log scan only. Expect brief Glotze windows
to flash on your desktop during the Wayland cells.

A running Glotze is closed at startup so launches don't hand off to it (GApplication
is single-instance per session bus); each cell starts a genuine fresh process.

## Reading the scaling results

Open the **Group-B `x11-scale*` PNGs** side by side:

- `GDK_SCALE=2` should render the whole UI at ~2× (larger buffer).
- `GDK_DPI_SCALE=1.25/1.5/1.75` enlarges text/spacing within the same window
  geometry.

Look for clipped buttons, overlapping labels, blurry icons, or blank regions.

## Scope / caveats

- This is a **smoke test**, not a functional test — "it launched and drew a window
  without crashing," not "every feature works."
- The `GDK_*` scaling sweep **approximates** fractional scaling; it is not the real
  `wp_fractional_scale_v1` Wayland protocol path.
- **No multi-distro / VM testing.** It was deliberately left out as too heavy —
  because the runtime makes the host distro mostly irrelevant, and the renderer/GL
  env sweep already reproduces the "weak/emulated GPU" crashes a VM would surface.

### Optional upgrade (not implemented)

For isolated Wayland screenshots *and* true compositor fractional scaling, a nested
wlroots compositor would do it — e.g. `labwc` (lighter) or `sway`, configured
`output * scale 1.5`, plus `grim` for capture. We skip it to stay zero-install; add it
here if that coverage ever becomes worth the dependency.
