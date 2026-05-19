#!/usr/bin/env bash
# Source before Android cross-builds, then use rustup target + cargo-ndk.
#
#   source packaging/android-env.sh
#   rustup target add aarch64-linux-android
#   cargo ndk -t arm64-v8a build --release --lib
#
# Set ANDROID_HOME / NDK_VERSION if auto-detection fails.

if [[ -z "${ANDROID_HOME:-}" ]]; then
  if [[ -n "${ANDROID_SDK_ROOT:-}" ]]; then
    export ANDROID_HOME="$ANDROID_SDK_ROOT"
  elif [[ -d "$HOME/Android/Sdk" ]]; then
    export ANDROID_HOME="$HOME/Android/Sdk"
  elif [[ -d /usr/local/lib/android/sdk ]]; then
    export ANDROID_HOME=/usr/local/lib/android/sdk
  else
    export ANDROID_HOME="${ANDROID_HOME:-/home/dwh/Apps/Android-SDK}"
  fi
fi

if [[ -z "${NDK_VERSION:-}" ]] && [[ -d "$ANDROID_HOME/ndk" ]]; then
  NDK_VERSION="$(ls -1 "$ANDROID_HOME/ndk" 2>/dev/null | sort -V | tail -1)"
  export NDK_VERSION
fi

export NDK_VERSION="${NDK_VERSION:-30.0.14904198}"
export NDK_HOME="${NDK_HOME:-$ANDROID_HOME/ndk/$NDK_VERSION}"
export NDK_TOOLCHAIN="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64"

if [[ ! -d "$NDK_TOOLCHAIN/bin" ]]; then
  echo "error: NDK toolchain not found at $NDK_TOOLCHAIN/bin" >&2
  echo "Set NDK_VERSION to match: ls $ANDROID_HOME/ndk" >&2
  return 1 2>/dev/null || exit 1
fi

export PATH="$NDK_TOOLCHAIN/bin:$PATH"

export CC_aarch64_linux_android="${CC_aarch64_linux_android:-aarch64-linux-android34-clang}"
export CC_armv7_linux_androideabi="${CC_armv7_linux_androideabi:-armv7a-linux-androideabi34-clang}"
export CC_x86_64_linux_android="${CC_x86_64_linux_android:-x86_64-linux-android34-clang}"
export CC_i686_linux_android="${CC_i686_linux_android:-i686-linux-android34-clang}"

echo "ANDROID_HOME=$ANDROID_HOME"
echo "NDK_HOME=$NDK_HOME"
echo "PATH includes NDK clang wrappers"
