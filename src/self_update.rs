//! Puts a newer published launcher in place of the running one and starts it.

use crate::cdn;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A binary in a source checkout's `target` folder: cargo builds it, the CDN does not replace it.
fn built_from_source(exe: &Path) -> bool {
    exe.ancestors().any(|dir| {
        dir.file_name().is_some_and(|name| name == "target")
            && dir.parent().is_some_and(|checkout| checkout.join("Cargo.toml").is_file())
    })
}

/// The running launcher's binary, when an update may replace it.
pub fn binary() -> Option<PathBuf> {
    std::env::current_exe().ok().filter(|exe| !built_from_source(exe))
}

/// `exe` with `suffix` after its whole file name.
fn beside(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    exe.with_file_name(name)
}

/// Downloads launcher `version` and puts it in place of the running binary.
pub fn install(version: &str, progress: &dyn Fn(cdn::Progress)) -> Result<PathBuf, String> {
    let exe = binary().ok_or("this launcher is built from source: update it with git and cargo")?;
    let new = beside(&exe, ".new");
    cdn::fetch_launcher(version, &new, progress)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("{}: {e}", new.display()))?;
    }
    swap(&exe, &new)?;
    progress(cdn::Progress {
        line: format!("saved {}", exe.display()),
        bytes: None,
    });
    Ok(exe)
}

/// Moves `new` to `exe`. The old binary stays as `<name>.old` until the next update: Windows
/// does not let a running program's file be replaced, only renamed, and an antivirus may still
/// take the new one away.
fn swap(exe: &Path, new: &Path) -> Result<(), String> {
    let old = beside(exe, ".old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(exe, &old).map_err(|e| format!("{}: {e}", exe.display()))?;
    if let Err(e) = std::fs::rename(new, exe) {
        let _ = std::fs::rename(&old, exe);
        return Err(format!("{}: {e}", exe.display()));
    }
    Ok(())
}

/// Starts the launcher at `exe` with this run's arguments; one that does not start is swapped
/// back for the binary it replaced.
pub fn restart(exe: &Path) -> Result<(), String> {
    match Command::new(exe).args(std::env::args_os().skip(1)).spawn() {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = std::fs::rename(beside(exe, ".old"), exe);
            Err(format!("{}: {e}", exe.display()))
        }
    }
}

/// Removes a launcher download that an earlier run left unfinished.
pub fn clean_up() {
    if let Some(exe) = binary() {
        let _ = std::fs::remove_file(beside(&exe, ".new"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cargo_build_is_not_replaced() {
        let checkout = crate::store::scratch("self-update-source");
        std::fs::write(checkout.join("Cargo.toml"), "").unwrap();
        assert!(built_from_source(&checkout.join("target/debug/melon-launcher.exe")));
        assert!(!built_from_source(&checkout.join("bin/melon-launcher.exe")));
        let elsewhere = crate::store::scratch("self-update-installed");
        assert!(!built_from_source(&elsewhere.join("target/melon-launcher.exe")));
        let _ = std::fs::remove_dir_all(checkout);
        let _ = std::fs::remove_dir_all(elsewhere);
    }

    #[test]
    fn the_new_binary_takes_the_place_and_the_old_one_is_kept() {
        let dir = crate::store::scratch("self-update-swap");
        let exe = dir.join("melon-launcher.exe");
        std::fs::write(&exe, "old").unwrap();
        std::fs::write(beside(&exe, ".new"), "new").unwrap();
        swap(&exe, &beside(&exe, ".new")).unwrap();
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(dir.join("melon-launcher.exe.old")).unwrap(), "old");
        assert!(!beside(&exe, ".new").exists());
        assert!(swap(&exe, &dir.join("missing")).is_err());
        assert_eq!(std::fs::read_to_string(&exe).unwrap(), "new", "a failed swap leaves it be");
        let _ = std::fs::remove_dir_all(dir);
    }
}
