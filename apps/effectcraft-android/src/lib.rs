//! EffectCraft for Android.
//!
//! The same engine and egui UI as the desktop and web apps, run by eframe's Android runner in a
//! `NativeActivity` and packaged as an APK by `cargo-apk` (see `README.md` and
//! `.github/workflows/android.yml`). What the desktop shell gets from the OS, this crate gets from
//! Android; the table is in [`android`].
//!
//! [`storage`] (where the app keeps its files) is portable and unit-tested natively; everything
//! else is `target_os = "android"`.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod storage;

#[cfg(target_os = "android")]
mod android;

/// The entry point `android-activity` calls on its own thread once the activity starts.
// The single `unsafe` item of this crate: `no_mangle` is how the runtime finds this function
// (Cargo.toml explains why the workspace's `forbid` does not apply here).
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    // Everything logged goes to logcat: `adb logcat -s effectcraft_android`.
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info));
    // A panic on the activity thread ends the process; say why first.
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    if let Err(e) = android::run(app) {
        log::error!("EffectCraft stopped: {e}");
    }
}
