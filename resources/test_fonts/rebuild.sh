#!/usr/bin/env bash

# run this script from the repo root to rebuild the binary inputs (ttfs) from
# their xml/ttx representations.

SRC_DIR=./resources/test_fonts/ttx
OUT_DIR=./resources/test_fonts/ttf
VERSION_FILE=$OUT_DIR/GENERATED_BY_TTX_VERSION
PREV_VERSION=$(<$VERSION_FILE)
THIS_VERSION=$(ttx --version)

if [ ! -d "$SRC_DIR" ]; then
  echo "Error: $DIRECTORY does not exist. Are you in the repo root?" >&2
  exit 1
fi

if [ "$THIS_VERSION" == "" ]; then
  echo "Error: 'ttx' not found. Is fonttools installed?" >&2
  exit 1
fi

if [ "$PREV_VERSION" != "$THIS_VERSION" ]; then
    echo "Note: using ttx version '$THIS_VERSION', files previously generated with version '$PREV_VERSION'" >&2
fi

for f in $(ls $SRC_DIR/*.ttx); do
    ttx -o $OUT_DIR/$(basename "$f" .ttx).ttf --no-recalc-timestamp -b $f
done

ttx --version > $VERSION_FILE

