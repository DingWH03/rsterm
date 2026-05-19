//! Desktop binary entry (`android` uses `cdylib` + `android_main` in `lib.rs`).

#[cfg(not(target_os = "android"))]
fn main() {
    rsterm::run_desktop();
}

#[cfg(target_os = "android")]
fn main() {}
