//! The Rust toolchain the game builds source-code mods with: `rustc` and `cargo`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// rustup's site, with its installers.
pub const RUSTUP_URL: &str = "https://rustup.rs";
/// The install line rustup's site gives for Linux and macOS.
pub const RUSTUP_SH: &str = "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh";
/// rustup from Windows' package manager.
pub const RUSTUP_WINGET: &str = "winget install --id Rustlang.Rustup";
/// What sets up a toolchain once rustup is there.
pub const RUSTUP_DEFAULT: &str = "rustup default stable";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rust {
    /// `rustc --version`'s line, and the folder that holds both tools.
    Found { version: String, bin: PathBuf },
    /// The tools are there (rustup's proxies), but they run no toolchain.
    NoToolchain,
    Missing,
}

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// `dir` holds both `rustc` and `cargo`.
fn has_tools(dir: &Path) -> bool {
    dir.join(exe("rustc")).is_file() && dir.join(exe("cargo")).is_file()
}

/// rustup's `bin` folder: `CARGO_HOME`'s, else `.cargo` in the user's home.
fn rustup_bin() -> Option<PathBuf> {
    let cargo_home = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| {
        let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        std::env::var_os(home).map(|home| PathBuf::from(home).join(".cargo"))
    })?;
    Some(cargo_home.join("bin"))
}

fn path_dirs() -> Vec<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect()
}

/// Looks for the tools on `PATH`, then in rustup's folder, and asks `rustc` its version.
///
/// It runs `rustc`, so call it off the window's thread.
pub fn detect() -> Rust {
    let dirs: Vec<PathBuf> = path_dirs()
        .into_iter()
        .chain(rustup_bin())
        .filter(|dir| has_tools(dir))
        .collect();
    if dirs.is_empty() {
        return Rust::Missing;
    }
    dirs.into_iter()
        .find_map(|bin| version(&bin.join(exe("rustc"))).map(|version| Rust::Found { version, bin }))
        .unwrap_or(Rust::NoToolchain)
}

/// `rustc --version`, when it runs and says something.
fn version(rustc: &Path) -> Option<String> {
    let mut cmd = Command::new(rustc);
    cmd.arg("--version");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok().filter(|out| out.status.success())?;
    let line = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!line.is_empty()).then_some(line)
}

/// The game's `PATH`, with rustup's folder in front when only that folder has the tools.
///
/// rustup adds its folder to `PATH` for programs started after the install, so a launcher
/// that was open during it would otherwise start a game that cannot find `cargo`.
pub fn game_path() -> Option<OsString> {
    let dirs = path_dirs();
    if dirs.iter().any(|dir| has_tools(dir)) {
        return None;
    }
    let bin = rustup_bin().filter(|bin| has_tools(bin))?;
    std::env::join_paths(std::iter::once(bin).chain(dirs)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_needs_both_tools() {
        let dir = crate::store::scratch("toolchain");
        assert!(!has_tools(&dir));
        std::fs::write(dir.join(exe("rustc")), b"").unwrap();
        assert!(!has_tools(&dir), "rustc alone builds no crate");
        std::fs::write(dir.join(exe("cargo")), b"").unwrap();
        assert!(has_tools(&dir));
        let _ = std::fs::remove_dir_all(dir);
    }
}
