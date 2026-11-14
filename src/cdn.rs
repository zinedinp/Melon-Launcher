//! Where published Melon binaries live. The game client and the launcher

pub mod discover;

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// CDN origin, no trailing slash.
pub const ORIGIN: &str = "https://elbengeist.moe/melonmc/cdn";

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

/// This crate's version (`0.7.5+mc26.3`): the game client.
pub fn game_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Directory of a game version: `…/cdn/game/<version>/<os-arch>/`.
pub fn game_url_for(version: &str) -> String {
    format!("{ORIGIN}/game/{version}/{}/", platform_folder())
}

/// Where the launcher stores the downloaded game client (`<melon folder>/melon`).
pub fn game_dest(home: &Path) -> PathBuf {
    home.join(game_bin())
}

/// Fetch the published game client into `home`, verify its SHA-256, and
pub fn fetch_game(home: &Path, progress: &dyn Fn(&str)) -> Result<PathBuf, String> {
    let version = discover::latest_game();
    let dest = game_dest(home);
    let folder = game_url_for(&version);
    let sha_url = format!("{}{}.sha256", folder, game_bin());
    let bin_url = format!("{}{}", folder, game_bin());
    let expected = if let Some(hash) = discover::game_sha(&version, platform_folder()) {
        progress("discover sha256");
        hash
    } else {
        progress(&format!("GET {sha_url}"));
        fetch_text(&sha_url).and_then(|text| {
            parse_sha256_line(&text).ok_or_else(|| "sha256 file is not a hash".into())
        })?
    };
    if dest.is_file()
        && let Some(have) = sha256_file(&dest)
        && have == expected
    {
        progress("already up to date");
        return Ok(dest);
    }
    progress(&format!("GET {bin_url}"));
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
    progress(&format!("saved {}", dest.display()));
    Ok(dest)
}

/// First token of a `sha256sum` line (`<hash>  <name>`).
pub(super) fn parse_sha256_line(text: &str) -> Option<String> {
    let hash = text.split_whitespace().next()?;
    (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
}

pub(super) fn sha256_file(path: &Path) -> Option<String> {
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

pub(super) fn fetch_text(url: &str) -> Result<String, String> {
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

fn download_verified(
    url: &str,
    dest: &Path,
    expected: &str,
    progress: &dyn Fn(&str),
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
            if done.saturating_sub(last_report) >= 1_048_576 {
                last_report = done;
                progress(&format_progress(done, total));
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

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
