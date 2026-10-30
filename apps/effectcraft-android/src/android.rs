//! The Android shell: eframe's Android runner around the shared EffectCraft app.
//!
//! What the desktop app gets from the OS, this shell gets from Android:
//!
//! | Desktop | Android |
//! |---|---|
//! | settings folder | the app's private folder ([`Layout::config`]) |
//! | `rfd` file dialogs | the app's `Projects`, `Media` and `Exports` folders ([`install_hooks`]) |
//! | OS windows for floating panels | embedded viewports (one activity, one window) |
//! | cpal audio output | none yet: preview plays without sound |
//! | control channel, native menu, update check, launch guard | not started |

use std::sync::Arc;

use effectcraft_engine::Session;
use effectcraft_engine::config::DirConfig;
use effectcraft_ui_egui::EffectcraftApp;
use serde_json::json;
use winit::platform::android::activity::AndroidApp;

use crate::storage::Layout;

/// Space kept clear under the status bar and above the gesture bar, in egui points (dp).
/// Winit does not report the system bars' insets on Android yet (rust-windowing/winit#3910);
/// egui's own `hello_android` example reserves the same way.
const STATUS_BAR_POINTS: f32 = 32.0;
const GESTURE_BAR_POINTS: f32 = 16.0;

/// Runs the app until the activity ends. Called by `android_main`.
pub fn run(app: AndroidApp) -> Result<(), String> {
    let private = app.internal_data_path().ok_or("Android gave the app no private data folder")?;
    let layout = Layout::new(&private, app.external_data_path().as_deref());
    for problem in layout.create_dirs() {
        log::warn!("storage: {problem}");
    }
    log::info!("effectcraft {} on Android; projects in {}", env!("CARGO_PKG_VERSION"), layout.projects.display());
    let options = eframe::NativeOptions { android_app: Some(app), wgpu_options: wgpu_options(), ..Default::default() };
    eframe::run_native("EffectCraft", options, Box::new(move |cc| Ok(create(cc, layout)))).map_err(|e| e.to_string())
}

/// The window's wgpu device asks for the adapter's own limits, like the desktop app's: a phone
/// GPU that cannot offer 16384 px textures still opens the app, and the compositor checks what
/// it needs and falls back to the CPU when the device cannot run it.
fn wgpu_options() -> eframe::WgpuConfiguration {
    use eframe::egui_wgpu::WgpuSetup;
    let mut config = eframe::WgpuConfiguration::default().with_surface_config(eframe::SurfaceConfig::LOW_LATENCY);
    if let WgpuSetup::CreateNew(setup) = &mut config.wgpu_setup {
        setup.instance_descriptor.flags = setup.instance_descriptor.flags.difference(eframe::wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL).with_env();
        setup.device_descriptor = Arc::new(|adapter| device_descriptor(adapter.limits()));
    }
    config
}

fn device_descriptor(adapter: eframe::wgpu::Limits) -> eframe::wgpu::DeviceDescriptor<'static> {
    eframe::wgpu::DeviceDescriptor {
        label: Some("EffectCraft device"),
        required_limits: eframe::wgpu::Limits { max_texture_dimension_2d: adapter.max_texture_dimension_2d.min(16384), ..adapter },
        memory_hints: eframe::wgpu::MemoryHints::MemoryUsage,
        ..Default::default()
    }
}

/// A session wired for this device: the host's media decoding, importer, expressions, scripting
/// and export, with settings in the private folder.
fn session(layout: &Layout) -> Session {
    let mut session = effectcraft_host::session();
    // Lazy open and non-blocking auto-save, like the desktop app.
    session.check_footage_on_open = true;
    session.autosave.background = true;
    // `host::session()` resolves the config folder from desktop environment variables that
    // Android does not set; use the app's own.
    session.models_dir = Some(layout.config.join("models"));
    session.config = Some(Arc::new(DirConfig::new(layout.config.clone())));
    session.load_settings();
    session
}

/// The eframe app creator: graphics failure handling, the session, the start screen and hooks.
fn create(cc: &eframe::CreationContext<'_>, layout: Layout) -> Box<dyn eframe::App> {
    // Android has one window: floating panels stay inside it instead of asking for OS windows.
    cc.egui_ctx.set_embed_viewports(true);
    EffectcraftApp::install_ime_reset_hooks(&cc.egui_ctx);
    match &cc.wgpu_render_state {
        Some(rs) => log::info!("graphics: {:?}", rs.adapter.get_info()),
        None => log::info!("graphics: no wgpu device"),
    }
    // Install the shared device's handlers before the compositor builds its pipelines.
    let gpu_failures = effectcraft_ui_egui::gpu_failure::GpuFailureBridge::new(&cc.egui_ctx);
    if let Some(rs) = &cc.wgpu_render_state {
        let errors = gpu_failures.clone();
        rs.device.on_uncaptured_error(Arc::new(move |error| {
            let message = format!("uncaptured GPU error: {error}");
            if errors.report(&message, false) {
                log::error!("{message}");
            }
        }));
        let lost = gpu_failures.clone();
        rs.device.set_device_lost_callback(move |reason, message| {
            lost.report(&format!("GPU device lost ({reason:?}): {message}"), true);
        });
    }
    let mut session = session(&layout);
    // Scripts in Scripts/Startup run before a project opens (a failing one is reported in a toast).
    let _ = session.execute("file.runStartupScripts", json!({}));
    let recovery = session.begin_recovery();
    // The first launch has nothing to open: show the demo project, so the app is not an empty
    // window. Later launches show the Home screen, as the desktop app does.
    let first_run = layout.newest_project().is_none();
    if first_run && let Err(e) = session.execute("file.openDemoProject", json!({})) {
        log::warn!("demo project: {e}");
    }
    let _ = session.execute("window.restoreScriptPanels", json!({}));
    let show_home = !first_run && session.prefs.startup.show_home_on_launch;
    let mut app = EffectcraftApp::new(session);
    app.set_gpu_failure_bridge(gpu_failures);
    app.ui.start_screen = show_home;
    if let Some(r) = recovery {
        app.offer_recovery(r);
    }
    install_hooks(&mut app, layout);
    log::info!("app created");
    Box::new(AndroidShell { app })
}

/// The pick hooks, answered from the app's folders instead of a system picker (see `storage`).
fn install_hooks(app: &mut EffectcraftApp, layout: Layout) {
    let layout = Arc::new(layout);
    let l = layout.clone();
    app.hooks.pick_files = Some(Box::new(move |exts: &[&str]| l.media_files(exts)));
    let l = layout.clone();
    app.hooks.pick_save = Some(Box::new(move |name: &str| Some(l.project_path(name))));
    let l = layout.clone();
    app.hooks.pick_open_project = Some(Box::new(move || l.newest_project().map(|p| p.to_string_lossy().into_owned())));
    let l = layout.clone();
    app.hooks.pick_folder = Some(Box::new(move || Some(l.media.to_string_lossy().into_owned())));
    let l = layout;
    app.hooks.pick_save_file = Some(Box::new(move |name: &str, _ext: &str| Some(l.export_path(name))));
    // No dialog to show: the log has it, and the auto-save is where the desktop app points.
    app.hooks.device_lost = Some(Box::new(|saved: Option<Result<String, String>>| match saved {
        Some(Ok(path)) => log::error!("graphics device lost; unsaved changes were saved to {path}"),
        Some(Err(e)) => log::error!("graphics device lost; unsaved changes could not be saved ({e})"),
        None => log::error!("graphics device lost"),
    }));
}

/// The eframe app: the shared EffectCraft UI inside the system bars.
struct AndroidShell {
    app: EffectcraftApp,
}

impl eframe::App for AndroidShell {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.tick(ctx, frame);
        self.app.finish_ime_logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::Panel::top("android_status_bar_space").show(ui, |ui| {
            ui.set_height(STATUS_BAR_POINTS);
        });
        egui::Panel::bottom("android_gesture_bar_space").show(ui, |ui| {
            ui.set_height(GESTURE_BAR_POINTS);
        });
        self.app.ui(ui, frame);
    }

    fn on_exit(&mut self) {
        log::info!("the activity is closing");
        self.app.on_exit();
        // Scripts in Scripts/Shutdown run with the project closed.
        let _ = self.app.session.execute("file.runShutdownScripts", json!({}));
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.app.raw_input_hook(ctx, raw_input);
    }
}
