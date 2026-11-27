//! The release catalog (`discover.json` on the CDN): fetched at run time, never baked in, and
//! only taken with a valid signature (`discover.json.minisig`) by the CDN's key.

use crate::cdn::{self, Cdn, fetch_text};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const CACHE: &str = "discover.json";
const CACHE_SIGNATURE: &str = "discover.json.minisig";

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

/// SHA-256 of the binary of `version` on `platform` among `releases`.
fn sha_in(releases: &[Release], version: &str, platform: &str) -> Option<String> {
    releases
        .iter()
        .find(|r| r.version == version)
        .and_then(|r| r.platforms.get(platform))
        .and_then(|p| p.sha256.clone())
}

/// SHA-256 of the published game binary for `version` on this platform.
pub fn game_sha(version: &str, platform: &str) -> Option<String> {
    sha_in(&current().game, version, platform)
}

/// SHA-256 of the published launcher binary for `version` on this platform.
pub fn launcher_sha(version: &str, platform: &str) -> Option<String> {
    sha_in(&current().launcher, version, platform)
}

/// Latest published launcher version; `None` while no catalog is known.
pub fn latest_launcher() -> Option<String> {
    Some(current().latest.launcher).filter(|v| !v.is_empty())
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

/// The cached catalog and its signature, if `cdn`'s key signed it.
fn cached(home: &Path, cdn: &Cdn) -> Option<(Catalog, String)> {
    let bytes = std::fs::read(cache_path(home)).ok()?;
    let signature = std::fs::read_to_string(home.join(CACHE_SIGNATURE)).ok()?;
    cdn.verify(&bytes, &signature).ok()?;
    Some((parse_bytes(&bytes).ok()?, signature))
}

/// Loads the cached catalog, if an earlier run saved one the CDN's key signed.
pub fn load(home: &Path) {
    match cached(home, &cdn::current()) {
        Some((catalog, _)) => set(catalog, Source::Cache),
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

/// The signature first: when it is the cached one, the cached catalog is current.
fn try_refresh(home: &Path) -> Result<Catalog, String> {
    let cdn = cdn::current();
    let signature = fetch_text(&format!("{}/{CACHE_SIGNATURE}", cdn.origin))?;
    if let Some((catalog, cached_signature)) = cached(home, &cdn)
        && cached_signature == signature
    {
        return Ok(catalog);
    }
    let body = fetch_text(&format!("{}/{CACHE}", cdn.origin))?;
    cdn.verify(body.as_bytes(), &signature)
        .map_err(|e| format!("discover.json: {e}"))?;
    let catalog = parse_bytes(body.as_bytes())?;
    let saved = std::fs::write(cache_path(home), body.as_bytes())
        .and_then(|()| std::fs::write(home.join(CACHE_SIGNATURE), &signature));
    if let Err(e) = saved {
        eprintln!("discover.json not saved: {e}");
    }
    Ok(catalog)
}

fn parse_bytes(bytes: &[u8]) -> Result<Catalog, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    serde_json::from_str(text).map_err(|e| e.to_string())
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
    fn only_a_cache_the_cdn_key_signed_is_used() {
        let home = crate::store::scratch("catalog-cache");
        let key = crate::cdn::tests::TestKey::new();
        let body = br#"{"latest":{"game":"0.7.6+mc26.3","launcher":""}}"#;
        std::fs::write(cache_path(&home), body).unwrap();
        std::fs::write(home.join(CACHE_SIGNATURE), key.sign(body)).unwrap();
        let (catalog, _) = cached(&home, &key.cdn()).expect("signed cache");
        assert_eq!(catalog.latest.game, "0.7.6+mc26.3");
        assert!(cached(&home, &crate::cdn::tests::TestKey::new().cdn()).is_none());
        std::fs::write(cache_path(&home), br#"{"latest":{"game":"9.9.9","launcher":""}}"#).unwrap();
        assert!(cached(&home, &key.cdn()).is_none(), "an edited cache is refused");
        let _ = std::fs::remove_dir_all(home);
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
