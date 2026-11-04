use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use web_time::Instant;

use egui::{Color32, ColorImage, Context, TextureHandle, TextureOptions};

use crate::settings::data_dir;

pub const THUMB_SIZE: u32 = 192;
const MEMORY_CAP: usize = 160;
const FAILURE_BACKOFF: Duration = Duration::from_secs(60);
const MAX_IMAGE_BYTES: usize = 6 * 1024 * 1024;

fn dir() -> PathBuf {
    data_dir().join("thumbs")
}

pub fn disk_path(key: &str) -> PathBuf {
    dir().join(format!("{key}.bin"))
}

pub fn load_disk(key: &str) -> Option<Vec<u8>> {
    let bytes = std::fs::read(disk_path(key)).ok()?;
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

pub fn store_disk(key: &str, bytes: &[u8]) {
    let _ = std::fs::create_dir_all(dir());
    let _ = std::fs::write(disk_path(key), bytes);
}

#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_bytes(source: &str) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    if !source.starts_with("http://") && !source.starts_with("https://") {
        let bytes = std::fs::read(source).map_err(|err| format!("cannot read {source}: {err}"))?;
        return if bytes.is_empty() { Err("empty file".to_string()) } else { Ok(bytes) };
    }
    let agent = crate::http::fetch_agent();
    let response = agent.get(source).call().map_err(|err| err.to_string())?;
    let mut bytes = Vec::new();
    response
        .into_body()
        .into_reader()
        .take(MAX_IMAGE_BYTES as u64)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("download interrupted: {err}"))?;
    if bytes.is_empty() {
        Err("empty response".to_string())
    } else {
        Ok(bytes)
    }
}

pub fn square_crop(width: u32, height: u32, letterboxed: bool) -> (u32, u32, u32) {
    let usable_height = if letterboxed && width * 3 == height * 4 {
        height * 3 / 4
    } else {
        height
    };
    let side = width.min(usable_height);
    let x = (width - side) / 2;
    let y = (height - side) / 2;
    (x, y, side)
}

pub fn decode(bytes: &[u8], letterboxed: bool) -> Result<ColorImage, String> {
    let image = image::load_from_memory(bytes).map_err(|err| format!("decode failed: {err}"))?;
    let (x, y, side) = square_crop(image.width(), image.height(), letterboxed);
    let small = image
        .crop_imm(x, y, side.max(1), side.max(1))
        .resize(THUMB_SIZE, THUMB_SIZE, image::imageops::FilterType::Triangle);
    let rgba = small.to_rgba8();
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;
    Ok(ColorImage::from_rgba_unmultiplied([width, height], &rgba))
}

pub struct ThumbJob {
    pub track_id: u64,
    pub source: String,
    pub letterboxed: bool,
}

#[derive(Clone, Default)]
pub struct ThumbQueue {
    inner: Arc<(Mutex<Vec<ThumbJob>>, Condvar)>,
}

impl ThumbQueue {
    pub fn push(&self, job: ThumbJob) {
        let (lock, ready) = &*self.inner;
        lock.lock().unwrap_or_else(|err| err.into_inner()).push(job);
        ready.notify_one();
        #[cfg(target_arch = "wasm32")]
        crate::jobs::pump_thumbs();
    }

    #[cfg(target_arch = "wasm32")]
    pub fn pop(&self) -> Option<ThumbJob> {
        let (lock, _) = &*self.inner;
        lock.lock().unwrap_or_else(|err| err.into_inner()).pop()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn pop_blocking(&self) -> ThumbJob {
        let (lock, ready) = &*self.inner;
        let mut jobs = lock.lock().unwrap_or_else(|err| err.into_inner());
        loop {
            if let Some(job) = jobs.pop() {
                return job;
            }
            jobs = ready.wait(jobs).unwrap_or_else(|err| err.into_inner());
        }
    }

}

pub fn dominant_color(image: &ColorImage) -> Color32 {
    let [width, height] = image.size;
    let step = ((width * height) / 900).max(1);
    let (mut weighted, mut weight, mut plain, mut count) = ([0.0f32; 3], 0.0f32, [0.0f32; 3], 0.0f32);
    for pixel in image.pixels.iter().step_by(step) {
        let rgb = [pixel.r() as f32, pixel.g() as f32, pixel.b() as f32];
        let high = rgb.iter().cloned().fold(0.0, f32::max);
        let low = rgb.iter().cloned().fold(255.0, f32::min);
        let vividness = if high > 0.0 { (high - low) / high } else { 0.0 } * (high / 255.0);
        for channel in 0..3 {
            weighted[channel] += rgb[channel] * vividness;
            plain[channel] += rgb[channel];
        }
        weight += vividness;
        count += 1.0;
    }
    if count == 0.0 {
        return Color32::from_rgb(40, 40, 48);
    }
    let rgb = if weight > 1.0 { [weighted[0] / weight, weighted[1] / weight, weighted[2] / weight] } else { [plain[0] / count, plain[1] / count, plain[2] / count] };
    let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    let scale = if luma > 85.0 { 85.0 / luma } else { 1.0 };
    Color32::from_rgb((rgb[0] * scale) as u8, (rgb[1] * scale) as u8, (rgb[2] * scale) as u8)
}

pub struct ThumbnailCache {
    tints: HashMap<u64, Color32>,
    textures: HashMap<u64, TextureHandle>,
    order: VecDeque<u64>,
    pending: Vec<u64>,
    failed: HashMap<u64, Instant>,
    cap: usize,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self {
            tints: HashMap::new(),
            textures: HashMap::new(),
            order: VecDeque::new(),
            pending: Vec::new(),
            failed: HashMap::new(),
            cap: MEMORY_CAP,
        }
    }
}

impl ThumbnailCache {
    pub fn get(&mut self, id: u64) -> Option<TextureHandle> {
        let handle = self.textures.get(&id).cloned()?;
        if let Some(position) = self.order.iter().position(|entry| *entry == id) {
            self.order.remove(position);
            self.order.push_back(id);
        }
        Some(handle)
    }

    pub fn tint(&self, id: u64) -> Option<Color32> {
        self.tints.get(&id).copied()
    }

    pub fn insert(&mut self, id: u64, ctx: &Context, image: ColorImage) {
        if self.tints.len() > 600 {
            self.tints.clear();
        }
        self.tints.insert(id, dominant_color(&image));
        let handle = ctx.load_texture(
            format!("aria-thumb-{id}"),
            image,
            TextureOptions::LINEAR,
        );
        self.textures.insert(id, handle);
        self.order.retain(|entry| *entry != id);
        self.order.push_back(id);
        while self.order.len() > self.cap {
            if let Some(oldest) = self.order.pop_front() {
                self.textures.remove(&oldest);
            }
        }
    }

    pub fn is_pending(&self, id: u64) -> bool {
        self.pending.contains(&id)
    }

    pub fn mark_pending(&mut self, id: u64) {
        if !self.pending.contains(&id) {
            self.pending.push(id);
        }
    }

    pub fn clear_pending(&mut self, id: u64) {
        self.pending.retain(|entry| *entry != id);
    }

    pub fn mark_failed(&mut self, id: u64) {
        self.failed.insert(id, Instant::now());
    }

    pub fn recently_failed(&self, id: u64) -> bool {
        self.failed
            .get(&id)
            .is_some_and(|when| when.elapsed() < FAILURE_BACKOFF)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dominant_color_follows_the_vivid_part_of_a_cover_and_stays_dark_enough_for_white_text() {
        let mut pixels = vec![egui::Color32::from_rgb(235, 235, 235); 64 * 64];
        for pixel in pixels.iter_mut().take(64 * 20) {
            *pixel = egui::Color32::from_rgb(220, 30, 40);
        }
        let image = egui::ColorImage { size: [64, 64], pixels, source_size: egui::vec2(64.0, 64.0) };
        let tint = super::dominant_color(&image);
        assert!(tint.r() > tint.g() + 30 && tint.r() > tint.b() + 30, "{tint:?}");
        let luma = 0.2126 * tint.r() as f32 + 0.7152 * tint.g() as f32 + 0.0722 * tint.b() as f32;
        assert!(luma <= 90.0, "{luma}");
        let grey = egui::ColorImage { size: [8, 8], pixels: vec![egui::Color32::from_gray(200); 64], source_size: egui::vec2(8.0, 8.0) };
        let tint = super::dominant_color(&grey);
        assert!(tint.r().abs_diff(tint.g()) < 3 && tint.r() <= 90);
    }

    use super::*;

    #[test]
    fn crops_to_a_centered_square() {
        assert_eq!(square_crop(500, 500, false), (0, 0, 500));
        assert_eq!(square_crop(640, 360, false), (140, 0, 360));
        assert_eq!(square_crop(300, 600, false), (0, 150, 300));
    }

    #[test]
    fn youtube_letterbox_bars_are_cut_away() {
        let (x, y, side) = square_crop(480, 360, true);
        assert_eq!(side, 270);
        assert_eq!((x, y), (105, 45));
        assert_eq!(square_crop(480, 360, false).2, 360);
        assert_eq!(square_crop(640, 360, true).2, 360);
    }

    #[test]
    fn newest_request_is_served_first() {
        let queue = ThumbQueue::default();
        for id in 1..=3 {
            queue.push(ThumbJob { track_id: id, source: String::new(), letterboxed: false });
        }
        assert_eq!(queue.pop_blocking().track_id, 3);
        assert_eq!(queue.pop_blocking().track_id, 2);
    }

    #[test]
    fn failures_are_remembered_so_they_are_not_retried_every_frame() {
        let mut cache = ThumbnailCache::default();
        assert!(!cache.recently_failed(7));
        cache.mark_failed(7);
        assert!(cache.recently_failed(7));
        assert!(!cache.recently_failed(8));
    }

    #[test]
    fn decode_reports_why_bad_data_fails() {
        let err = decode(b"not an image", false).unwrap_err();
        assert!(err.starts_with("decode failed"), "{err}");
        assert!(fetch_bytes("/definitely/not/here.jpg").unwrap_err().contains("cannot read"));
    }
}
