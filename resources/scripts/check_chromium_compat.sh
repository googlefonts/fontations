#!/usr/bin/env bash
#
# Compile Chromium's own Rust font code against this working tree.
#
# Chromium reads fonts from Rust in two places of its own — a format check in
# Blink and a name-table lookup in the browser process — and builds them with
# GN, so there is no cargo build to borrow. This generates one.
#
# Only Chromium's own code. The other consumers that link read-fonts from a
# Chromium checkout — Skia, HarfBuzz, fontconfig, pdfium — are DEPS submodules
# whose sources are not in its git tree at all, and Skia and fontconfig have
# their own checks here.
#
# Usage:
#   resources/scripts/check_chromium_compat.sh <path-to-chromium>
#
# Exits non-zero if the code does not compile.

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: $0 <path-to-chromium-checkout>" >&2
    exit 2
fi

# cargo is a native binary, so on Windows it needs a native path; a bare
# `pwd` under Git Bash yields /c/... which it reads as a relative directory.
abspath() {
    local resolved
    resolved=$(cd "$1" && pwd)
    if command -v cygpath > /dev/null 2>&1; then
        cygpath -m "$resolved"
    else
        printf '%s' "$resolved"
    fi
}

cr_dir=$(abspath "$1")
fontations_dir=$(abspath "$(dirname "${BASH_SOURCE[0]}")/../..")

# Found rather than listed, so a file that moves is noticed here instead of
# being silently dropped.
#
# The excluded paths are the vendored projects, named one by one rather than by
# excluding `third_party` wholesale: Blink lives under `third_party/blink` and
# is Chromium's own, so the broad exclusion would drop half of what this is
# for. `third_party/rust` holds the vendored copies of read-fonts itself.
mapfile -t sources < <(
    find "$cr_dir" -name '*.rs' \
        -not -path '*/third_party/rust/*' \
        -not -path '*/third_party/skia/*' \
        -not -path '*/third_party/harfbuzz/*' \
        -not -path '*/third_party/fontconfig/*' \
        -not -path '*/third_party/pdfium/*' \
        -exec grep -lE 'read_fonts|skrifa' {} + 2> /dev/null | sort
)

if [ ${#sources[@]} -eq 0 ]; then
    echo "no Rust files using read-fonts under $cr_dir" >&2
    echo "is the checkout sparse, and does it still include the font paths?" >&2
    exit 2
fi

harness=$(mktemp -d)
trap 'rm -rf "$harness"' EXIT
mkdir -p "$harness/src"

cat > "$harness/Cargo.toml" << EOF
[package]
name = "chromium-fonts-harness"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
read-fonts = { path = "$fontations_dir/read-fonts" }
font-types = { path = "$fontations_dir/font-types" }
skrifa = { path = "$fontations_dir/skrifa" }
cxx = "1.0"

[workspace]
EOF

{
    echo '// Generated: every file in the checkout that reads fonts.'
    for src in "${sources[@]}"; do
        name=$(basename "$src" .rs)
        echo "#[path = \"$src\"]"
        echo "mod $name;"
    done
} > "$harness/src/lib.rs"

echo "chromium:   $cr_dir ($(git -C "$cr_dir" rev-parse --short HEAD 2> /dev/null || echo '?'))"
echo "fontations: $fontations_dir ($(git -C "$fontations_dir" rev-parse --short HEAD))"
for src in "${sources[@]}"; do
    echo "            ${src#"$cr_dir"/}"
done
echo

cargo check --manifest-path "$harness/Cargo.toml"
