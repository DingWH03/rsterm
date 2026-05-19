#!/usr/bin/env bash
# Build a .deb for the given Rust target and Debian architecture.
# Usage: ./packaging/build-deb.sh <rust-target> <deb-arch> [--cross]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RUST_TARGET="${1:?rust target triple required, e.g. x86_64-unknown-linux-gnu}"
DEB_ARCH="${2:?debian arch required, e.g. amd64}"
USE_CROSS="${USE_CROSS:-0}"

if [[ "${3:-}" == "--cross" ]]; then
  USE_CROSS=1
fi

VERSION="$(grep '^version' Cargo.toml | head -1 | sed -E 's/.*"([^"]+)".*/\1/')"
DEB_VERSION="${VERSION}-1"

echo "==> Building rsterm ${VERSION} for ${RUST_TARGET} (${DEB_ARCH})"

export CARGO_TERM_COLOR=always
export RUST_BACKTRACE=1

if [[ "$USE_CROSS" == "1" ]]; then
  if ! command -v cross >/dev/null 2>&1; then
    echo "error: cross not found (install with: cargo install cross --locked)" >&2
    exit 1
  fi
  cross build --release --target "$RUST_TARGET"
else
  cargo build --release --target "$RUST_TARGET"
fi

if ! command -v cargo-deb >/dev/null 2>&1; then
  echo "==> Installing cargo-deb"
  cargo install cargo-deb --locked
fi

cargo deb --target "$RUST_TARGET" --no-build

mkdir -p dist
shopt -s nullglob
DEBS=(target/debian/rsterm_*_"${DEB_ARCH}".deb)
if ((${#DEBS[@]} == 0)); then
  DEBS=(target/debian/*.deb)
fi
if ((${#DEBS[@]} == 0)); then
  echo "error: no .deb produced under target/debian/" >&2
  exit 1
fi

OUT="dist/rsterm_${DEB_VERSION}_${DEB_ARCH}.deb"
cp "${DEBS[0]}" "$OUT"
echo "==> Wrote $OUT"
ls -lh "$OUT"
