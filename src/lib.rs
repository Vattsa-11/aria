mod agent;
mod app;
mod cache;
mod db;
mod dialogs;
mod doctor;
mod downloads;
mod embedded;
mod net;
mod equalizer;
#[cfg(not(target_arch = "wasm32"))]
mod http;
#[cfg(target_arch = "wasm32")]
#[path = "web/http.rs"]
mod http;
#[cfg(not(target_arch = "wasm32"))]
mod httpdl;
mod import;
mod jobs;
mod live;
mod update;
mod logs;
mod lyrics;
mod matchscore;
#[cfg(not(target_arch = "wasm32"))]
mod media_controls;
#[cfg(target_arch = "wasm32")]
#[path = "web/media_controls.rs"]
mod media_controls;
mod miniplayer;
mod model;
#[cfg(not(target_arch = "wasm32"))]
mod normalize;
mod pace;
mod player;
mod proc;
mod queue;
#[cfg(not(target_arch = "wasm32"))]
mod radio;
mod region;
mod settings;
mod sleep;
mod stats;
mod suggest;
#[cfg(target_arch = "wasm32")]
#[path = "web/storage.rs"]
mod storage;
#[cfg(target_arch = "wasm32")]
#[path = "web/os.rs"]
mod os;
#[cfg(target_arch = "wasm32")]
#[path = "web/install.rs"]
mod install;
mod ui;
mod writer;
mod ytdlp;
mod provider;
#[cfg(not(target_arch = "wasm32"))]
mod provider_youtube_music;
#[cfg(not(target_arch = "wasm32"))]
mod provider_jiosaavn;
#[cfg(not(target_arch = "wasm32"))]
mod provider_youtube;

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
fn preview_size() -> Option<[f32; 2]> {
    let value = std::env::var("ARIA_WINDOW").ok()?;
    let (width, height) = value.split_once('x')?;
    let (width, height) = (width.trim().parse::<f32>().ok()?, height.trim().parse::<f32>().ok()?);
    ((200.0..=4000.0).contains(&width) && (200.0..=4000.0).contains(&height)).then_some([width, height])
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn open_library() -> bool {
    match sqlite_wasm_vfs::relaxed_idb::install::<os::WebOs>(&sqlite_wasm_vfs::relaxed_idb::RelaxedIdbCfg::default(), true).await {
        Ok(_) => true,
        Err(error) => {
            web_sys::console::warn_1(&format!("Saved library unavailable, using memory: {error:?}").into());
            false
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn start(canvas_id: String) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;
    std::panic::set_hook(Box::new(|info| web_sys::console::error_1(&info.to_string().into())));
    let document = web_sys::window().and_then(|window| window.document()).ok_or("no document")?;
    let canvas = document
        .get_element_by_id(&canvas_id)
        .ok_or("canvas not found")?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .map_err(|_| "not a canvas")?;
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|cc| {
                jobs::set_context(&cc.egui_ctx);
                Ok(Box::new(app::App::new(cc)))
            }),
        )
        .await
}
