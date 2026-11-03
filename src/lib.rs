mod app;
mod cache;
mod db;
mod dialogs;
mod downloads;
mod embedded;
mod net;
mod equalizer;
mod http;
mod httpdl;
mod import;
mod jobs;
mod logs;
mod lyrics;
mod matchscore;
mod media_controls;
mod miniplayer;
mod model;
mod normalize;
mod pace;
mod player;
mod proc;
mod queue;
mod radio;
mod region;
mod settings;
mod sleep;
mod stats;
mod suggest;
mod ui;
mod writer;
mod ytdlp;
mod provider;
mod provider_youtube_music;
mod provider_jiosaavn;
mod provider_youtube;

pub fn run() -> eframe::Result {
    logs::install_panic_hook();
    if std::env::args().any(|arg| arg == "--stats") {
        if !crate::proc::attach_parent_console() {
            logs::error("--stats could not attach to a console; run it from a terminal");
        }
        stats::spawn_reporter();
    }
    run_with(native_options())
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    logs::install_panic_hook();
    if let Some(dir) = app.internal_data_path() {
        std::env::set_var("ARIA_DATA_DIR", dir);
    }
    let options = eframe::NativeOptions {
        android_app: Some(app),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let _ = run_with(options);
}

fn run_with(options: eframe::NativeOptions) -> eframe::Result {
    let result = eframe::run_native(
        "Aria",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    );
    if let Err(err) = &result {
        logs::error(&format!("Aria exited with an error: {err}"));
    }
    result
}

fn preview_size() -> Option<[f32; 2]> {
    let value = std::env::var("ARIA_WINDOW").ok()?;
    let (width, height) = value.split_once('x')?;
    let (width, height) = (width.trim().parse::<f32>().ok()?, height.trim().parse::<f32>().ok()?);
    ((200.0..=4000.0).contains(&width) && (200.0..=4000.0).contains(&height)).then_some([width, height])
}

fn native_options() -> eframe::NativeOptions {
    let size = preview_size().unwrap_or([1180.0, 780.0]);
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("aria.player")
            .with_title("Aria")
            .with_icon(window_icon())
            .with_inner_size(size)
            .with_min_inner_size([320.0, 420.0]),
        renderer: eframe::Renderer::Glow,
        persist_window: preview_size().is_none(),
        centered: true,
        ..Default::default()
    }
}

fn window_icon() -> std::sync::Arc<egui::IconData> {
    let decoded = image::load_from_memory(include_bytes!("../assets/icon-256.png"))
        .map(|image| image.to_rgba8());
    match decoded {
        Ok(rgba) => std::sync::Arc::new(egui::IconData {
            width: rgba.width(),
            height: rgba.height(),
            rgba: rgba.into_raw(),
        }),
        Err(_) => std::sync::Arc::new(egui::IconData::default()),
    }
}
