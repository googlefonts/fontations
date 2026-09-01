#!/usr/bin/env bash
#
# Compile fontconfig's Rust font code against this working tree.
#
# fontconfig reads fonts through `fc-fontations`, and per the impact reports it
# is the consumer that changes shape most: `ScriptList::script_records` and
# `Name::name_record` are the two accessors whose arrays stop being slices.
#
# It checks in one of two ways, depending on what is available:
#
#   full    every module, including the four that bind to fontconfig's C.
#           Needs a fontconfig that has been built once, because two of its
#           crates `include!` bindgen output that meson generates.
#
#   subset  only the modules that use no C bindings — which is where the
#           interesting read-fonts surface is anyway: `capabilities.rs` and
#           `name_records.rs` are the two the impact reports name, and both
#           are binding-free. Needs no C toolchain and runs in seconds.
#
# Pass a built fontconfig to get the full check; otherwise it falls back to
# the subset and says so.
#
# Usage:
#   resources/scripts/check_fontconfig_compat.sh <path-to-fontconfig>
#
# Exits non-zero if the code does not compile.

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: $0 <path-to-fontconfig-checkout>" >&2
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

fc_dir=$(abspath "$1")
fontations_dir=$(abspath "$(dirname "${BASH_SOURCE[0]}")/../..")
src="$fc_dir/fc-fontations"

if [ ! -f "$src/mod.rs" ]; then
    echo "not a fontconfig checkout: $src/mod.rs does not exist" >&2
    exit 2
fi

harness=$(mktemp -d)
trap 'rm -rf "$harness"' EXIT
mkdir -p "$harness/src"

# The bindings crates include! files that meson's rust.bindgen() writes into
# the build directory. Without them only the binding-free modules can build.
if [ -f "$fc_dir/build/fc-fontations/fontconfig.rs" ] &&
    [ -f "$fc_dir/build/fc-fontations/fcint.rs" ]; then
    mode=full
else
    mode=subset
fi

deps() {
    cat <<EOF
[dependencies]
read-fonts = { path = "$fontations_dir/read-fonts" }
font-types = { path = "$fontations_dir/font-types", features = ["bytemuck"] }
skrifa = { path = "$fontations_dir/skrifa" }
EOF
}

if [ "$mode" = full ]; then
    # Reuse fontconfig's own manifest so its bindings crates and their
    # versions come along; only the fontations dependencies are redirected.
    # The manifest is generated outside the checkout, so fontconfig's own
    # relative paths -- its two bindings crates, and the crate root -- have
    # to be made absolute or they resolve against the wrong directory.
    sed -e 's|^read-fonts = .*|read-fonts = { path = "'"$fontations_dir"'/read-fonts" }|' \
        -e 's|^font-types = .*|font-types = { path = "'"$fontations_dir"'/font-types", features = ["bytemuck"] }|' \
        -e 's|^skrifa = .*|skrifa = { path = "'"$fontations_dir"'/skrifa" }|' \
        -e 's|path = "\./|path = "'"$fc_dir"'/|' \
        -e 's|^path = "fc-fontations/|path = "'"$fc_dir"'/fc-fontations/|' \
        "$fc_dir/Cargo.toml" > "$harness/Cargo.toml"
    # build.rs only checks that this exists and then emits link flags, and
    # `cargo check` never links.
    mkdir -p "$fc_dir/build"
    [ -f "$fc_dir/build/libfontconfig.a" ] || touch "$fc_dir/build/libfontconfig.a"
    manifest_dir="$fc_dir"
else
    # Every module that stands on its own, discovered rather than listed so
    # the set follows fontconfig rather than this script. A module is included
    # when it names neither bindings crate and reaches into no sibling: a
    # `crate::` path to its own items still resolves, because each module
    # keeps its file name here, but one to anything else does not, since the
    # modules that would define it are the ones being left out.
    modules=$(
        for f in "$src"/*.rs; do
            name=$(basename "$f" .rs)
            [ "$name" = mod ] && continue
            grep -qE 'fcint_bindings|fontconfig_bindings' "$f" && continue
            foreign=$(grep -oE 'crate::[A-Za-z_0-9]+' "$f" |
                sed 's|crate::||' | sort -u | grep -v "^${name}$" || true)
            [ -n "$foreign" ] && continue
            echo "$name"
        done
    )
    if [ -z "$modules" ]; then
        echo "no binding-free modules found; has fc-fontations been restructured?" >&2
        exit 2
    fi
    {
        echo '[package]'
        echo 'name = "fc-fontations-compat"'
        echo 'version = "0.0.0"'
        echo 'edition = "2021"'
        echo 'publish = false'
        echo
        deps
        echo
        echo '[workspace]'
    } > "$harness/Cargo.toml"
    {
        echo '// Generated: fontconfig modules that bind to no C.'
        for m in $modules; do
            echo "#[path = \"$src/$m.rs\"]"
            echo "mod $m;"
        done
    } > "$harness/src/lib.rs"
    manifest_dir="$harness"
fi

echo "fontconfig: $fc_dir ($(git -C "$fc_dir" rev-parse --short HEAD 2> /dev/null || echo '?'))"
echo "fontations: $fontations_dir ($(git -C "$fontations_dir" rev-parse --short HEAD))"
echo "mode:       $mode"
if [ "$mode" = subset ]; then
    echo "            $(echo $modules | tr '\n' ' ')"
    echo "            (build fontconfig with -Dfontations=enabled for the full check)"
fi
echo

cd "$manifest_dir"
cargo check --manifest-path "$harness/Cargo.toml"
