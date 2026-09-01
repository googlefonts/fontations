#!/usr/bin/env bash
#
# Compile Skia's Rust font bridge against this working tree.
#
# Skia consumes read-fonts and skrifa through a cxx bridge in
# src/ports/fontations. It builds that with Bazel, and the Cargo.toml in its
# tree exists only to pin dependency versions — it has no `src`, so there is no
# cargo build to borrow. This generates one: the same crate root and the same
# dependencies, resolved against the fontations checkout this script lives in
# rather than against crates.io.
#
# What it answers is "would this change break Skia", which no test in this
# repo can. Nothing else here compiles a downstream consumer.
#
# Usage:
#   resources/scripts/check_skia_compat.sh <path-to-skia>
#
# Exits non-zero if the bridge does not compile.

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: $0 <path-to-skia-checkout>" >&2
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

skia_dir=$(abspath "$1")
fontations_dir=$(abspath "$(dirname "${BASH_SOURCE[0]}")/../..")
bridge="$skia_dir/src/ports/fontations/src/ffi.rs"

if [ ! -f "$bridge" ]; then
    echo "not a Skia checkout: $bridge does not exist" >&2
    exit 2
fi

# The generated crate lives outside both trees so that neither workspace
# adopts it and no build artifacts land in a checkout.
harness=$(mktemp -d)
trap 'rm -rf "$harness"' EXIT

# Dependency versions come from Skia's own manifest, so the harness stays
# honest about what it is pinned to; only the fontations crates are
# redirected. Anything Skia adds later is picked up without editing this.
skia_manifest="$skia_dir/bazel/external/fontations/Cargo.toml"
extra_deps=$(
    sed -n '/^\[dependencies\]/,/^\[/p' "$skia_manifest" |
        grep -E '^[a-z0-9_-]+ *=' |
        grep -vE '^(read-fonts|font-types|skrifa) *=' || true
)

cat > "$harness/Cargo.toml" <<EOF
[package]
name = "skia-fontations-harness"
version = "0.0.0"
edition = "2021"
publish = false

[lib]
name = "fontations_ffi"
path = "$bridge"

[dependencies]
read-fonts = { path = "$fontations_dir/read-fonts" }
font-types = { path = "$fontations_dir/font-types" }
skrifa = { path = "$fontations_dir/skrifa" }
$extra_deps

[workspace]
EOF

echo "skia:       $skia_dir"
echo "fontations: $fontations_dir ($(git -C "$fontations_dir" rev-parse --short HEAD))"
echo

cargo check --manifest-path "$harness/Cargo.toml" --all-targets
