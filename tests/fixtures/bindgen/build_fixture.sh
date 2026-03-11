#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
OUT_DIR="$REPO_ROOT/target/test-bindgen"
mkdir -p "$OUT_DIR"

SRC="$SCRIPT_DIR/example.c"
INCLUDE_DIR="$SCRIPT_DIR"

UNAME="$(uname -s)"
if [[ "$UNAME" == "Darwin" ]]; then
  OUT_LIB="$OUT_DIR/libexample_bindgen.dylib"
  clang -dynamiclib "$SRC" -I"$INCLUDE_DIR" -o "$OUT_LIB"
elif [[ "$UNAME" == "Linux" ]]; then
  OUT_LIB="$OUT_DIR/libexample_bindgen.so"
  clang -shared -fPIC "$SRC" -I"$INCLUDE_DIR" -o "$OUT_LIB"
else
  echo "unsupported host OS: $UNAME" >&2
  exit 1
fi

echo "$OUT_LIB"
