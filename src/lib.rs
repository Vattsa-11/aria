mod app;
mod cache;
mod db;
mod dialogs;
mod downloads;
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
mod player;
mod proc;
mod queue;
mod radio;
mod region;
mod settings;
mod sleep;
mod stats;
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

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("aria.player")
            .with_title("Aria")
            .with_icon(window_icon())
            .with_inner_size([1180.0, 780.0])
            .with_min_inner_size([420.0, 420.0]),
        renderer: eframe::Renderer::Glow,
        persist_window: true,
        centered: true,
        ..Default::default()
    }
}

fn window_icon() -> std::sync::Arc<egui::IconData> {
    const SIZE: u32 = 64;
    let accent = [16u8, 185, 165];
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let center = (SIZE as f32 - 1.0) * 0.5;
    let radius = SIZE as f32 * 0.5 - 2.0;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > radius + 1.0 {
                continue;
            }
            let coverage = (radius - distance).clamp(0.0, 1.0);
            let play = dx > -9.0 && dx < 12.0 && dy.abs() < (12.0 - (dx + 9.0) * 0.42).max(0.0);
            let (r, g, b) = if play {
                (246, 248, 250)
            } else {
                (accent[0], accent[1], accent[2])
            };
            let index = ((y * SIZE + x) * 4) as usize;
            rgba[index] = r;
            rgba[index + 1] = g;
            rgba[index + 2] = b;
            rgba[index + 3] = (coverage * 255.0).round() as u8;
        }
    }

    std::sync::Arc::new(egui::IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    })
}
