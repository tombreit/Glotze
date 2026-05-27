#!/usr/bin/env bash
#
# test-live.sh — on-demand local "live environment" smoke test for the Glotze flatpak.
#
# Why: a Flatpak bundles the whole GUI userspace (GTK4 + libadwaita + Mesa from the
# GNOME runtime), so the host *distribution* barely changes anything. What actually
# breaks GTK4 apps on a stranger's machine is a narrower set of axes — and those are
# the ones we sweep here, all locally, with no VMs and no CI cost:
#
#   1. Display server   — Wayland vs X11 (the --socket=fallback-x11 path)
#   2. GSK renderer / GPU — vulkan (GTK4 default, the #1 "works on my machine" crash),
#                           ngl, gl, and cairo (pure software); plus an llvmpipe run
#                           that mimics a weak/emulated GPU
#   3. Display scaling  — HiDPI / fractional via GDK_SCALE and GDK_DPI_SCALE
#
# Glotze only downloads (no video decode/playback), so the whole risk surface is
# "does the UI come up, render, and lay out without crashing." That is what we assert.
#
# Each cell launches the app in an isolated session bus (so it never hands off to a
# Glotze you already have open, and never disturbs it), lets it settle, screenshots the
# isolated X11 cells, then tears it down and judges PASS/FAIL.
#
# Usage:
#   build-aux/test-live.sh                 # test the currently installed --user app
#   build-aux/test-live.sh path/to.flatpak # install that bundle (--user) and test it
#   build-aux/test-live.sh --build         # build+install from the manifest, then test
#
# Options:
#   --settle N   seconds to let each launch run before judging (default 6)
#   --keep       keep the output directory's screenshots/logs (default: kept anyway)
#   -h, --help

set -uo pipefail

APPID="io.github.tombreit.Glotze"
MANIFEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="$MANIFEST_DIR/$APPID.yml"
SETTLE=6
# Roomy enough that a GDK_SCALE=2 window (default ~900x640 -> ~1800x1280 device px)
# is captured whole, so the Group-B screenshots show the full layout, not a crop.
SCREEN_W=2200
SCREEN_H=1400
BUNDLE=""
DO_BUILD=0

# --- arg parsing ------------------------------------------------------------
while [ $# -gt 0 ]; do
    case "$1" in
        --build) DO_BUILD=1 ;;
        --settle) shift; SETTLE="${1:-6}" ;;
        --keep) : ;; # output is always kept; flag accepted for clarity
        -h|--help)
            sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        -*) echo "Unknown option: $1" >&2; exit 2 ;;
        *)  BUNDLE="$1" ;;
    esac
    shift
done

# --- preflight: required tools ---------------------------------------------
missing=()
for t in flatpak xvfb-run import convert awk; do
    command -v "$t" >/dev/null 2>&1 || missing+=("$t")
done
if [ ${#missing[@]} -gt 0 ]; then
    echo "Missing required tools: ${missing[*]}" >&2
    echo "On Debian/Ubuntu: sudo apt install flatpak xvfb imagemagick dbus" >&2
    exit 1
fi

OUTDIR="$PWD/test-live-out/$(date +%Y%m%d-%H%M%S)"
mkdir -p "$OUTDIR"
INNER="$OUTDIR/_cell.sh"

cleanup() { flatpak kill "$APPID" >/dev/null 2>&1 || true; }
trap cleanup EXIT

# --- resolve which build of the app we test --------------------------------
if [ "$DO_BUILD" -eq 1 ]; then
    echo ">> Building + installing from manifest ($MANIFEST) — this compiles Rust, be patient..."
    flatpak-builder --user --install --force-clean --default-branch=stable \
        "$OUTDIR/_builddir" "$MANIFEST" || { echo "flatpak-builder failed" >&2; exit 1; }
elif [ -n "$BUNDLE" ]; then
    [ -f "$BUNDLE" ] || { echo "Bundle not found: $BUNDLE" >&2; exit 1; }
    echo ">> Installing bundle into the --user installation: $BUNDLE"
    echo "   (this replaces your current --user $APPID; only the app ref is pulled)"
    flatpak install --user --bundle --reinstall --noninteractive "$BUNDLE" \
        || { echo "flatpak install failed" >&2; exit 1; }
fi

if ! flatpak info "$APPID" >/dev/null 2>&1; then
    echo "No installed $APPID found. Pass a bundle path or use --build." >&2
    exit 1
fi
VER="$(flatpak info "$APPID" 2>/dev/null | awk -F': *' '/Version/{print $2; exit}')"
echo ">> Testing $APPID (version ${VER:-unknown})"
echo ">> Output (logs + screenshots): $OUTDIR"

# GApplication is single-instance per session bus: a launch hands off to an
# already-running copy instead of starting fresh. Kill any running instance up
# front so every cell starts a genuine new process (we also kill between cells).
if flatpak ps --columns=application 2>/dev/null | grep -qx "$APPID"; then
    echo ">> Note: a running $APPID was found and will be closed so launches don't hand off to it."
    flatpak kill "$APPID" >/dev/null 2>&1 || true
    sleep 1
fi
echo

# --- the per-cell runner (static; parameters come in as argv) --------------
# argv: LOG SHOT BACKEND SETTLE APPID -- <flatpak run args...>
cat >"$INNER" <<'INNEREOF'
#!/usr/bin/env bash
set -u
LOG="$1"; SHOT="$2"; BACKEND="$3"; SETTLE="$4"; APPID="$5"; shift 5
[ "${1:-}" = "--" ] && shift

flatpak run "$@" >"$LOG" 2>&1 &
APP_PID=$!

# Wait for the app to settle, bailing the moment it dies (a startup crash is
# exactly the "live" failure we are hunting for).
slept=0
while [ "$slept" -lt "$SETTLE" ]; do
    sleep 1
    if ! kill -0 "$APP_PID" 2>/dev/null; then
        wait "$APP_PID" 2>/dev/null; rc=$?
        echo "__CELL__ crashed rc=$rc" >>"$LOG"
        exit 3
    fi
    slept=$((slept + 1))
done

# Still alive -> capture what it rendered (only meaningful for the isolated
# Xvfb cells; the live Wayland session has no clean per-app capture here).
if [ "$BACKEND" = "x11" ]; then
    import -window root "$SHOT" >/dev/null 2>&1 || true
fi

flatpak kill "$APPID" >/dev/null 2>&1 || true
kill -TERM "$APP_PID" 2>/dev/null || true
sleep 1
kill -KILL "$APP_PID" 2>/dev/null || true
echo "__CELL__ survived ${SETTLE}s" >>"$LOG"
exit 0
INNEREOF

# Patterns that indicate a real failure (kept specific to avoid false positives
# on informational lines). fatal-criticals already turns CRITICALs into a crash,
# so this is mostly a backstop plus the display-connection cases.
ERR_RE='CRITICAL \*\*| ERROR \*\*|Gtk-ERROR|GLib-ERROR|assertion .*failed|Segmentation fault|SIGSEGV|SIGABRT|Trace/breakpoint trap|core dumped|terminated by signal|[Cc]ouldn.t connect to|annot open display|Unable to init server|Failed to open display|Gdk-Message:.*(failed|[Ee]rror)|Failed to (create|initialize) .*(context|renderer|surface|display)'
# Informational: renderer not honored (cell didn't exercise what we intended).
INFO_RE='[Uu]nrecognized renderer|[Ff]alling back to|not available, falling'

# --- the matrix -------------------------------------------------------------
# Spec fields: GROUP|NAME|BACKEND|RENDERER|SOFTGL|GDK_SCALE|GDK_DPI_SCALE
#   RENDERER "" = runtime default; SOFTGL 1 = LIBGL_ALWAYS_SOFTWARE+llvmpipe
CELLS=(
    # A) display server x renderer (default scaling)
    "A|wayland-vulkan|wayland|vulkan|0||"
    "A|wayland-ngl|wayland|ngl|0||"
    "A|wayland-gl|wayland|gl|0||"
    "A|wayland-cairo-sw|wayland|cairo|0||"
    "A|x11-gl|x11|gl|0||"
    "A|x11-cairo-sw|x11|cairo|0||"
    "A|x11-gl-llvmpipe|x11|gl|1||"
    # B) scaling sweep (renderer gl)
    "B|x11-scale1-dpi1.00|x11|gl|0|1|1.0"
    "B|x11-scale1-dpi1.25|x11|gl|0|1|1.25"
    "B|x11-scale1-dpi1.43|x11|gl|0|1|1.43"
    "B|x11-scale1-dpi1.50|x11|gl|0|1|1.5"
    "B|x11-scale1-dpi1.75|x11|gl|0|1|1.75"
    "B|x11-scale2-dpi1.00|x11|gl|0|2|1.0"
    "B|x11-scale2-dpi0.75|x11|gl|0|2|0.75"
    "B|wl-scale1-dpi1.25|wayland|gl|0|1|1.25"
    "B|wl-scale2-dpi1.00|wayland|gl|0|2|1.0"
)

R_NAMES=(); R_STATUS=(); R_NOTE=()

run_cell() {
    local group="$1" name="$2" backend="$3" renderer="$4" softgl="$5" gscale="$6" gdpi="$7"
    local log="$OUTDIR/$name.log" shot="$OUTDIR/$name.png"

    local args=()
    [ -n "$renderer" ] && args+=( "--env=GSK_RENDERER=$renderer" )
    [ "$softgl" = "1" ] && args+=( --env=LIBGL_ALWAYS_SOFTWARE=1 --env=GALLIUM_DRIVER=llvmpipe )
    [ -n "$gscale" ] && args+=( "--env=GDK_SCALE=$gscale" )
    [ -n "$gdpi" ] && args+=( "--env=GDK_DPI_SCALE=$gdpi" )
    if [ "$backend" = "x11" ]; then
        args+=( --socket=x11 --nosocket=wayland --env=GDK_BACKEND=x11 )
    else
        args+=( --nosocket=fallback-x11 --env=GDK_BACKEND=wayland )
    fi
    args+=( "$APPID" )

    printf '  %-22s [%s] ... ' "$name" "$backend"

    # Wrapper noise (Xvfb/Mesa chatter) goes to a side log so the console stays
    # readable; the app's own stdout/stderr is captured in "$log" by the inner.
    local wlog="$OUTDIR/$name.wrapper.log" rc
    if [ "$backend" = "x11" ]; then
        # Isolated X11 server (no DRI3 -> GL transparently falls back to llvmpipe).
        xvfb-run -a -s "-screen 0 ${SCREEN_W}x${SCREEN_H}x24 -dpi 96" \
            bash "$INNER" "$log" "$shot" "$backend" "$SETTLE" "$APPID" -- "${args[@]}" 2>"$wlog"
        rc=$?
    else
        # Live Wayland session (real GNOME/Mutter compositor).
        bash "$INNER" "$log" "$shot" "$backend" "$SETTLE" "$APPID" -- "${args[@]}" 2>"$wlog"
        rc=$?
    fi

    # Evaluate
    local status="PASS" note=""
    local hits info
    hits="$(grep -hE "$ERR_RE" "$log" 2>/dev/null | head -1)"
    info="$(grep -hE "$INFO_RE" "$log" 2>/dev/null | head -1)"

    if [ "$rc" -eq 3 ]; then
        status="FAIL"; note="startup crash ($(grep -hE "$ERR_RE" "$log" 2>/dev/null | head -1 | cut -c1-60))"
    elif [ "$rc" -ne 0 ]; then
        status="ERROR"; note="wrapper rc=$rc"
    else
        if [ -n "$hits" ]; then
            status="FAIL"; note="log: $(echo "$hits" | cut -c1-60)"
        fi
        # Non-blank render check, isolated X11 cells only.
        if [ "$backend" = "x11" ] && [ -f "$shot" ]; then
            local sd
            sd="$(convert "$shot" -colorspace Gray -format '%[fx:standard_deviation]' info: 2>/dev/null || echo 0)"
            if awk "BEGIN{exit !($sd <= 0.005)}"; then
                if [ "$status" = "PASS" ]; then status="FAIL"; note="blank render (stddev=$sd)"; fi
            fi
        fi
    fi
    [ -n "$info" ] && note="${note:+$note; }note: renderer fell back"

    case "$status" in
        PASS)  printf '\033[32mPASS\033[0m' ;;
        FAIL)  printf '\033[31mFAIL\033[0m' ;;
        *)     printf '\033[33m%s\033[0m' "$status" ;;
    esac
    [ -n "$note" ] && printf '  — %s' "$note"
    printf '\n'

    R_NAMES+=("$name"); R_STATUS+=("$status"); R_NOTE+=("$note")
}

echo "Group A — display server x renderer (default scaling):"
for spec in "${CELLS[@]}"; do
    IFS='|' read -r g n b r s sc dp <<<"$spec"
    [ "$g" = "A" ] && run_cell "$g" "$n" "$b" "$r" "$s" "$sc" "$dp"
done
echo
echo "Group B — scaling sweep (renderer gl); X11 cells produce screenshots to eyeball:"
for spec in "${CELLS[@]}"; do
    IFS='|' read -r g n b r s sc dp <<<"$spec"
    [ "$g" = "B" ] && run_cell "$g" "$n" "$b" "$r" "$s" "$sc" "$dp"
done

# --- summary ----------------------------------------------------------------
echo
pass=0; fail=0; err=0
for i in "${!R_NAMES[@]}"; do
    case "${R_STATUS[$i]}" in
        PASS) pass=$((pass+1)) ;;
        FAIL) fail=$((fail+1)) ;;
        *)    err=$((err+1)) ;;
    esac
done
echo "Summary: $pass passed, $fail failed, $err errored  (of ${#R_NAMES[@]} cells)"
echo "Screenshots + per-cell logs: $OUTDIR"
echo "Tip: open the Group-B x11-scale* PNGs side by side to spot clipping/blur/layout breaks."

[ $((fail + err)) -eq 0 ]
