#!/usr/bin/env bash
# Build rsTerm Android APK: Rust (cargo-ndk) + Gradle.
#
#   ./packaging/build-apk.sh              # debug APK, arm64-v8a
#   ./packaging/build-apk.sh release       # release APK
#
# Requires: ANDROID_HOME, NDK, rustup android targets, cargo-ndk, Java 17+.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ANDROID_DIR="$ROOT/android"
BUILD_TYPE="${1:-debug}"
GRADLE_TASK="assembleDebug"
CARGO_PROFILE="dev"
NDK_TARGETS=(arm64-v8a)

if [[ "$BUILD_TYPE" == "release" ]]; then
  GRADLE_TASK="assembleRelease"
  CARGO_PROFILE="release"
fi

source "$ROOT/packaging/android-env.sh"

rustup target add aarch64-linux-android >/dev/null 2>&1 || true

JNI_LIBS="$ANDROID_DIR/app/src/main/jniLibs"
rm -rf "$JNI_LIBS"
mkdir -p "$JNI_LIBS"

echo "==> cargo ndk ($CARGO_PROFILE, ${NDK_TARGETS[*]})"
if [[ "$CARGO_PROFILE" == "release" ]]; then
  cargo ndk -o "$JNI_LIBS" -t "${NDK_TARGETS[@]}" -P 34 build --release --lib
else
  cargo ndk -o "$JNI_LIBS" -t "${NDK_TARGETS[@]}" -P 34 build --lib
fi

if [[ ! -f "$JNI_LIBS/arm64-v8a/librsterm.so" ]]; then
  echo "error: librsterm.so not found under $JNI_LIBS" >&2
  exit 1
fi

SDK_DIR="${ANDROID_HOME:-/home/dwh/Apps/Android-SDK}"
cat >"$ANDROID_DIR/local.properties" <<EOF
sdk.dir=$SDK_DIR
EOF

ensure_gradle_wrapper() {
  if [[ -x "$ANDROID_DIR/gradlew" ]]; then
    return
  fi
  echo "==> installing Gradle wrapper (AGP 8.x needs Gradle 8.7+)"
  local ver="8.10.2"
  local zip="/tmp/gradle-${ver}-bin.zip"
  local home="/tmp/gradle-${ver}"
  if [[ ! -d "$home" ]]; then
    if [[ ! -f "$zip" ]]; then
      curl -fL "https://services.gradle.org/distributions/gradle-${ver}-bin.zip" -o "$zip"
    fi
    rm -rf "$home"
    unzip -q "$zip" -d /tmp
  fi
  "$home/bin/gradle" -p "$ANDROID_DIR" wrapper --gradle-version "$ver"
}

ensure_gradle_wrapper

echo "==> gradle $GRADLE_TASK"
cd "$ANDROID_DIR"
./gradlew "$GRADLE_TASK" --no-daemon

APK_DIR="$ANDROID_DIR/app/build/outputs/apk"
if [[ "$BUILD_TYPE" == "release" ]]; then
  APK="$(find "$APK_DIR/release" -name '*.apk' | head -1)"
else
  APK="$(find "$APK_DIR/debug" -name '*.apk' | head -1)"
fi

mkdir -p "$ROOT/dist"
OUT_NAME="rsterm-android-arm64"
if [[ "$BUILD_TYPE" == "release" ]]; then
  OUT_NAME="${OUT_NAME}-release"
else
  OUT_NAME="${OUT_NAME}-debug"
fi
DIST_APK="$ROOT/dist/${OUT_NAME}.apk"
cp -f "$APK" "$DIST_APK"

echo ""
echo "APK: $APK"
echo "dist: $DIST_APK"
ls -lh "$APK" "$DIST_APK"
