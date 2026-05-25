# Screenshots

AppStream screenshots for the Flathub listing. Flathub does **not** host
screenshots — it mirrors the HTTPS URLs in
`data/io.github.tombreit.Glotze.metainfo.xml` at build time. Keeping the images
here and referencing them via `raw.githubusercontent.com` keeps everything in
one repo.

## How to add them

1. Capture screenshots (GNOME Screenshot / Loupe) and drop the PNGs in this
   directory using these exact filenames (they're already wired into the
   metainfo):

   | File                    | Caption                              | Hero?         |
   |-------------------------|--------------------------------------|---------------|
   | `search-results.png`    | Search results from MediathekViewWeb | `type="default"` |
   | `quality-picker.png`    | Choosing a download quality          |               |
   | `download-progress.png` | A download in progress               |               |

   Add or remove entries as you like — just keep the metainfo block in sync,
   and make sure **exactly one** screenshot is `type="default"` (the hero image
   shown on the Flathub card).

2. In `data/io.github.tombreit.Glotze.metainfo.xml`, **uncomment** the
   `<screenshots>` block.

3. Commit and push to `main` so the
   `https://raw.githubusercontent.com/tombreit/Glotze/main/data/screenshots/…`
   URLs resolve, then validate:

   ```sh
   appstreamcli validate --explain data/io.github.tombreit.Glotze.metainfo.xml
   ```

   (Drop `--no-net` here so it actually fetches the images and confirms they
   resolve — the same check Flathub runs.)

## Image guidelines (Flathub)

- **PNG**, lossless. JPEG is allowed but visibly worse.
- **At least 1280×720**; 1600×900 or 1920×1080 is better.
- **16:9** preferred — Flathub's hero card is widescreen.
- **Light theme**, cropped to a clean window (no desktop, no panels).

See `PUBLISHING.md` → Step 2 for the full rationale.

> Note: these PNGs are copied into the Flatpak build context but **not**
> installed into the app (`data/meson.build` doesn't reference them), so they
> don't bloat the shipped Flatpak. They exist only to be served over HTTPS.
>
> If you later prefer a churn-free URL, move them to a dedicated `screenshots`
> or `gh-pages` branch and update the `<image>` URLs accordingly.
