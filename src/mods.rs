//! The mod zips of an instance and which of them are switched off. The game builds and loads
//! them; the launcher only lists them.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};

const MANIFEST: &str = "melon-mod.toml";
const STATE_FILE: &str = "mods.json";
/// A mod's icon, in the zip's root.
pub const ICON: &str = "icon.png";

/// What the launcher shows of a `melon-mod.toml`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// `client`, `server` or `both`.
    #[serde(default = "both")]
    pub side: String,
    /// `none`, `wasm`, `source` or `prebuilt`.
    #[serde(default = "none")]
    pub code: String,
}

fn both() -> String {
    "both".into()
}

fn none() -> String {
    "none".into()
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.message().to_owned())
    }

    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

fn read_manifest(zip: &Path) -> Result<Manifest, String> {
    let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut text = String::new();
    archive
        .by_name(MANIFEST)
        .map_err(|_| format!("no {MANIFEST}"))?
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    Manifest::parse(&text)
}

/// The zips in `mods_dir` with their manifests, by file name.
pub fn list(mods_dir: &Path) -> Vec<(PathBuf, Result<Manifest, String>)> {
    let mut zips: Vec<PathBuf> = std::fs::read_dir(mods_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")))
        .collect();
    zips.sort();
    zips.into_iter()
        .map(|path| {
            let manifest = read_manifest(&path);
            (path, manifest)
        })
        .collect()
}

/// `mods.json`: the ids of the mods switched off, as the game reads it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    pub disabled: Vec<String>,
}

impl State {
    pub fn load(game_dir: &Path) -> Self {
        melon_base::json_file::load(&game_dir.join(STATE_FILE)).unwrap_or_default()
    }

    pub fn save(&self, game_dir: &Path) -> std::io::Result<()> {
        melon_base::json_file::save(&game_dir.join(STATE_FILE), self)
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        !self.disabled.iter().any(|d| d == id)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        self.disabled.retain(|d| d != id);
        if !enabled {
            self.disabled.push(id.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_zip_is_listed_with_its_description() {
        let dir = crate::store::scratch("mods");
        let file = std::fs::File::create(dir.join("hello.zip")).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file(MANIFEST, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"id = \"hello\"\nversion = \"0.1.0\"\ndescription = \"Says hello\"\nside = \"client\"\n")
            .unwrap();
        zip.finish().unwrap();
        std::fs::write(dir.join("broken.zip"), b"not a zip").unwrap();
        std::fs::write(dir.join("notes.txt"), b"").unwrap();

        let found = list(&dir);
        assert_eq!(found.len(), 2);
        assert!(found[0].1.is_err());
        let manifest = found[1].1.as_ref().unwrap();
        assert_eq!(manifest.display_name(), "hello");
        assert_eq!(manifest.description.as_deref(), Some("Says hello"));
        assert_eq!((manifest.side.as_str(), manifest.code.as_str()), ("client", "none"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn switching_a_mod_off_is_remembered() {
        let dir = crate::store::scratch("mods-state");
        let mut state = State::load(&dir);
        assert!(state.is_enabled("hello"));
        state.set_enabled("hello", false);
        state.save(&dir).unwrap();
        assert!(!State::load(&dir).is_enabled("hello"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
