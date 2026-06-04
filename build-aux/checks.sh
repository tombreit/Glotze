#!/bin/sh

# Inspired by https://gitlab.gnome.org/World/Shortwave/-/blob/main/build-aux/checks.sh

export LC_ALL=C

# Usage info
show_help() {
cat << EOF
Run conformity checks on the current Rust project.
Currently used in CI/CD (lint.yml, flatpak.yml), .pre-commit-config.yaml,
cargo release's pre-release hook, or triggered manually.

USAGE: ${0##*/} [CHECK]
EOF
}

# Style helpers
act="\033[1;32m"
warn="\033[1;33m"
err="\033[1;31m"
pos="\033[32m"
res="\033[0m"

# Common styled strings
checking="${act}Checking${res}"
warning="${warn}Warning${res}"
failed="${err}Failed${res}"
ok="${pos}OK${res}"

# In CI, add --gha-format so flatpak-builder-lint findings show as PR annotations.
[ "${GITHUB_ACTIONS:-}" = "true" ] && gha="--gha-format" || gha=""

# flatpak-builder-lint: prefer the native binary, fall back to the flatpak.
if command -v flatpak-builder-lint >/dev/null 2>&1; then
    fbl="flatpak-builder-lint"
elif command -v flatpak >/dev/null 2>&1 && flatpak info org.flatpak.Builder >/dev/null 2>&1; then
    fbl="flatpak run --command=flatpak-builder-lint org.flatpak.Builder"
else
    fbl=""
fi

check_tool_availability() {
    echo ""
    if ! $2 >/dev/null 2>&1; then
        printf "%b %s is not installed, skipping check.\n" "$warning" "$1"
        retval=1
    else
        printf "%b %s …\n" "$checking" "$1"
        retval=0
    fi
    return "$retval"
}

# Same, for the flatpak-builder-lint resolved above (a possibly multi-word command).
check_fbl_availability() {
    echo ""
    if [ -z "$fbl" ]; then
        printf "%b flatpak-builder-lint is not installed, skipping check.\n" "$warning"
        return 1
    fi
    printf "%b flatpak-builder-lint …\n" "$checking"
}

execute() {
    if ! $2; then
        echo ""
        printf "%s result: %b\n" "$1" "$failed"
        exit 1
    else
        echo ""
        printf "%s result: %b\n" "$1" "$ok"
    fi
}

cargo_fmt() {
    check_tool_availability "cargo-fmt" "cargo fmt --version" || return
    execute "cargo-fmt" "cargo fmt --all -- --check"
}

cargo_clippy() {
    check_tool_availability "cargo-clippy" "cargo clippy --version" || return
    execute "cargo-clippy" "cargo clippy --all-targets --locked -- -D warnings"
}

cargo_test() {
    check_tool_availability "cargo-test" "cargo --version" || return
    execute "cargo-test" "cargo test --all-targets --locked"
}

manifest() {
    check_fbl_availability || return
    execute "manifest" "$fbl $gha manifest build-aux/io.github.tombreit.Glotze.yml"
}

repo() {
    check_fbl_availability || return
    execute "repo" "$fbl $gha --exceptions --user-exceptions build-aux/flatpak-builder-lint-exceptions.json repo repo"
}

potfiles() {
    check_tool_availability "potfiles" "git --version" || return
    git ls-files 'src/*.rs' 'src/*.ui' 'data/*.ui' 'data/*.desktop.in*' 'data/*.metainfo.xml.in*' > po/POTFILES.in
    execute "potfiles" "git diff --exit-code po/POTFILES.in"
}

# Check arguments
while [ "$1" ]; do case $1 in
    cargo_fmt )    cargo_fmt;     exit 0 ;;
    cargo_clippy ) cargo_clippy;  exit 0 ;;
    cargo_test )   cargo_test;    exit 0 ;;
    manifest )     manifest;      exit 0 ;;
    repo )         repo;          exit 0 ;;
    potfiles )     potfiles;      exit 0 ;;
    * )            show_help >&2; exit 1 ;;
esac; shift; done

# Run
cargo_fmt
cargo_clippy
cargo_test
manifest
potfiles
