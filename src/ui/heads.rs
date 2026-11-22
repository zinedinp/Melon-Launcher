//! The faces of Microsoft accounts: their skin's head, fetched in the background and cached.
//! Offline profiles and accounts whose skin cannot be fetched show an initial instead.

use image::RgbaImage;
use melon_base::auth::accounts::Account;
use melon_base::auth::skin;
use melon_base::gfx::{Renderer2d, TextureId};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime};

/// How long a fetched head is used before the skin is asked for again.
const FRESH: Duration = Duration::from_secs(24 * 60 * 60);

pub struct Heads {
    textures: HashMap<String, TextureId>,
    requested: HashSet<String>,
    sender: Sender<(String, Option<RgbaImage>)>,
    arrived: Receiver<(String, Option<RgbaImage>)>,
    cache: PathBuf,
}

impl Heads {
    pub fn new(launcher_dir: &Path) -> Self {
        let (sender, arrived) = channel();
        Self {
            textures: HashMap::new(),
            requested: HashSet::new(),
            sender,
            arrived,
            cache: launcher_dir.join("cache").join("heads"),
        }
    }

    /// The face of the account with this key, once it is loaded.
    pub fn get(&self, key: &str) -> Option<TextureId> {
        self.textures.get(key).copied()
    }

    /// Takes in the faces that arrived and asks for those still missing.
    pub fn update(&mut self, accounts: &[Account], renderer: &mut Renderer2d) {
        while let Ok((key, head)) = self.arrived.try_recv() {
            self.upload(renderer, key, head);
        }
        for account in accounts {
            let Account::Microsoft(stored) = account else {
                continue;
            };
            let key = account.key();
            if !self.requested.insert(key.clone()) {
                continue;
            }
            let file = self.cache.join(format!("{}.png", stored.uuid));
            match fresh_cache(&file) {
                Some(head) => self.upload(renderer, key, Some(head)),
                None => self.fetch(key, file),
            }
        }
    }

    fn upload(&mut self, renderer: &mut Renderer2d, key: String, head: Option<RgbaImage>) {
        if let Some(texture) = head.and_then(|head| renderer.load_texture(&head).ok()) {
            self.textures.insert(key, texture);
        }
    }

    fn fetch(&self, uuid: String, file: PathBuf) {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let head = match skin::fetch_head(&uuid) {
                Ok(head) => {
                    if let Some(dir) = file.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    let _ = head.save(&file);
                    Some(head)
                }
                Err(_) => image::open(&file).ok().map(|image| image.to_rgba8()),
            };
            let _ = sender.send((uuid, head));
        });
    }
}

/// The cached head, if it is younger than [`FRESH`].
fn fresh_cache(file: &Path) -> Option<RgbaImage> {
    let age = SystemTime::now()
        .duration_since(std::fs::metadata(file).ok()?.modified().ok()?)
        .ok()?;
    (age < FRESH)
        .then(|| image::open(file).ok().map(|image| image.to_rgba8()))
        .flatten()
}
