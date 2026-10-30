# EffectCraft for Android

The same engine and egui UI as the desktop and web apps, run by eframe's Android runner in a
`NativeActivity` and packaged as an APK with [`cargo-apk`](https://github.com/parasyte/cargo-apk).

> **Status: first draft, not yet built.** The portable code (`src/storage.rs`) has unit tests that
> pass. The Android code (`src/android.rs`, `src/lib.rs`) and the packaging metadata were written
> against the sources of this repository and of the egui fork it pins, but were **not compiled**:
> the environment they were written in could not reach crates.io or the Android SDK. Expect the
> first CI run to need a fix or two. Send the log of the failing step and iterate.

## Get the APK

1. Put this repository (or your fork) on GitHub.
2. Actions tab ▸ **Android APK** ▸ **Run workflow**.
3. When it ends, download the `effectcraft-mobile-apk` artifact, unzip it and install the `.apk`
   (allow "install unknown apps" for your file manager or browser).
4. Download the `cargo-lock` artifact as well and commit it as `Cargo.lock`: this crate adds
   dependencies, so the lock file changes.

Release builds are signed with a throwaway debug key unless the secrets `ANDROID_KEYSTORE_BASE64`
(base64 of your `.jks`) and `ANDROID_KEYSTORE_PASSWORD` exist. A debug-signed APK installs on your
own phone; it is not suitable for the Play Store.

## Build on your own machine

```sh
rustup target add aarch64-linux-android
# Android SDK + NDK: see egui's examples/hello_android/README.md (same versions as the workflow).
export ANDROID_HOME=... ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/29.0.14206865
cargo install --git https://github.com/parasyte/cargo-apk.git \
  --rev 282639508eeed7d73f2e1eaeea042da2716436d5 cargo-apk
sh apps/effectcraft-android/prepare-res.sh
cargo apk build --release -p effectcraft-android --lib      # APK under target/release/apk/
cargo apk run --release -p effectcraft-android --lib        # build, install and start on a connected phone
adb logcat -s effectcraft_android                           # the app's log
```

Only `aarch64-linux-android` (64-bit ARM phones) is built. Add `x86_64-linux-android` to
`build_targets` in `Cargo.toml` for an emulator.

## What works, what does not

Working by design (still to be confirmed on a device):

- The whole engine: compositions, layers, keyframes, effects, expressions, 3D, scripting, the
  Render Queue (CPU and GPU through wgpu: Vulkan, or OpenGL ES as a fallback).
- Settings, shortcuts and auto-saves in the app's private folder.
- First launch opens the demo project; later launches show the Home screen.

Different from desktop:

- **No system file picker.** Files live in the app's folder on shared storage,
  `Android/data/com.example.effectcraftmobile/files/` (reachable from a computer over USB, or from
  a file manager): `Media/` for footage, `Projects/` for projects, `Exports/` for rendered files.
  File ▸ Import imports **every** file in `Media/` with a matching extension, File ▸ Open opens the
  **newest** project in `Projects/`, Save writes into `Projects/`.
- **No audio output yet**: preview plays silently.
- **No system-bar insets**: the UI keeps a fixed gap under the status bar and above the gesture bar.
- No control channel, native menu, update check or crash-launch guard.

## Known limitations and risks (most likely first)

1. **Build errors on the first run.** The Android shell is a port of `apps/effectcraft/src/main.rs`
   and `apps/effectcraft-web/src/app.rs`; an API that differs in this egui fork shows up as a compile
   error in `src/android.rs`. The `cargo-apk` metadata keys in `Cargo.toml` and the
   `CARGO_APK_RELEASE_KEYSTORE*` variables in the workflow come from memory of cargo-apk's docs.
2. **Dependencies that may not build for Android.** `ring` (through `craft-update`'s HTTPS client)
   and `craft-ui`'s floating-window code are compiled for every non-wasm32 target. The first is
   a C build that needs the NDK; the second is held to embedded windows by
   `set_embed_viewports(true)`.
3. **Help ▸ Check for Updates** may fail: the HTTPS certificate check is not initialised for
   Android. The startup update check is not started.
4. **Soft keyboard.** `NativeActivity` shows the keyboard but delivers only plain key events: no
   IME composition, so typing in fields can be unreliable. The fix is `GameActivity`
   (`android-game-activity` in eframe), which needs a Gradle build instead of cargo-apk alone.
5. **Performance and memory.** Phones have far less memory and GPU than the desktop this was tuned
   for; heavy compositions, 4K footage and the VP9/AV1/HEVC encoders will be slow. Use `release`.
6. **Layout.** The panel docking is designed for a desktop window. Landscape is the usable
   orientation; the UI scale in Settings helps.

## Licensing and branding

The code is MIT OR Apache-2.0, like the rest of the repository; keep `LICENSE-*`, `NOTICE` and
`ATTRIBUTION.md` with any copy. Using it on your own phone needs nothing else.

If you **distribute or publish** this build, `docs/brand/LICENSE-brand.txt` requires you to remove
(or replace with your own) the ArtCraft name, logos and marks, which the shared UI shows in its
About window and Help menu, and not to call the app ArtCraft. The app label here, "EffectCraft
Mobile", and the package id, `com.example.effectcraftmobile`, are placeholders: change both in
`Cargo.toml` before publishing. The launcher icon is the repository's own EffectCraft app icon.

## Why this crate has one `unsafe`

The workspace sets `unsafe_code = "forbid"`. `android-activity` finds the app through a function
named `android_main`, which must be `#[unsafe(no_mangle)]`; no safe alternative exists. This crate
therefore does not inherit the workspace lints: it denies `unsafe_code`, repeats the workspace's
clippy levels and the crate-level panic-free denies, and allows `unsafe_code` on that one function.
egui's own `hello_android` example makes the same exception.
