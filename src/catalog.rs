//! The release catalog (`discover.json` on the CDN): fetched at run time, never baked in.

use crate::cdn::{ORIGIN, fetch_text, hex, parse_sha256_line};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const CACHE: &str = "discover.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Catalog {
    #[serde(default)]
    pub latest: Latest,
    #[serde(default)]
    pub game: Vec<Release>,
    #[serde(default)]
    pub launcher: Vec<Release>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Latest {
    #[serde(default)]
    pub game: String,
    #[serde(default)]
    pub launcher: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    #[serde(default)]
    pub platforms: BTreeMap<String, Platform>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Platform {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Where the catalog in memory came from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    /// Nothing cached and the CDN was not reached: no versions are known.
    #[default]
    None,
    /// The copy cached by an earlier run.
    Cache,
    /// Checked against the CDN in this run.
    Online,
}

#[derive(Default)]
struct Slot {
    catalog: Catalog,
    source: Source,
}

fn slot() -> &'static Mutex<Slot> {
    static CURRENT: OnceLock<Mutex<Slot>> = OnceLock::new();
    CURRENT.get_or_init(Mutex::default)
}

pub fn current() -> Catalog {
    slot().lock().map(|s| s.catalog.clone()).unwrap_or_default()
}

pub fn source() -> Source {
    slot().lock().map(|s| s.source).unwrap_or_default()
}

fn set(catalog: Catalog, source: Source) {
    if let Ok(mut slot) = slot().lock() {
        *slot = Slot { catalog, source };
    }
}

/// Latest published game version; `None` while no catalog is known.
pub fn latest_game() -> Option<String> {
    Some(current().latest.game).filter(|v| !v.is_empty())
}

/// Published game versions, newest first.
pub fn game_versions() -> Vec<String> {
    let catalog = current();
    let mut versions: Vec<String> = catalog.game.into_iter().map(|r| r.version).collect();
    if !catalog.latest.game.is_empty() && !versions.contains(&catalog.latest.game) {
        versions.push(catalog.latest.game);
    }
    versions.sort_by(|a, b| compare_versions(b, a));
    versions
}

/// SHA-256 of the published game binary for `version` on this platform.
pub fn game_sha(version: &str, platform: &str) -> Option<String> {
    current()
        .game
        .iter()
        .find(|r| r.version == version)
        .and_then(|r| r.platforms.get(platform))
        .and_then(|p| p.sha256.clone())
}

/// A published launcher version newer than this build.
pub fn launcher_update() -> Option<String> {
    let latest = current().latest.launcher;
    (!latest.is_empty()
        && compare_versions(&latest, env!("CARGO_PKG_VERSION")) == std::cmp::Ordering::Greater)
        .then_some(latest)
}

/// Orders versions by their numeric parts; build metadata (`+mc26.3`) is ignored.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |v: &str| -> Vec<u64> {
        v.split('+')
            .next()
            .unwrap_or_default()
            .split(['.', '-'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    parts(a).cmp(&parts(b)).then_with(|| a.cmp(b))
}

fn cache_path(home: &Path) -> PathBuf {
    home.join(CACHE)
}

fn cache_bytes(home: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(cache_path(home)).ok()?;
    parse_bytes(&bytes).ok()?;
    Some(bytes)
}

/// Loads the cached catalog, if an earlier run saved one.
pub fn load(home: &Path) {
    match cache_bytes(home).and_then(|bytes| parse_bytes(&bytes).ok()) {
        Some(catalog) => set(catalog, Source::Cache),
        None => set(Catalog::default(), Source::None),
    }
}

/// Asks the CDN for the catalog; keeps what is loaded when it cannot be reached.
pub fn refresh(home: &Path) {
    match try_refresh(home) {
        Ok(catalog) => set(catalog, Source::Online),
        Err(error) => eprintln!("catalog: {error}"),
    }
}

fn try_refresh(home: &Path) -> Result<Catalog, String> {
    let local = cache_bytes(home);
    let sha_url = format!("{ORIGIN}/discover.json.sha256");
    let remote_hash = fetch_text(&sha_url).and_then(|text| {
        parse_sha256_line(&text).ok_or_else(|| "discover.json.sha256 is not a hash".into())
    })?;
    if let Some(local) = &local
        && sha256_bytes(local) == remote_hash
    {
        return parse_bytes(local);
    }
    let body = fetch_text(&format!("{ORIGIN}/discover.json"))?;
    let have = sha256_bytes(body.as_bytes());
    if have != remote_hash {
        return Err(format!("discover.json sha256 mismatch (got {have})"));
    }
    let catalog = parse_bytes(body.as_bytes())?;
    if let Err(e) = std::fs::write(cache_path(home), body.as_bytes()) {
        eprintln!("discover.json not saved: {e}");
    }
    Ok(catalog)
}

fn parse_bytes(bytes: &[u8]) -> Result<Catalog, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    serde_json::from_str(text).map_err(|e| e.to_string())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(hasher.finalize().as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn versions_compare_by_number() {
        assert_eq!(compare_versions("0.10.0+mc26.3", "0.9.7+mc26.3"), Ordering::Greater);
        assert_eq!(compare_versions("0.7.5", "0.7.6"), Ordering::Less);
        assert_eq!(compare_versions("0.1.1", "0.1.1"), Ordering::Equal);
    }

    #[test]
    fn an_empty_catalog_parses_and_knows_nothing() {
        let catalog = parse_bytes(b"{}").unwrap();
        assert_eq!(catalog, Catalog::default());
        assert!(catalog.latest.game.is_empty());
    }

    #[test]
    fn a_catalog_file_parses() {
        let text = br#"{"latest":{"game":"0.7.6+mc26.3","launcher":"0.1.0"},
            "game":[{"version":"0.7.6+mc26.3","platforms":{"linux-x86-64":{"sha256":"ab"}}}]}"#;
        let catalog = parse_bytes(text).unwrap();
        assert_eq!(catalog.latest.game, "0.7.6+mc26.3");
        assert_eq!(catalog.game[0].platforms["linux-x86-64"].sha256.as_deref(), Some("ab"));
    }
}
