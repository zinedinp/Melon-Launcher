//! Where published Melon binaries live on the CDN and on disk.

use crate::store::settings::Settings;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// The official CDN origin, no trailing slash.
pub const ORIGIN: &str = "https://elbengeist.moe/melonmc/cdn";

/// The key the official CDN's catalog and hashes are signed with (`sign-cdn.py` next to
/// `build-dist.sh` on the build server), minisign's base64 form.
pub const OFFICIAL_KEY: &str = "RWSCE+X5fx1YD2Mv24ADsR2feosIbndDGzHiX9pX1GlhbkHpf3vU05tc";

/// Where the catalog and the binaries come from, and the key that has to vouch for them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cdn {
    /// No trailing slash.
    pub origin: String,
    key: String,
}

impl Cdn {
    pub fn official() -> Self {
        Self {
            origin: ORIGIN.to_owned(),
            key: OFFICIAL_KEY.to_owned(),
        }
    }

    /// The address and key `settings` set, the official ones where they set none.
    pub fn from_settings(settings: &Settings) -> Self {
        let set = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        let official = Self::official();
        Self {
            origin: set(&settings.cdn_origin)
                .map(|origin| origin.trim_end_matches('/').to_owned())
                .unwrap_or(official.origin),
            key: set(&settings.cdn_public_key)
                .and_then(|key| key_line(&key).map(str::to_owned))
                .unwrap_or(official.key),
        }
    }

    /// Checks that `signature` (a `.minisig` file's text) signs `data` with this CDN's key.
    pub fn verify(&self, data: &[u8], signature: &str) -> Result<(), String> {
        let key = minisign_verify::PublicKey::from_base64(&self.key)
            .map_err(|e| format!("CDN public key: {e}"))?;
        let signature =
            minisign_verify::Signature::decode(signature).map_err(|e| format!("signature: {e}"))?;
        key.verify(data, &signature, false)
            .map_err(|e| format!("signature: {e}"))
    }
}

/// The base64 line of a minisign public key: the line itself, or the last line of a
/// `.pub` file's text.
pub fn key_line(text: &str) -> Option<&str> {
    let line = text
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.starts_with("untrusted comment:"))?;
    minisign_verify::PublicKey::from_base64(line)
        .is_ok()
        .then_some(line)
}

fn current_slot() -> &'static RwLock<Option<Cdn>> {
    static CURRENT: RwLock<Option<Cdn>> = RwLock::new(None);
    &CURRENT
}

/// Sets the CDN for this run; the official one until this is called.
pub fn configure(cdn: Cdn) {
    if let Ok(mut slot) = current_slot().write() {
        *slot = Some(cdn);
    }
}

pub fn current() -> Cdn {
    current_slot()
        .read()
        .ok()
        .and_then(|slot| slot.clone())
        .unwrap_or_else(Cdn::official)
}

const UA: &str = concat!(
    "Melon/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/zinedinp/Melon-Launcher)"
);
const GET_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

/// CDN folder for this build's OS and arch (`linux-x86-64`, `windows-x86-64`, …).
pub fn platform_folder() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x86-64",
        ("linux", "aarch64") => "linux-aarch64",
        ("linux", "arm") => "linux-arm",
        ("linux", "riscv64") => "linux-riscv64",
        ("windows", "x86_64") => "windows-x86-64",
        ("windows", "aarch64") => "windows-aarch64",
        ("macos", "x86_64") => "macos-x86-64",
        ("macos", "aarch64") => "macos-aarch64",
        ("freebsd", "x86_64") => "freebsd-x86-64",
        ("freebsd", "aarch64") => "freebsd-aarch64",
        ("freebsd", "riscv64") => "freebsd-riscv64",
        _ => "linux-x86-64",
    }
}

/// File name of the game client on the CDN.
pub fn game_bin() -> &'static str {
    if cfg!(windows) { "melon.exe" } else { "melon" }
}

/// Directory of a game version: `…/cdn/game/<version>/<os-arch>/`.
pub fn game_url_for(cdn: &Cdn, version: &str) -> String {
    format!("{}/game/{version}/{}/", cdn.origin, platform_folder())
}

/// File name of the launcher on the CDN.
pub fn launcher_bin() -> &'static str {
    if cfg!(windows) { "melon-launcher.exe" } else { "melon-launcher" }
}

/// Directory of a launcher version: `…/cdn/launcher/<version>/<os-arch>/`.
pub fn launcher_url_for(cdn: &Cdn, version: &str) -> String {
    format!("{}/launcher/{version}/{}/", cdn.origin, platform_folder())
}

/// Where downloaded game clients live: `<melon folder>/versions/`.
pub fn versions_dir(home: &Path) -> PathBuf {
    home.join("versions")
}

/// The game client of `version`: `<melon folder>/versions/<version>/melon`.
pub fn game_path(home: &Path, version: &str) -> PathBuf {
    versions_dir(home).join(version).join(game_bin())
}

pub fn is_installed(home: &Path, version: &str) -> bool {
    game_path(home, version).is_file()
}

/// Versions with a client on disk, newest first.
pub fn installed_versions(home: &Path) -> Vec<String> {
    let mut versions: Vec<String> = std::fs::read_dir(versions_dir(home))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|version| is_installed(home, version))
        .collect();
    versions.sort_by(|a, b| crate::catalog::compare_versions(b, a));
    versions
}

/// A version that is safe as a folder name and in a URL.
pub fn safe_version(version: &str) -> bool {
    !version.is_empty()
        && !version.starts_with('.')
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}

/// What a download reports while it runs.
#[derive(Clone, Debug)]
pub struct Progress {
    pub line: String,
    /// Bytes so far and in all, once the size is known.
    pub bytes: Option<(u64, u64)>,
}

impl Progress {
    fn line(line: impl Into<String>) -> Self {
        Self {
            line: line.into(),
            bytes: None,
        }
    }
}

/// The SHA-256 `bin` in `folder` must have: `listed` from the signed catalog, else a `.sha256`
/// file that carries its own signature.
fn expected_sha(
    cdn: &Cdn,
    folder: &str,
    bin: &str,
    listed: Option<String>,
    progress: &dyn Fn(Progress),
) -> Result<String, String> {
    if let Some(hash) = listed {
        return Ok(hash);
    }
    let sha_url = format!("{folder}{bin}.sha256");
    progress(Progress::line(format!("GET {sha_url}")));
    let text = fetch_text(&sha_url)?;
    let signature = fetch_text(&format!("{sha_url}.minisig"))?;
    cdn.verify(text.as_bytes(), &signature)
        .map_err(|e| format!("{sha_url}: {e}"))?;
    parse_sha256_line(&text).ok_or_else(|| "sha256 file is not a hash".to_owned())
}

/// Fetches launcher `version` to `dest`, verified like a game client.
pub fn fetch_launcher(version: &str, dest: &Path, progress: &dyn Fn(Progress)) -> Result<(), String> {
    if !safe_version(version) {
        return Err(format!("bad version {version}"));
    }
    let cdn = current();
    let folder = launcher_url_for(&cdn, version);
    let listed = crate::catalog::launcher_sha(version, platform_folder());
    let expected = expected_sha(&cdn, &folder, launcher_bin(), listed, progress)?;
    let bin_url = format!("{folder}{}", launcher_bin());
    progress(Progress::line(format!("GET {bin_url}")));
    download_verified(&bin_url, dest, &expected, progress)
}

/// Fetches the game client of `version` into `versions/<version>/` and verifies its SHA-256:
/// the signed catalog's, else a `.sha256` file that carries its own signature.
pub fn fetch_game(
    home: &Path,
    version: &str,
    progress: &dyn Fn(Progress),
) -> Result<PathBuf, String> {
    if !safe_version(version) {
        return Err(format!("bad version {version}"));
    }
    let dest = game_path(home, version);
    let cdn = current();
    let folder = game_url_for(&cdn, version);
    let bin_url = format!("{}{}", folder, game_bin());
    let listed = crate::catalog::game_sha(version, platform_folder());
    let expected = expected_sha(&cdn, &folder, game_bin(), listed, progress)?;
    if dest.is_file()
        && let Some(have) = sha256_file(&dest)
        && have == expected
    {
        progress(Progress::line("already up to date"));
        return Ok(dest);
    }
    progress(Progress::line(format!("GET {bin_url}")));
    download_verified(&bin_url, &dest, &expected, progress)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&dest)
            .map_err(|e| format!("{}: {e}", dest.display()))?
            .permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(&dest, perm).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    let _ = std::fs::write(dest.with_extension("sha256"), &expected);
    progress(Progress::line(format!("saved {}", dest.display())));
    Ok(dest)
}

/// First token of a `sha256sum` line (`<hash>  <name>`).
pub fn parse_sha256_line(text: &str) -> Option<String> {
    let hash = text.split_whitespace().next()?;
    (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
}

pub fn sha256_file(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex(hasher.finalize().as_ref()))
}

pub fn fetch_text(url: &str) -> Result<String, String> {
    let mut response = ureq::get(url)
        .header("User-Agent", UA)
        .config()
        .timeout_global(Some(GET_TIMEOUT))
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("http {}", response.status()));
    }
    response
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

pub fn download_verified(
    url: &str,
    dest: &Path,
    expected: &str,
    progress: &dyn Fn(Progress),
) -> Result<(), String> {
    let response = ureq::get(url)
        .header("User-Agent", UA)
        .config()
        .timeout_global(Some(DOWNLOAD_TIMEOUT))
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("http {}", response.status()));
    }
    let total = response.body().content_length();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let part = dest.with_extension("part");
    let result = (|| {
        let mut reader = response.into_body().into_reader();
        let mut file =
            std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 64 * 1024];
        let mut done = 0u64;
        let mut last_report = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            file.write_all(&buf[..n])
                .map_err(|e| format!("{}: {e}", part.display()))?;
            done += n as u64;
            if done.saturating_sub(last_report) >= 262_144 {
                last_report = done;
                progress(Progress {
                    line: format_progress(done, total),
                    bytes: total.filter(|t| *t > 0).map(|t| (done, t)),
                });
            }
        }
        file.flush()
            .map_err(|e| format!("{}: {e}", part.display()))?;
        let have = hex(hasher.finalize().as_ref());
        if have != expected {
            return Err(format!("sha256 mismatch (got {have})"));
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(())
}

fn format_progress(done: u64, total: Option<u64>) -> String {
    let mb = |n: u64| n as f64 / 1_048_576.0;
    match total {
        Some(total) if total > 0 => format!("{:.1} / {:.1} MB", mb(done), mb(total)),
        _ => format!("{:.1} MB", mb(done)),
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A throwaway key pair, and `data` signed with it the way `minisign` does.
    pub(crate) struct TestKey {
        pair: minisign::KeyPair,
    }

    impl TestKey {
        pub(crate) fn new() -> Self {
            Self {
                pair: minisign::KeyPair::generate_unencrypted_keypair().unwrap(),
            }
        }

        pub(crate) fn cdn(&self) -> Cdn {
            Cdn {
                origin: "https://cdn.test".into(),
                key: self.pair.pk.to_base64(),
            }
        }

        pub(crate) fn sign(&self, data: &[u8]) -> String {
            minisign::sign(None, &self.pair.sk, data, None, None)
                .unwrap()
                .into_string()
        }
    }

    /// Made by `sign-cdn.py` (keygen, then `sign vector.txt`) with a throwaway key.
    const PY_KEY: &str = "untrusted comment: minisign public key E8F5E3E73FE7EE96\nRWSW7uc/5+P16FqowlsinQ4Bhm4r35JRvT8wuB2yBhm+LjpnftEX1rmH\n";
    const PY_SIGNATURE: &str = "untrusted comment: signature from the Melon CDN key\nRUSW7uc/5+P16Os4MgpmOi165BeeWXr151PGMzvmz0nyU1LYSQsOwar5053AeNNNbGEFAA5/2p/Ol7yg8LqXu4rqiFysz8KT1ws=\ntrusted comment: timestamp:1795465083\tfile:vector.txt\thashed\nUeUkk+Nsp2cyOJH4FuFFELjaOwKucWm9rT2gix4gpjfmcfpFz9hdTMlG9ZHs2ANAC/vAGxLXaONzuMrEXm4uDQ==\n";

    #[test]
    fn the_build_servers_signatures_verify() {
        let cdn = Cdn {
            origin: ORIGIN.into(),
            key: key_line(PY_KEY).unwrap().into(),
        };
        assert_eq!(cdn.verify(b"Melon CDN test vector\n", PY_SIGNATURE), Ok(()));
        assert!(cdn.verify(b"Melon CDN test vector!\n", PY_SIGNATURE).is_err());
    }

    #[test]
    fn the_official_key_verifies_the_build_servers_key() {
        let signature = "untrusted comment: signature from the Melon CDN key\nRUSCE+X5fx1YD4W+hKyo0CESUO6XrDaFn6zQRKbnJ/EwsdF0oTIKCSytLNkMmw/QeP7pU0CCzU7zbBSOVLPO+NrE5emnPMhvxwo=\ntrusted comment: timestamp:1795465221\tfile:tmp.FrDr40YlgW\thashed\neiR/T06N6LTEVA7wwZuaBxjyU4xFeqn9kR2Hb503ncUDHYMVCF3CKjYJARK3ylQ5/ohC4Hebsuuarq6evKh1Ag==\n";
        assert_eq!(key_line(OFFICIAL_KEY), Some(OFFICIAL_KEY));
        assert_eq!(Cdn::official().verify(b"real key check\n", signature), Ok(()));
    }

    #[test]
    fn a_signature_holds_only_for_its_data_and_its_key() {
        let key = TestKey::new();
        let signature = key.sign(b"{\"latest\":{}}");
        assert_eq!(key.cdn().verify(b"{\"latest\":{}}", &signature), Ok(()));
        assert!(key.cdn().verify(b"{\"latest\":{ }}", &signature).is_err());
        assert!(TestKey::new().cdn().verify(b"{\"latest\":{}}", &signature).is_err());
        assert!(key.cdn().verify(b"{\"latest\":{}}", "not a signature").is_err());
    }

    #[test]
    fn the_settings_pick_the_cdn_and_its_key() {
        assert_eq!(Cdn::from_settings(&Settings::default()), Cdn::official());
        let key = TestKey::new();
        let file = key.pair.pk.to_box().unwrap().into_string();
        let custom = Cdn::from_settings(&Settings {
            cdn_origin: Some(" https://mirror.test/cdn/ ".into()),
            cdn_public_key: Some(file),
            ..Settings::default()
        });
        assert_eq!(custom.origin, "https://mirror.test/cdn");
        assert_eq!(custom, Cdn { origin: custom.origin.clone(), ..key.cdn() });
        let bad_key = Cdn::from_settings(&Settings {
            cdn_public_key: Some("not a key".into()),
            ..Settings::default()
        });
        assert_eq!(bad_key.key, OFFICIAL_KEY, "an unreadable key falls back to the official one");
    }
}
