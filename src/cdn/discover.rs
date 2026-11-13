//! `https://elbengeist.moe/melonmc/cdn/discover.json`: which game and

use super::{ORIGIN, fetch_text, hex, parse_sha256_line, sha256_file};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const BAKED: &str = include_str!("../../discover.json");
const CACHE: &str = "discover.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Discover {
    pub latest: Latest,
    #[serde(default)]
    pub game: Vec<Release>,
    #[serde(default)]
    pub launcher: Vec<Release>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Latest {
    pub game: String,
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

fn slot() -> &'static Mutex<Discover> {
    static CURRENT: OnceLock<Mutex<Discover>> = OnceLock::new();
    CURRENT.get_or_init(|| Mutex::new(baked()))
}

/// Catalog last loaded or fetched. Falls back to the baked baseline.
pub fn current() -> Discover {
    slot().lock().ok().map(|g| g.clone()).unwrap_or_else(baked)
}

pub fn set_current(discover: Discover) {
    if let Ok(mut g) = slot().lock() {
        *g = discover;
    }
}

/// Latest game version the launcher should offer (discover, then this crate).
pub fn latest_game() -> String {
    let version = current().latest.game;
    if version.is_empty() {
        super::game_version().to_owned()
    } else {
        version
    }
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

pub fn baked() -> Discover {
    parse_bytes(BAKED.as_bytes()).unwrap_or_else(|_| Discover {
        latest: Latest {
            game: super::game_version().into(),
            launcher: String::new(),
        },
        game: Vec::new(),
        launcher: Vec::new(),
    })
}

fn cache_path(home: &Path) -> PathBuf {
    home.join(CACHE)
}

fn cache_bytes(home: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(cache_path(home)).ok()?;
    parse_bytes(&bytes).ok()?;
    Some(bytes)
}

/// Cached discover.json, or the baked baseline.
pub fn load(home: &Path) -> Discover {
    if let Some(bytes) = cache_bytes(home)
        && let Ok(discover) = parse_bytes(&bytes)
    {
        return discover;
    }
    baked()
}

/// GET `discover.json.sha256`. Download `discover.json` only when the hash
pub fn refresh(home: &Path) -> Discover {
    match try_refresh(home) {
        Ok(discover) => discover,
        Err(error) => {
            eprintln!("discover: {error}");
            load(home)
        }
    }
}

fn try_refresh(home: &Path) -> Result<Discover, String> {
    let local = cache_bytes(home).unwrap_or_else(|| BAKED.as_bytes().to_vec());
    let local_hash = sha256_bytes(&local);
    let sha_url = format!("{ORIGIN}/discover.json.sha256");
    let remote_hash = fetch_text(&sha_url).and_then(|text| {
        parse_sha256_line(&text).ok_or_else(|| "discover.json.sha256 is not a hash".into())
    })?;
    if remote_hash == local_hash {
        return parse_bytes(&local);
    }
    let body = fetch_text(&format!("{ORIGIN}/discover.json"))?;
    let have = sha256_bytes(body.as_bytes());
    if have != remote_hash {
        return Err(format!("discover.json sha256 mismatch (got {have})"));
    }
    let discover = parse_bytes(body.as_bytes())?;
    if let Err(e) = std::fs::write(cache_path(home), body.as_bytes()) {
        eprintln!("discover.json konnte nicht gespeichert werden: {e}");
    }
    Ok(discover)
}

/// True when `home`'s game binary matches the latest published hash.
pub fn client_is_latest(home: &Path) -> bool {
    let dest = super::game_dest(home);
    if !dest.is_file() {
        return false;
    }
    let version = latest_game();
    let Some(expected) = game_sha(&version, super::platform_folder()) else {
        return true;
    };
    sha256_file(&dest).as_deref() == Some(expected.as_str())
}

fn parse_bytes(bytes: &[u8]) -> Result<Discover, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    serde_json::from_str(text).map_err(|e| e.to_string())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(hasher.finalize().as_ref())
}
