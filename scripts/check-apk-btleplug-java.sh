#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 path/to/app.apk" >&2
  exit 2
fi

APK="$1"
if [[ ! -f "$APK" ]]; then
  echo "APK not found: $APK" >&2
  exit 2
fi

REQUIRED=(
  'com/nonpolynomial/btleplug/android/impl/Adapter'
  'com/nonpolynomial/btleplug/android/impl/Peripheral'
  'io/github/gedgygedgy/rust/future/FutureException'
  'io/github/gedgygedgy/rust/task/Waker'
)

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
unzip -q "$APK" 'classes*.dex' -d "$TMP_DIR"

status=0
for cls in "${REQUIRED[@]}"; do
  # grep -aF: binary grep with fixed-string match, much more reliable than
  # strings | grep for DEX files (class names may not be contiguous printable
  # ASCII that strings can extract).
  if grep -aFq "$cls" "$TMP_DIR"/classes*.dex 2>/dev/null; then
    echo "OK: $cls"
  else
    echo "MISSING: $cls" >&2
    status=1
  fi
done

exit "$status"
