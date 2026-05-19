# rsTerm

Multi terminal emulator with SSH, local PTY, serial, BLE, and a dual-pane file manager.

## Build from source

```bash
cargo build --release
```

## Debian packages

CI builds `.deb` packages for **amd64**, **arm64**, **i386**, and **armhf** (see [`.github/workflows/debian-packages.yml`](.github/workflows/debian-packages.yml)).

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

Output: `dist/rsterm_<version>-1_<arch>.deb`
