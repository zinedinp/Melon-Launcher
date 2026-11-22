//! The global reference: `<melon folder>/global/`, laid out like an instance, which instances
//! link folders to or copy from.

use std::path::{Path, PathBuf};

const FOLDER: &str = "global";
/// Where an instance's own folder waits while the folder is linked.
pub const OWN_SUFFIX: &str = ".own";
pub const OPTIONS: &str = "options.txt";

/// A folder an instance can share.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Saves,
    ResourcePacks,
    Mods,
    Plugins,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Saves, Kind::ResourcePacks, Kind::Mods, Kind::Plugins];

    pub fn folder(self) -> &'static str {
        match self {
            Kind::Saves => "saves",
            Kind::ResourcePacks => "resourcepacks",
            Kind::Mods => "mods",
            Kind::Plugins => "plugins",
        }
    }
}

pub fn dir(main: &Path) -> PathBuf {
    main.join(FOLDER)
}

/// Creates the global folders; the global options start as a copy of the ones shared before.
pub fn ensure(main: &Path) {
    let global = dir(main);
    for kind in Kind::ALL {
        let _ = std::fs::create_dir_all(global.join(kind.folder()));
    }
    let options = global.join(OPTIONS);
    let legacy = main.join("instances").join("melon").join(OPTIONS);
    if !options.exists() && legacy.is_file() {
        let _ = std::fs::copy(legacy, options);
    }
}

/// Whether `path` is a link (a junction on Windows) and not a folder of its own.
pub fn is_link(path: &Path) -> bool {
    #[cfg(windows)]
    if junction::exists(path).unwrap_or(false) {
        return true;
    }
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

fn make_link(target: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    return junction::create(target, link);
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link);
    #[cfg(not(any(windows, unix)))]
    Err(std::io::ErrorKind::Unsupported.into())
}

fn remove_link(link: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    return std::fs::remove_dir(link);
    #[cfg(not(windows))]
    std::fs::remove_file(link)
}

fn own_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(OWN_SUFFIX);
    path.with_file_name(name)
}

/// Makes the instance's folder a link to the global one; its own content is set aside.
pub fn link(main: &Path, instance: &Path, kind: Kind) -> std::io::Result<()> {
    let target = dir(main).join(kind.folder());
    std::fs::create_dir_all(&target)?;
    let path = instance.join(kind.folder());
    if is_link(&path) {
        return Ok(());
    }
    if path.exists() {
        if std::fs::read_dir(&path)?.next().is_none() {
            std::fs::remove_dir(&path)?;
        } else {
            let own = own_path(&path);
            if own.exists() {
                return Err(std::io::ErrorKind::AlreadyExists.into());
            }
            std::fs::rename(&path, own)?;
        }
    }
    make_link(&target, &path)
}

/// Removes the link, if there is one; the global folder stays.
pub fn drop_link(instance: &Path, kind: Kind) -> std::io::Result<()> {
    let path = instance.join(kind.folder());
    if is_link(&path) {
        remove_link(&path)?;
    }
    Ok(())
}

/// Gives the instance its own folder back.
pub fn unlink(instance: &Path, kind: Kind) -> std::io::Result<()> {
    drop_link(instance, kind)?;
    let path = instance.join(kind.folder());
    let own = own_path(&path);
    if path.exists() {
        return Ok(());
    }
    if own.is_dir() {
        std::fs::rename(own, &path)
    } else {
        std::fs::create_dir_all(&path)
    }
}

/// Copies the global folder's content into the instance's own folder, over files of the same
/// name.
pub fn copy_into(main: &Path, instance: &Path, kind: Kind) -> std::io::Result<()> {
    let path = instance.join(kind.folder());
    if is_link(&path) {
        return Ok(());
    }
    super::copy_dir(&dir(main).join(kind.folder()), &path)
}

/// Copies the global `options.txt` into the instance.
pub fn copy_options_into(main: &Path, instance: &Path) -> std::io::Result<()> {
    let src = dir(main).join(OPTIONS);
    if !src.is_file() {
        return Err(std::io::ErrorKind::NotFound.into());
    }
    std::fs::copy(src, instance.join(OPTIONS)).map(|_| ())
}

/// Before the game starts: an instance on global options plays with the global `options.txt`.
pub fn apply_options(main: &Path, instance: &super::instances::Instance) {
    if instance.use_global_options
        && let Err(e) = copy_options_into(main, &instance.path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("Global options.txt not copied: {e}");
    }
}

/// After the game: its `options.txt` becomes the global one.
pub fn collect_options(main: &Path, instance: &super::instances::Instance) {
    let src = instance.path.join(OPTIONS);
    if !instance.use_global_options || !src.is_file() {
        return;
    }
    let _ = std::fs::create_dir_all(dir(main));
    if let Err(e) = std::fs::copy(&src, dir(main).join(OPTIONS)) {
        eprintln!("Global options.txt not written back: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::scratch;

    #[test]
    fn copying_from_global_leaves_the_instance_on_its_own() {
        let main = scratch("global-copy");
        ensure(&main);
        let instance = main.join("instances").join("a");
        std::fs::create_dir_all(instance.join("saves")).unwrap();
        std::fs::create_dir_all(dir(&main).join("saves").join("World")).unwrap();
        std::fs::write(dir(&main).join("saves").join("World").join("level.dat"), "x").unwrap();
        copy_into(&main, &instance, Kind::Saves).unwrap();
        assert!(instance.join("saves").join("World").join("level.dat").is_file());
        assert!(!is_link(&instance.join("saves")));
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn linking_twice_and_unlinking_an_unlinked_folder_do_nothing() {
        let main = scratch("global-link");
        ensure(&main);
        let instance = main.join("instances").join("a");
        std::fs::create_dir_all(instance.join("mods")).unwrap();
        link(&main, &instance, Kind::Mods).unwrap();
        link(&main, &instance, Kind::Mods).unwrap();
        assert!(is_link(&instance.join("mods")));
        unlink(&instance, Kind::Mods).unwrap();
        unlink(&instance, Kind::Mods).unwrap();
        assert!(instance.join("mods").is_dir() && !is_link(&instance.join("mods")));
        assert!(dir(&main).join("mods").is_dir());
        let _ = std::fs::remove_dir_all(main);
    }
}
