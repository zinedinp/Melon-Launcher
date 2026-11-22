//! `launcher.json`: what the launcher remembers between runs.

use melon_base::i18n::languages::Language;
use melon_base::home;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE: &str = "launcher.json";
/// Heap for Java instances when nothing is set.
pub const DEFAULT_MEMORY_MB: u32 = 2048;

/// The first six fields are the ones the game client reads; their names must stay.
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Settings {
    /// The selected instance's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(default)]
    pub validation: bool,
    #[serde(default)]
    pub probe: bool,
    /// Leave when the game ends instead of coming back.
    #[serde(default)]
    pub exit_after_game: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets_dir: Option<String>,
    /// A local Melon client for instances set to the local binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub melon_binary: Option<String>,
    /// Language code (`de_de`); none until one is picked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Keep the launcher window open while the game runs.
    #[serde(default)]
    pub keep_open: bool,
    /// The Java runtime for Java instances without their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_memory_mb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm_args: Option<String>,
    /// Instance groups folded shut in the list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collapsed_groups: Vec<String>,
    /// The first-run setup is still to be finished or skipped.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub welcome: bool,
}

impl Settings {
    pub fn load(main: &Path) -> Self {
        melon_base::json_file::load(&main.join(FILE)).unwrap_or_default()
    }

    pub fn save(&self, main: &Path) {
        if let Err(e) = melon_base::json_file::save(&main.join(FILE), self) {
            eprintln!("{FILE} not saved: {e}");
        }
    }

    /// Base folder for extracted assets: the chosen one, else the personal folder, else the
    /// folder older versions extracted into next to the launcher.
    pub fn assets_dir(&self, main: &Path) -> PathBuf {
        if let Some(path) = self.assets_dir.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            return PathBuf::from(path);
        }
        let personal = home::default_assets_dir();
        if home::is_prepared_at(&home::version_assets(&personal, home::VERSION))
            || home::is_prepared_at(&personal)
            || !home::is_prepared(main)
        {
            personal
        } else {
            home::assets_root(main)
        }
    }

    /// Where the Melon client's assets are extracted to.
    pub fn melon_extract(&self, main: &Path) -> PathBuf {
        home::version_assets(&self.assets_dir(main), home::VERSION)
    }

    /// The assets folder the Melon client reads: the version's, or the flat one of old.
    pub fn melon_play_assets(&self, main: &Path) -> PathBuf {
        let versioned = self.melon_extract(main);
        let base = self.assets_dir(main);
        if !home::is_prepared_at(&versioned) && home::is_prepared_at(&base) {
            base
        } else {
            versioned
        }
    }

    pub fn set_assets_path(&mut self, home: &Path, path: &Path) {
        let path = home::assets_base_from_pick(path);
        self.assets_dir = (path != home::default_assets_dir())
            .then(|| path.display().to_string());
        self.save(home);
    }

    /// A Melon client outside `versions/`: the configured one, else one found nearby.
    pub fn local_binary(&self, home: &Path) -> Option<PathBuf> {
        self.melon_binary
            .as_deref()
            .map(PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| melon_base::prepare::scan_binaries(home).map(|found| found.path))
    }

    /// The launcher's language: the picked one, else what the game used, else English.
    pub fn language(&self, home: &Path) -> Language {
        self.language
            .as_deref()
            .and_then(Language::from_code)
            .or_else(|| legacy_language(home))
            .unwrap_or(Language::EnUs)
    }

    pub fn memory_mb(&self) -> u32 {
        self.java_memory_mb.unwrap_or(DEFAULT_MEMORY_MB)
    }
}

/// The `lang:` of the options the launcher used to read its language from.
fn legacy_language(home: &Path) -> Option<Language> {
    ["global", "instances/melon"].iter().find_map(|dir| {
        let text = std::fs::read_to_string(home.join(dir).join("options.txt")).ok()?;
        let code = text.lines().find_map(|line| line.strip_prefix("lang:"))?;
        Language::from_code(code.trim())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_json_keeps_the_fields_the_client_reads() {
        let main = crate::store::scratch("settings");
        let settings = Settings {
            instance: Some("survival".into()),
            assets_dir: Some("D:/assets".into()),
            keep_open: true,
            language: Some("de_de".into()),
            ..Settings::default()
        };
        settings.save(&main);
        let text = std::fs::read_to_string(main.join(FILE)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["instance"], "survival");
        assert_eq!(json["assets_dir"], "D:/assets");
        assert_eq!(settings.assets_dir(&main), PathBuf::from("D:/assets"));
        let again = Settings::load(&main);
        assert!(again.keep_open);
        assert_eq!(again.language(&main), Language::DeDe);
        let _ = std::fs::remove_dir_all(main);
    }
}
