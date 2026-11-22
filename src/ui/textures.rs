//! Icons the window shows: instance icons and the icons inside mod zips.

use image::RgbaImage;
use melon_base::gfx::{Renderer2d, TextureId};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

/// The biggest icon side that is uploaded as it is; larger ones are scaled down.
const MAX_SIDE: u32 = 128;

#[derive(Default)]
pub struct Textures {
    /// By key; `None` while waiting for the upload or when there is no image.
    loaded: HashMap<String, Option<TextureId>>,
    waiting: Vec<(String, RgbaImage)>,
}

impl Textures {
    /// The texture for `key`; `load` runs once, the first time the key is asked for.
    pub fn get(&mut self, key: &str, load: impl FnOnce() -> Option<RgbaImage>) -> Option<TextureId> {
        if let Some(known) = self.loaded.get(key) {
            return *known;
        }
        if let Some(image) = load() {
            self.waiting.push((key.to_owned(), image));
        }
        self.loaded.insert(key.to_owned(), None);
        None
    }

    /// Forgets `key`, so a changed file is read again.
    pub fn forget(&mut self, key: &str) {
        self.loaded.remove(key);
    }

    /// Uploads the images read since the last frame.
    pub fn update(&mut self, renderer: &mut Renderer2d) {
        for (key, image) in self.waiting.drain(..) {
            self.loaded.insert(key, renderer.load_texture(&image).ok());
        }
    }
}

fn bounded(image: image::DynamicImage) -> RgbaImage {
    if image.width() > MAX_SIDE || image.height() > MAX_SIDE {
        image
            .resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
            .to_rgba8()
    } else {
        image.to_rgba8()
    }
}

/// A PNG file as an icon.
pub fn png_file(path: &Path) -> Option<RgbaImage> {
    image::open(path).ok().map(bounded)
}

/// `name` inside a zip as an icon.
pub fn png_in_zip(zip: &Path, name: &str) -> Option<RgbaImage> {
    let file = std::fs::File::open(zip).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(name).ok()?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).ok()?;
    image::load_from_memory(&bytes).ok().map(bounded)
}
