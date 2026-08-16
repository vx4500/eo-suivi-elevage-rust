#!/usr/bin/env sh
set -eu
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$SCRIPT_DIR"
: "${ELEVAGE_DATA:=$SCRIPT_DIR/data}"
export ELEVAGE_DATA
exec ./target/release/eo-suivi-elevage

