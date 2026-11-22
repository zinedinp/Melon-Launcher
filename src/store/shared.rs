//! Shared folders: `<melon folder>/shared/<name>/`, each laid out like an instance. An instance
//! uses one of them and links its folders to it or copies from it. `global` always exists.

use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

const ROOT: &str = "shared";
/// The shared folder every instance starts with; it cannot be renamed or deleted.
pub const GLOBAL: &str = "global";
/// Where the only shared folder lived before there could be several.
const LEGACY: &str = "global";
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

/// The folder all shared folders are in.
pub fn root(main: &Path) -> PathBuf {
    main.join(ROOT)
}

/// The shared folder `name`.
pub fn dir(main: &Path, name: &str) -> PathBuf {
    root(main).join(name)
}

/// The shared folders there are, `global` first, then by name.
pub fn list(main: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root(main))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name != GLOBAL && !name.starts_with('.'))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.insert(0, GLOBAL.to_owned());
    names
}

fn ensure_folders(set: &Path) {
    for kind in Kind::ALL {
        let _ = std::fs::create_dir_all(set.join(kind.folder()));
    }
}

/// Creates `shared/global/`, moving the old `global/` there. True when it was moved: links
/// into it then point nowhere and must be made again.
pub fn ensure(main: &Path) -> bool {
    let global = dir(main, GLOBAL);
    let legacy = main.join(LEGACY);
    let mut moved = false;
    if !global.exists() && legacy.is_dir() {
        let _ = std::fs::create_dir_all(root(main));
        match std::fs::rename(&legacy, &global) {
            Ok(()) => moved = true,
            Err(e) => eprintln!("{} not moved: {e}", legacy.display()),
        }
    }
    ensure_folders(&global);
    let options = global.join(OPTIONS);
    let old_options = main.join("instances").join("melon").join(OPTIONS);
    if !options.exists() && old_options.is_file() {
        let _ = std::fs::copy(old_options, options);
    }
    moved
}

fn check_new(main: &Path, name: &str) -> Result<()> {
    if !super::instances::valid_name(name) || name.starts_with('.') {
        return Err(ErrorKind::InvalidInput.into());
    }
    if list(main).iter().any(|n| n.eq_ignore_ascii_case(name)) {
        return Err(ErrorKind::AlreadyExists.into());
    }
    Ok(())
}

/// Makes an empty shared folder.
pub fn create(main: &Path, name: &str) -> Result<()> {
    let name = name.trim();
    check_new(main, name)?;
    let set = dir(main, name);
    std::fs::create_dir_all(&set)?;
    ensure_folders(&set);
    Ok(())
}

/// Renames the folder itself. Instances using it must be linked again.
pub fn rename(main: &Path, from: &str, to: &str) -> Result<()> {
    let to = to.trim();
    if from == GLOBAL {
        return Err(Error::new(ErrorKind::PermissionDenied, "global cannot be renamed"));
    }
    if from.eq_ignore_ascii_case(to) && from != to {
        // Only the case changes: Windows sees the same name, so go through a temporary one.
        let temp = dir(main, &format!(".{from}.rename"));
        std::fs::rename(dir(main, from), &temp)?;
        return std::fs::rename(temp, dir(main, to));
    }
    check_new(main, to)?;
    std::fs::rename(dir(main, from), dir(main, to))
}

/// Deletes the folder with everything in it. No instance may still use it.
pub fn remove(main: &Path, name: &str) -> Result<()> {
    if name == GLOBAL {
        return Err(Error::new(ErrorKind::PermissionDenied, "global cannot be deleted"));
    }
    std::fs::remove_dir_all(dir(main, name))
}

/// Whether `path` is a link (a junction on Windows) and not a folder of its own.
pub fn is_link(path: &Path) -> bool {
    #[cfg(windows)]
    if junction::exists(path).unwrap_or(false) {
        return true;
    }
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

fn make_link(target: &Path, link: &Path) -> Result<()> {
    #[cfg(windows)]
    return junction::create(target, link);
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link);
    #[cfg(not(any(windows, unix)))]
    Err(ErrorKind::Unsupported.into())
}

fn remove_link(link: &Path) -> Result<()> {
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

/// Makes the instance's folder a link into the shared folder `set`; its own content is set
/// aside. A link to another shared folder is replaced.
pub fn link(main: &Path, set: &str, instance: &Path, kind: Kind) -> Result<()> {
    let target = dir(main, set).join(kind.folder());
    std::fs::create_dir_all(&target)?;
    let path = instance.join(kind.folder());
    if is_link(&path) {
        remove_link(&path)?;
    } else if path.exists() {
        if std::fs::read_dir(&path)?.next().is_none() {
            std::fs::remove_dir(&path)?;
        } else {
            let own = own_path(&path);
            if own.exists() {
                return Err(ErrorKind::AlreadyExists.into());
            }
            std::fs::rename(&path, own)?;
        }
    }
    make_link(&target, &path)
}

/// Removes the link, if there is one; the shared folder stays.
pub fn drop_link(instance: &Path, kind: Kind) -> Result<()> {
    let path = instance.join(kind.folder());
    if is_link(&path) {
        remove_link(&path)?;
    }
    Ok(())
}

/// Gives the instance its own folder back.
pub fn unlink(instance: &Path, kind: Kind) -> Result<()> {
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

/// Copies the shared folder's content into the instance's own folder, over files of the same
/// name.
pub fn copy_into(main: &Path, set: &str, instance: &Path, kind: Kind) -> Result<()> {
    let path = instance.join(kind.folder());
    if is_link(&path) {
        return Ok(());
    }
    super::copy_dir(&dir(main, set).join(kind.folder()), &path)
}

/// Copies the shared `options.txt` into the instance.
pub fn copy_options_into(main: &Path, set: &str, instance: &Path) -> Result<()> {
    let src = dir(main, set).join(OPTIONS);
    if !src.is_file() {
        return Err(ErrorKind::NotFound.into());
    }
    std::fs::copy(src, instance.join(OPTIONS)).map(|_| ())
}

/// Before the game starts: an instance on shared options plays with the shared `options.txt`.
pub fn apply_options(main: &Path, instance: &super::instances::Instance) {
    if instance.use_global_options
        && let Err(e) = copy_options_into(main, &instance.shared_folder, &instance.path)
        && e.kind() != ErrorKind::NotFound
    {
        eprintln!("Shared options.txt not copied: {e}");
    }
}

/// After the game: its `options.txt` becomes the shared one.
pub fn collect_options(main: &Path, instance: &super::instances::Instance) {
    let src = instance.path.join(OPTIONS);
    if !instance.use_global_options || !src.is_file() {
        return;
    }
    let set = dir(main, &instance.shared_folder);
    let _ = std::fs::create_dir_all(&set);
    if let Err(e) = std::fs::copy(&src, set.join(OPTIONS)) {
        eprintln!("Shared options.txt not written back: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::scratch;

    #[test]
    fn copying_from_a_shared_folder_leaves_the_instance_on_its_own() {
        let main = scratch("shared-copy");
        ensure(&main);
        let instance = main.join("instances").join("a");
        std::fs::create_dir_all(instance.join("saves")).unwrap();
        let world = dir(&main, GLOBAL).join("saves").join("World");
        std::fs::create_dir_all(&world).unwrap();
        std::fs::write(world.join("level.dat"), "x").unwrap();
        copy_into(&main, GLOBAL, &instance, Kind::Saves).unwrap();
        assert!(instance.join("saves").join("World").join("level.dat").is_file());
        assert!(!is_link(&instance.join("saves")));
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn linking_twice_and_unlinking_an_unlinked_folder_do_nothing() {
        let main = scratch("shared-link");
        ensure(&main);
        let instance = main.join("instances").join("a");
        std::fs::create_dir_all(instance.join("mods")).unwrap();
        link(&main, GLOBAL, &instance, Kind::Mods).unwrap();
        link(&main, GLOBAL, &instance, Kind::Mods).unwrap();
        assert!(is_link(&instance.join("mods")));
        unlink(&instance, Kind::Mods).unwrap();
        unlink(&instance, Kind::Mods).unwrap();
        assert!(instance.join("mods").is_dir() && !is_link(&instance.join("mods")));
        assert!(dir(&main, GLOBAL).join("mods").is_dir());
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn the_old_global_folder_moves_and_global_stays_first() {
        let main = scratch("shared-legacy");
        std::fs::create_dir_all(main.join("global").join("mods")).unwrap();
        std::fs::write(main.join("global").join("mods").join("a.zip"), "x").unwrap();
        assert!(ensure(&main));
        assert!(!main.join("global").exists());
        assert!(dir(&main, GLOBAL).join("mods").join("a.zip").is_file());
        create(&main, "Modded").unwrap();
        create(&main, "alpha").unwrap();
        assert_eq!(list(&main), ["global", "alpha", "Modded"]);
        assert_eq!(create(&main, "modded").unwrap_err().kind(), ErrorKind::AlreadyExists);
        assert!(rename(&main, GLOBAL, "x").is_err() && remove(&main, GLOBAL).is_err());
        rename(&main, "Modded", "modded").unwrap();
        assert_eq!(list(&main), ["global", "alpha", "modded"]);
        let _ = std::fs::remove_dir_all(main);
    }
}
