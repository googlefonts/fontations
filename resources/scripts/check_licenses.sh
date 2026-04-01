#!/usr/bin/env bash

# Script to ensure all crates have the proper license symlinks.
#
# This script finds all directories containing a Cargo.toml and ensures that
# LICENSE-APACHE and LICENSE-MIT are symlinks pointing back to the root licenses.
#
# Usage:
#   ./resources/scripts/check_licenses.sh         # Check only, fail if incorrect
#   ./resources/scripts/check_licenses.sh --fix   # Fix any incorrect symlinks

set -o nounset
set -o errexit

SCRIPTS_DIR=$(dirname "$(realpath "$0")")
ROOT_DIR=$(realpath "$SCRIPTS_DIR/../..")

FIX=false
if [[ $# -gt 0 && "$1" == "--fix" ]]; then
    FIX=true
fi

# Ensure root licenses exist
if [[ ! -f "$ROOT_DIR/LICENSE-APACHE" || ! -f "$ROOT_DIR/LICENSE-MIT" ]]; then
    echo "Error: Root licenses (LICENSE-APACHE/MIT) not found in $ROOT_DIR"
    exit 1
fi


# check_license <crate_dir> <rel_root> <lic_name>
check_license() {
    local crate_dir="$1"
    local rel_root="$2"
    local lic_name="$3"
    
    local target="${rel_root}/${lic_name}"
    local current="${ROOT_DIR}/${crate_dir}/${lic_name}"

    if [[ ! -L "$current" || "$(readlink "$current")" != "$target" ]]; then
        if [ "$FIX" = true ]; then
            echo "Fixing $lic_name in $crate_dir"
            ln -sf "$target" "$current"
        else
            echo "Error: $lic_name in $crate_dir is missing or incorrect."
            echo "Run '$0 --fix' to resolve this."
            exit 1
        fi
    fi
}


# Read each Cargo.toml path from the git command below.
while read -r cargo_toml; do
    crate_dir=$(dirname "$cargo_toml")
    rel_root=$(realpath --relative-to="$crate_dir" .)
    
    check_license "$crate_dir" "$rel_root" "LICENSE-APACHE"
    check_license "$crate_dir" "$rel_root" "LICENSE-MIT"
    
done < <(git ls-files '**/Cargo.toml' | grep -v '^Cargo.toml$')

if [ "$FIX" = false ]; then
    echo "All license symlinks are correct."
fi
