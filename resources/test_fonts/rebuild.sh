#!/usr/bin/env bash

# this script rebuilds the binary test fonts from their xml (ttx) sources

set -o errexit
set -o nounset
set -o pipefail

SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
SRC_DIR=$SCRIPT_DIR/ttx
OUT_DIR=$SCRIPT_DIR/ttf
VENV_DIR=$SCRIPT_DIR/../../.venv
REQUIREMENTS=$SCRIPT_DIR/requirements.txt
PIP=$VENV_DIR/bin/pip
TTX=$VENV_DIR/bin/ttx
EXTRACT_GLYPHS=$SCRIPT_DIR/extract_glyphs.py

# check that we have python3 + virtualenv installed:
if ! python3 -m venv -h  >/dev/null 2>&1; then
    echo "Error: script requires python3 and venv module" >&2
    exit 1
fi

if [ ! -d "$VENV_DIR" ]; then
    echo "Setting up venv at $VENV_DIR"
    python3 -m venv $VENV_DIR
fi

echo "Installing fonttools and freetype-py"
$PIP install --upgrade pip
$PIP install -r $REQUIREMENTS

source $VENV_DIR/bin/activate

for f in $(ls $SRC_DIR/*.ttx); do
    $TTX -o $OUT_DIR/$(basename "$f" .ttx).ttf --no-recalc-timestamp -b $f
    python3 $EXTRACT_GLYPHS $OUT_DIR/$(basename "$f" .ttx).ttf
done

deactivate
