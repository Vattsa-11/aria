use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

use crate::settings::data_dir;

pub const THUMB_SIZE: u32 = 96;
const MEMORY_CAP: usize = 128;

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

pub fn fetch_bytes(url: &str) -> Option<Vec<u8>> {
    let agent = crate::http::fetch_agent();
    let (parts, body) = agent.get(url).call().ok()?.into_parts();
    let _ = parts;
    let mut reader = body.into_reader();
    let mut bytes = Vec::new();
    use std::io::Read as _;
    if reader.read_to_end(&mut bytes).is_err() {
        return None;
    }
    if bytes.len() > 4 * 1024 * 1024 {
        bytes.truncate(4 * 1024 * 1024);
    }
    if bytes.is_empty() {
        None
    } else {
        Some(bytes)
    }
}

pub fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let image = image::load_from_memory(bytes).ok()?;
    let small = image.thumbnail(THUMB_SIZE, THUMB_SIZE);
    let rgba = small.to_rgba8();
    let width = rgba.width() as usize;
    let height = rgba.height() as usize;
    Some(ColorImage::from_rgba_unmultiplied(
        [width, height],
        &rgba,
    ))
}

pub struct ThumbnailCache {
    textures: HashMap<u64, TextureHandle>,
    order: VecDeque<u64>,
    pending: Vec<u64>,
    cap: usize,
}

impl Default for ThumbnailCache {
    fn default() -> Self {
        Self {
            textures: HashMap::new(),
            order: VecDeque::new(),
            pending: Vec::new(),
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

    pub fn insert(&mut self, id: u64, ctx: &Context, image: ColorImage) {
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
}
