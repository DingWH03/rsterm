# rsTerminal

Multi terminal emulator with SSH, local PTY, serial, BLE, and a dual-pane file manager.

## Build from source

```bash
cargo build --release
```

## Debian packages

CI builds `.deb` packages for **amd64**, **arm64**, **i386**, and **armhf** (see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

Local packaging (example, amd64):

```bash
sudo apt-get install build-essential pkg-config \
  libxcb-render0-dev libxkbcommon-dev libasound2-dev libudev-dev \
  libwayland-dev libx11-dev libxi-dev libxrandr-dev libegl1-mesa-dev
cargo install cargo-deb --locked
./packaging/build-deb.sh x86_64-unknown-linux-gnu amd64
```

Cross-build example (arm32):

```bash
cargo install cross cargo-deb --locked
./packaging/build-deb.sh armv7-unknown-linux-gnueabihf armhf --cross
```

Output: `dist/rsTerminal_<version>-1_<arch>.deb`

## Android APK

Requires [Android SDK + NDK](https://developer.android.com/ndk), `rustup target add aarch64-linux-android`, and `cargo install cargo-ndk`. 

```bash
source packaging/android-env.sh
./packaging/build-apk.sh          # debug APK
./packaging/build-apk.sh release  # release APK
```

Install:

```bash
adb install -r dist/rsTerminal-android-arm64-release.apk
```

