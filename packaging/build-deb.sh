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

echo "==> Building rsTerminal ${VERSION} for ${RUST_TARGET} (${DEB_ARCH})"

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

cross_strip_for_target() {
    case "$1" in
        armv7-unknown-linux-gnueabihf) command -v arm-linux-gnueabihf-strip ;;
        i686-unknown-linux-gnu) command -v i686-linux-gnu-strip ;;
        *) return 1 ;;
    esac
}

deb_args=(--target "$RUST_TARGET" --no-build)
if [[ "$USE_CROSS" == "1" ]] && ! cross_strip_for_target "$RUST_TARGET" >/dev/null; then
    echo "warning: cross strip tool not found; .deb will not be stripped (install binutils-*-linux-gnu)" >&2
    deb_args+=(--no-strip)
fi
cargo deb "${deb_args[@]}"

mkdir -p dist
shopt -s nullglob
DEBS=(target/debian/rsTerminal_*_"${DEB_ARCH}".deb)
if ((${#DEBS[@]} == 0)); then
  DEBS=(target/debian/*.deb)
fi
if ((${#DEBS[@]} == 0)); then
  echo "error: no .deb produced under target/debian/" >&2
  exit 1
fi

OUT="dist/rsTerminal_${DEB_VERSION}_${DEB_ARCH}.deb"
cp "${DEBS[0]}" "$OUT"
echo "==> Wrote $OUT"
ls -lh "$OUT"
