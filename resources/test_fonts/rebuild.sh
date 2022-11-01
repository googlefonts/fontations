#!/usr/bin/env bash

# run this script from the repo root to rebuild the binary inputs (ttfs) from
# their xml/ttx representations.

set -o errexit
set -o nounset
set -o pipefail

SRC_DIR=./resources/test_fonts/ttx
OUT_DIR=./resources/test_fonts/ttf
VENV_DIR=./.venv
PIP=$VENV_DIR/bin/pip
REQUIREMENTS=./resources/test_fonts/requirements.txt
TTX=$VENV_DIR/bin/ttx

if [ ! -d "$SRC_DIR" ]; then
  echo "Error: $SRC_DIR does not exist. Are you in the repo root?" >&2
  exit 1
fi

# check that we have python3 + virtualenv installed:
if ! python3 -m venv -h  >/dev/null 2>&1; then
    echo "Error: script requires python3 and venv module" >&2
    exit 1
fi

if [ ! -d "$VENV_DIR" ]; then
    echo "Setting up venv at $VENV_DIR"
    python3 -m venv $VENV_DIR
fi

echo "Installing fonttools"
$PIP install --upgrade pip
$PIP install -r $REQUIREMENTS

for f in $(ls $SRC_DIR/*.ttx); do
    $TTX -o $OUT_DIR/$(basename "$f" .ttx).ttf --no-recalc-timestamp -b $f
done
