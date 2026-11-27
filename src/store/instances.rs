//! Instances: game folders under `instances/`, none of them forced.
//!
//! `instances.json` keeps the fields the game client also reads and rewrites; what only the
//! launcher knows lives in `launcher_instances.json`, which the client never touches.

use super::shared::{self, Kind};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// What plays an instance. Spelled as the game client reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Melon,
    Java,
}

pub const MAX_NAME_LEN: usize = 48;
/// The id the client falls back to; the instance made on the first run uses it.
pub const MELON: &str = "melon";
/// Ids the client re-creates whenever a game starts.
const CLIENT_DEFAULTS: [&str; 2] = [MELON, "vanilla"];
/// [`Instance::version`] for a Melon client outside `versions/`.
pub const LOCAL_VERSION: &str = "local";
const FILE: &str = "instances.json";
const EXTRAS_FILE: &str = "launcher_instances.json";
const OWN: &str = "instances";
const ICON: &str = "icon.png";
/// The biggest side an instance icon is saved at.
pub const ICON_SIDE: u32 = 128;
const NOT_COPIED: &[&str] = &["logs", "crash-reports", ".cache", "natives"];
/// Folders every instance has.
const GAME_DIRS: [&str; 5] = ["mods", "plugins", "saves", "resourcepacks", "logs"];
/// What the main folder held for the game before instances had folders of their own.
const LEGACY_GAME_FILES: &[&str] = &[
    "saves",
    "mods",
    "plugins",
    "resourcepacks",
    "logs",
    "downloads",
    "screenshots",
    "options.txt",
    "servers.json",
    "last_server.txt",
    "hotbar.nbt",
    "debug_settings.json",
    "command_history.txt",
];

#[derive(Clone, Debug)]
pub struct Instance {
    /// Folder name.
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    pub engine: Engine,
    /// Minecraft version of a Java instance.
    pub java_version: Option<String>,
    /// Official Mojang files. False: extract assets from the jar.
    pub mojang_java: bool,
    pub group: Option<String>,
    pub played_secs: u64,
    pub last_played: Option<u64>,
    /// `options.txt` comes from the shared folder.
    pub use_global_options: bool,
    /// Melon client version; none follows the latest, [`LOCAL_VERSION`] a local binary.
    pub version: Option<String>,
    /// Java runtime of this instance; none for the launcher's.
    pub java_path: Option<String>,
    pub memory_mb: Option<u32>,
    /// Folders linked to the shared folder.
    pub shared: Shared,
    /// The shared folder its links and shared options come from.
    pub shared_folder: String,
    /// The exit code of its last game, when that crashed.
    pub crashed: Option<String>,
}

impl Instance {
    pub fn icon_path(&self) -> PathBuf {
        self.path.join(ICON)
    }

    pub fn is_java(&self) -> bool {
        self.engine == Engine::Java
    }
}

/// Which folders of an instance are links into its shared folder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shared {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub saves: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub resourcepacks: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mods: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plugins: bool,
}

impl Shared {
    pub fn get(&self, kind: Kind) -> bool {
        match kind {
            Kind::Saves => self.saves,
            Kind::ResourcePacks => self.resourcepacks,
            Kind::Mods => self.mods,
            Kind::Plugins => self.plugins,
        }
    }

    fn set(&mut self, kind: Kind, on: bool) {
        match kind {
            Kind::Saves => self.saves = on,
            Kind::ResourcePacks => self.resourcepacks = on,
            Kind::Mods => self.mods = on,
            Kind::Plugins => self.plugins = on,
        }
    }
}

/// The client's schema of one `instances.json` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    played_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_played: Option<u64>,
    #[serde(default)]
    engine: Engine,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    java_version: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    mojang_java: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    use_global_options: bool,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Store {
    #[serde(default)]
    instances: BTreeMap<String, Meta>,
}

impl Store {
    fn load(main: &Path) -> Store {
        melon_base::json_file::load(&main.join(FILE)).unwrap_or_default()
    }

    fn save(&self, main: &Path) {
        if let Err(e) = melon_base::json_file::save(&main.join(FILE), self) {
            eprintln!("{FILE} not saved: {e}");
        }
    }
}

/// What only the launcher knows of an instance.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Extra {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    java_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    memory_mb: Option<u32>,
    #[serde(default)]
    shared: Shared,
    /// None: [`shared::GLOBAL`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shared_folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    crashed: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Extras {
    #[serde(default)]
    instances: BTreeMap<String, Extra>,
    /// Client defaults the user does not have; the client's empty copies of them are removed.
    #[serde(default)]
    dismissed: Vec<String>,
}

impl Extras {
    fn load(main: &Path) -> Extras {
        melon_base::json_file::load(&main.join(EXTRAS_FILE)).unwrap_or_default()
    }

    fn save(&self, main: &Path) {
        if let Err(e) = melon_base::json_file::save(&main.join(EXTRAS_FILE), self) {
            eprintln!("{EXTRAS_FILE} not saved: {e}");
        }
    }
}

/// A display name: letters, digits, `_`, `-` and inner spaces.
pub fn valid_name(name: &str) -> bool {
    (1..=MAX_NAME_LEN).contains(&name.len())
        && name == name.trim()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b' '))
}

/// Folder name from a display name: lowercase, spaces to `-`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() { "instance".into() } else { out }
}

/// The folder of the instance `id`.
pub fn dir(main: &Path, id: &str) -> PathBuf {
    main.join(OWN).join(id)
}

/// Creates the folders every instance has, where they are missing.
pub fn ensure_dirs(game_dir: &Path) {
    for name in GAME_DIRS {
        let path = game_dir.join(name);
        if !path.exists()
            && let Err(e) = std::fs::create_dir_all(&path)
        {
            eprintln!("{} not created: {e}", path.display());
        }
    }
}

/// First run: creates "Melon latest" in a fresh folder, or adopts the instances already there.
pub fn init(main: &Path) {
    if main.join(EXTRAS_FILE).exists() {
        return;
    }
    let mut extras = Extras::default();
    if folders(main).is_empty() {
        let melon = dir(main, MELON);
        if std::fs::create_dir_all(&melon).is_ok() {
            migrate_legacy(main, &melon);
            ensure_dirs(&melon);
            let mut store = Store::load(main);
            store.instances.entry(MELON.into()).or_default().name = Some("Melon latest".into());
            store.save(main);
        }
        extras.dismissed.push("vanilla".into());
    }
    extras.save(main);
}

/// Moves the game's files out of the main folder, never over files already there.
fn migrate_legacy(main: &Path, melon: &Path) {
    for name in LEGACY_GAME_FILES {
        let (from, to) = (main.join(name), melon.join(name));
        if from.exists()
            && !to.exists()
            && let Err(e) = std::fs::rename(&from, &to)
        {
            eprintln!("{} not moved: {e}", from.display());
        }
    }
}

/// Folder names under `instances/`.
fn folders(main: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(main.join(OWN))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    names
}

/// A folder with nothing but empty folders in it: what the client makes for its defaults.
fn is_hollow(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|entries| {
        entries.flatten().all(|entry| {
            let path = entry.path();
            entry.file_type().is_ok_and(|t| t.is_dir()) && is_hollow(&path)
        })
    })
}

/// The instances there are, by group and name.
pub fn list(main: &Path) -> Vec<Instance> {
    let mut store = Store::load(main);
    let mut extras = Extras::load(main);
    let mut ids = folders(main);
    let mut extras_dirty = false;
    ids.retain(|id| {
        if !extras.dismissed.contains(id) {
            return true;
        }
        if is_hollow(&dir(main, id)) {
            let _ = std::fs::remove_dir_all(dir(main, id));
            false
        } else {
            extras.dismissed.retain(|d| d != id);
            extras_dirty = true;
            true
        }
    });
    let before = store.instances.len();
    store.instances.retain(|id, _| ids.contains(id));
    if store.instances.len() != before {
        store.save(main);
    }
    let known = extras.instances.len();
    extras.instances.retain(|id, _| ids.contains(id));
    if extras_dirty || extras.instances.len() != known {
        extras.save(main);
    }
    let mut out: Vec<Instance> = ids
        .iter()
        .map(|id| from_meta(id, &dir(main, id), &store, &extras))
        .collect();
    out.sort_by_key(|i| (i.group.clone().unwrap_or_default().to_lowercase(), i.name.to_lowercase()));
    out
}

fn from_meta(id: &str, path: &Path, store: &Store, extras: &Extras) -> Instance {
    let meta = store.instances.get(id).cloned().unwrap_or_default();
    let extra = extras.instances.get(id).cloned().unwrap_or_default();
    let java_version = match meta.engine {
        Engine::Java => meta.java_version.or_else(|| Some(melon_base::home::VERSION.into())),
        Engine::Melon => meta.java_version,
    };
    Instance {
        id: id.to_string(),
        name: meta.name.unwrap_or_else(|| id.to_string()),
        path: path.to_path_buf(),
        engine: meta.engine,
        java_version,
        mojang_java: meta.mojang_java,
        group: meta.group.filter(|g| !g.trim().is_empty()),
        played_secs: meta.played_secs,
        last_played: meta.last_played,
        use_global_options: meta.use_global_options,
        version: extra.version,
        java_path: extra.java_path,
        memory_mb: extra.memory_mb,
        shared: extra.shared,
        shared_folder: extra.shared_folder.unwrap_or_else(|| shared::GLOBAL.to_owned()),
        crashed: extra.crashed,
    }
}

/// The instance an id or display name (any case) names.
pub fn find(main: &Path, query: &str) -> Option<Instance> {
    let q = query.trim();
    let all = list(main);
    all.iter()
        .find(|i| i.id == q)
        .or_else(|| all.iter().find(|i| i.name.eq_ignore_ascii_case(q)))
        .cloned()
}

/// Makes a new, empty instance named `name` at `instances/<slug>/`.
pub fn create(main: &Path, name: &str, engine: Engine) -> std::io::Result<Instance> {
    let instance = create_from(main, name, None)?;
    if engine == Engine::Java {
        return set_engine(main, &instance.id, engine);
    }
    Ok(instance)
}

/// A copy of `from`'s game folder; linked folders stay linked.
pub fn copy(main: &Path, from_id: &str, name: &str) -> std::io::Result<Instance> {
    let from = find(main, from_id).ok_or(std::io::ErrorKind::NotFound)?;
    create_from(main, name, Some(&from))
}

fn create_from(main: &Path, name: &str, from: Option<&Instance>) -> std::io::Result<Instance> {
    let name = name.trim();
    if !valid_name(name) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    if list(main).iter().any(|i| i.name.eq_ignore_ascii_case(name)) {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    std::fs::create_dir_all(main.join(OWN))?;
    let base = slug(name);
    let id = (1..)
        .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
        .find(|s| !dir(main, s).exists())
        .expect("slug");
    let path = dir(main, &id);
    let made = match from {
        Some(from) => copy_home(&from.path, &path),
        None => std::fs::create_dir(&path),
    };
    if let Err(e) = made {
        let _ = std::fs::remove_dir_all(&path);
        return Err(e);
    }
    let mut store = Store::load(main);
    let mut extras = Extras::load(main);
    let meta = store.instances.entry(id.clone()).or_default();
    meta.name = (name != id).then(|| name.to_string());
    if let Some(from) = from {
        meta.group = from.group.clone();
        meta.engine = from.engine;
        meta.java_version = from.java_version.clone();
        meta.mojang_java = from.mojang_java;
        meta.use_global_options = from.use_global_options;
        extras.instances.insert(
            id.clone(),
            Extra {
                version: from.version.clone(),
                java_path: from.java_path.clone(),
                memory_mb: from.memory_mb,
                shared: from.shared,
                shared_folder: (from.shared_folder != shared::GLOBAL).then(|| from.shared_folder.clone()),
                crashed: None,
            },
        );
        for kind in Kind::ALL {
            if from.shared.get(kind)
                && let Err(e) = shared::link(main, &from.shared_folder, &path, kind)
            {
                eprintln!("{} not linked: {e}", kind.folder());
            }
        }
    }
    extras.dismissed.retain(|d| d != &id);
    store.save(main);
    extras.save(main);
    ensure_dirs(&path);
    Ok(from_meta(&id, &path, &store, &extras))
}

/// Copies a game folder without logs, caches and links into shared folders.
fn copy_home(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let src = entry.path();
        if NOT_COPIED.iter().any(|n| name == *n) || shared::is_link(&src) {
            continue;
        }
        if name.to_string_lossy().ends_with(shared::OWN_SUFFIX) {
            continue;
        }
        let dest = to.join(&name);
        if src.is_dir() {
            super::copy_dir(&src, &dest)?;
        } else {
            std::fs::copy(&src, dest)?;
        }
    }
    Ok(())
}

fn edit_meta(main: &Path, id: &str, edit: impl FnOnce(&mut Meta)) -> std::io::Result<Instance> {
    let instance = find(main, id).ok_or(std::io::ErrorKind::NotFound)?;
    let mut store = Store::load(main);
    edit(store.instances.entry(instance.id.clone()).or_default());
    store.save(main);
    find(main, &instance.id).ok_or_else(|| std::io::ErrorKind::NotFound.into())
}

fn edit_extra(main: &Path, id: &str, edit: impl FnOnce(&mut Extra)) -> std::io::Result<Instance> {
    let instance = find(main, id).ok_or(std::io::ErrorKind::NotFound)?;
    let mut extras = Extras::load(main);
    edit(extras.instances.entry(instance.id.clone()).or_default());
    extras.save(main);
    find(main, &instance.id).ok_or_else(|| std::io::ErrorKind::NotFound.into())
}

/// Changes the display name. The folder stays.
pub fn rename(main: &Path, id: &str, name: &str) -> std::io::Result<Instance> {
    let name = name.trim();
    if !valid_name(name) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    if list(main)
        .iter()
        .any(|i| i.id != id && i.name.eq_ignore_ascii_case(name))
    {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    edit_meta(main, id, |meta| meta.name = (name != id).then(|| name.to_string()))
}

/// Moves the instance into a group; an empty name takes it out.
pub fn set_group(main: &Path, id: &str, group: &str) -> std::io::Result<Instance> {
    let group = group.trim();
    if !group.is_empty() && !valid_name(group) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    edit_meta(main, id, |meta| {
        meta.group = (!group.is_empty()).then(|| group.to_string());
    })
}

pub fn set_engine(main: &Path, id: &str, engine: Engine) -> std::io::Result<Instance> {
    edit_meta(main, id, |meta| {
        meta.engine = engine;
        if engine == Engine::Java && meta.java_version.is_none() {
            meta.java_version = Some(melon_base::home::VERSION.into());
        }
    })
}

pub fn set_use_global_options(main: &Path, id: &str, on: bool) -> std::io::Result<Instance> {
    edit_meta(main, id, |meta| meta.use_global_options = on)
}

/// The Melon client version: none for the latest, [`LOCAL_VERSION`] for a local binary.
pub fn set_version(main: &Path, id: &str, version: Option<String>) -> std::io::Result<Instance> {
    edit_extra(main, id, |extra| extra.version = version)
}

pub fn set_java_path(main: &Path, id: &str, path: Option<String>) -> std::io::Result<Instance> {
    edit_extra(main, id, |extra| extra.java_path = path)
}

pub fn set_memory(main: &Path, id: &str, memory_mb: Option<u32>) -> std::io::Result<Instance> {
    edit_extra(main, id, |extra| extra.memory_mb = memory_mb)
}

/// Links a folder of the instance into its shared folder, or gives it its own again.
pub fn set_shared(main: &Path, id: &str, kind: Kind, on: bool) -> std::io::Result<Instance> {
    let instance = find(main, id).ok_or(std::io::ErrorKind::NotFound)?;
    if on {
        shared::link(main, &instance.shared_folder, &instance.path, kind)?;
    } else {
        shared::unlink(&instance.path, kind)?;
    }
    edit_extra(main, id, |extra| extra.shared.set(kind, on))
}

/// Links the instance's linked folders into the shared folder `set`.
fn relink(main: &Path, instance: &Instance, set: &str) -> std::io::Result<()> {
    for kind in Kind::ALL {
        if instance.shared.get(kind) {
            shared::link(main, set, &instance.path, kind)?;
        }
    }
    Ok(())
}

/// Moves the instance to another shared folder; its linked folders follow.
pub fn set_shared_folder(main: &Path, id: &str, set: &str) -> std::io::Result<Instance> {
    let instance = find(main, id).ok_or(std::io::ErrorKind::NotFound)?;
    if !shared::list(main).iter().any(|name| name == set) {
        return Err(std::io::ErrorKind::NotFound.into());
    }
    relink(main, &instance, set)?;
    edit_extra(main, id, |extra| {
        extra.shared_folder = (set != shared::GLOBAL).then(|| set.to_owned());
    })
}

/// Renames a shared folder; the instances using it follow.
pub fn rename_shared_folder(main: &Path, from: &str, to: &str) -> std::io::Result<()> {
    shared::rename(main, from, to)?;
    let to = to.trim();
    let mut extras = Extras::load(main);
    for extra in extras.instances.values_mut() {
        if extra.shared_folder.as_deref() == Some(from) {
            extra.shared_folder = Some(to.to_owned());
        }
    }
    extras.save(main);
    for instance in list(main).iter().filter(|i| i.shared_folder == to) {
        relink(main, instance, to)?;
    }
    Ok(())
}

/// Links every instance again, after the shared folders moved.
pub fn relink_all(main: &Path) {
    for instance in list(main) {
        if let Err(e) = relink(main, &instance, &instance.shared_folder) {
            eprintln!("{} not linked again: {e}", instance.name);
        }
    }
}

/// Makes an image the instance's icon: its middle square, at most [`ICON_SIDE`] wide, as PNG.
pub fn set_icon(instance: &Instance, image: &Path) -> std::io::Result<()> {
    let bad = |e: image::ImageError| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string());
    let image = image::open(image).map_err(bad)?;
    let side = image.width().min(image.height());
    if side == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "the image is empty"));
    }
    let square = image.crop_imm((image.width() - side) / 2, (image.height() - side) / 2, side, side);
    let square = if side > ICON_SIDE {
        square.resize_exact(ICON_SIDE, ICON_SIDE, image::imageops::FilterType::Triangle)
    } else {
        square
    };
    let part = instance.path.join("icon.png.part");
    square
        .to_rgba8()
        .save_with_format(&part, image::ImageFormat::Png)
        .map_err(bad)?;
    std::fs::rename(&part, instance.icon_path())
}

/// Removes the instance's icon, so its initial stands in again.
pub fn clear_icon(instance: &Instance) -> std::io::Result<()> {
    match std::fs::remove_file(instance.icon_path()) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        done => done,
    }
}

/// How the instance's last game ended: the exit code when it crashed.
pub fn record_exit(main: &Path, id: &str, crashed: Option<String>) {
    if let Err(e) = edit_extra(main, id, |extra| extra.crashed = crashed) {
        eprintln!("{EXTRAS_FILE}: exit of {id} not saved: {e}");
    }
}

/// Deletes an instance with everything in it; its shared folder stays.
pub fn delete(main: &Path, id: &str) -> std::io::Result<()> {
    let instance = find(main, id).ok_or(std::io::ErrorKind::NotFound)?;
    for kind in Kind::ALL {
        shared::drop_link(&instance.path, kind)?;
    }
    std::fs::remove_dir_all(&instance.path)?;
    let mut store = Store::load(main);
    store.instances.remove(&instance.id);
    store.save(main);
    let mut extras = Extras::load(main);
    extras.instances.remove(&instance.id);
    if CLIENT_DEFAULTS.contains(&instance.id.as_str()) && !extras.dismissed.contains(&instance.id) {
        extras.dismissed.push(instance.id.clone());
    }
    extras.save(main);
    Ok(())
}

/// Seconds played in the instance, added up by the launcher.
pub fn record_play(main: &Path, id: &str, secs: u64) {
    let _ = edit_meta(main, id, |meta| {
        meta.played_secs = meta.played_secs.saturating_add(secs);
        meta.last_played = Some(super::now_secs());
    });
}

/// The server the game joined last in this folder.
pub fn last_server(game_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(game_dir.join("last_server.txt")).ok()?;
    let address = text.trim();
    (!address.is_empty()).then(|| address.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::scratch;

    #[test]
    fn names_cannot_leave_the_instances_folder() {
        assert!(valid_name("My World 2"));
        assert!(!valid_name("../x"));
        assert!(!valid_name(" lead"));
        assert!(!valid_name(""));
        assert_eq!(slug("My World 2"), "my-world-2");
    }

    #[test]
    fn the_first_run_makes_one_instance_and_it_can_be_deleted() {
        let main = scratch("first-run");
        init(&main);
        let all = list(&main);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, MELON);
        assert_eq!(all[0].name, "Melon latest");
        delete(&main, MELON).unwrap();
        assert!(list(&main).is_empty());
        init(&main);
        assert!(list(&main).is_empty(), "the first run happens once");
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn the_clients_empty_defaults_stay_hidden_and_used_ones_come_back() {
        let main = scratch("client-defaults");
        init(&main);
        delete(&main, MELON).unwrap();
        for id in CLIENT_DEFAULTS {
            ensure_dirs(&dir(&main, id));
        }
        assert!(list(&main).is_empty());
        assert!(!dir(&main, "vanilla").exists());
        ensure_dirs(&dir(&main, MELON));
        std::fs::write(dir(&main, MELON).join("options.txt"), "lang:de_de").unwrap();
        assert_eq!(list(&main).len(), 1);
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn existing_instances_are_adopted() {
        let main = scratch("adopt");
        ensure_dirs(&dir(&main, "vanilla"));
        ensure_dirs(&dir(&main, "old"));
        init(&main);
        let ids: Vec<_> = list(&main).into_iter().map(|i| i.id).collect();
        assert_eq!(ids, ["old", "vanilla"]);
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn an_icon_is_cut_square_and_shrunk_and_can_be_reset() {
        let main = scratch("icon");
        init(&main);
        let a = create(&main, "Pictured", Engine::Melon).unwrap();
        let wide = main.join("wide.png");
        image::RgbaImage::from_fn(400, 200, |x, _| image::Rgba([(x / 2) as u8, 0, 0, 255]))
            .save(&wide)
            .unwrap();
        set_icon(&a, &wide).unwrap();
        let icon = image::open(a.icon_path()).unwrap();
        assert_eq!((icon.width(), icon.height()), (ICON_SIDE, ICON_SIDE));
        assert!(!a.path.join("icon.png.part").exists());

        let fake = main.join("fake.png");
        std::fs::write(&fake, b"not a picture").unwrap();
        assert!(set_icon(&a, &fake).is_err());
        assert!(image::open(a.icon_path()).is_ok(), "a bad file leaves the old icon");

        clear_icon(&a).unwrap();
        assert!(!a.icon_path().exists());
        clear_icon(&a).unwrap();
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn a_crash_is_remembered_until_a_clean_exit() {
        let main = scratch("crashed");
        init(&main);
        let a = create(&main, "Shaky", Engine::Melon).unwrap();
        record_exit(&main, &a.id, Some("0xC0000005".into()));
        assert_eq!(find(&main, &a.id).unwrap().crashed.as_deref(), Some("0xC0000005"));
        assert_eq!(copy(&main, &a.id, "Shaky Copy").unwrap().crashed, None);
        record_exit(&main, &a.id, None);
        assert_eq!(find(&main, &a.id).unwrap().crashed, None);
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn instances_are_created_grouped_copied_renamed_and_deleted() {
        let main = scratch("crud");
        init(&main);
        let a = create(&main, "Survival", Engine::Melon).unwrap();
        assert_eq!(a.id, "survival");
        assert!(a.path.join("saves").is_dir());
        assert!(create(&main, "survival", Engine::Melon).is_err());
        let java = create(&main, "Old Java", Engine::Java).unwrap();
        assert!(java.is_java());
        assert_eq!(java.java_version.as_deref(), Some(melon_base::home::VERSION));

        std::fs::write(a.path.join("options.txt"), "x").unwrap();
        set_group(&main, &a.id, "Friends").unwrap();
        set_version(&main, &a.id, Some("0.7.5+mc26.3".into())).unwrap();
        let b = copy(&main, &a.id, "Survival Copy").unwrap();
        assert!(b.path.join("options.txt").is_file());
        assert_eq!(b.group.as_deref(), Some("Friends"));
        assert_eq!(b.version.as_deref(), Some("0.7.5+mc26.3"));

        let renamed = rename(&main, &b.id, "Second").unwrap();
        assert_eq!((renamed.id.as_str(), renamed.name.as_str()), ("survival-copy", "Second"));
        assert!(rename(&main, &b.id, "Survival").is_err());

        record_play(&main, &a.id, 90);
        assert_eq!(find(&main, "survival").unwrap().played_secs, 90);

        delete(&main, &a.id).unwrap();
        assert!(find(&main, "survival").is_none());
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn instances_json_keeps_the_fields_the_client_reads() {
        let main = scratch("client-schema");
        init(&main);
        let a = create(&main, "Shared One", Engine::Java).unwrap();
        set_version(&main, &a.id, Some("0.7.5+mc26.3".into())).unwrap();
        let text = std::fs::read_to_string(main.join(FILE)).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let entry = &json["instances"]["shared-one"];
        assert_eq!(entry["name"], "Shared One");
        assert_eq!(entry["engine"], "java");
        assert_eq!(entry["java_version"], melon_base::home::VERSION);
        assert!(entry.get("version").is_none(), "launcher-only fields stay out of it");
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn a_linked_folder_survives_its_instance() {
        let main = scratch("linked");
        init(&main);
        let a = create(&main, "Linker", Engine::Melon).unwrap();
        std::fs::write(a.path.join("saves").join("mine.txt"), "own").unwrap();
        let a = set_shared(&main, &a.id, Kind::Saves, true).unwrap();
        assert!(a.shared.saves);
        let world = shared::dir(&main, shared::GLOBAL).join("saves").join("world.txt");
        std::fs::write(&world, "shared").unwrap();
        assert!(a.path.join("saves").join("world.txt").is_file());
        assert!(!a.path.join("saves").join("mine.txt").exists());

        let a = set_shared(&main, &a.id, Kind::Saves, false).unwrap();
        assert!(a.path.join("saves").join("mine.txt").is_file(), "own content is back");

        set_shared(&main, &a.id, Kind::Saves, true).unwrap();
        delete(&main, &a.id).unwrap();
        assert!(world.is_file(), "deleting an instance keeps the shared folder");
        let _ = std::fs::remove_dir_all(main);
    }

    #[test]
    fn links_follow_the_instance_to_another_shared_folder_and_through_a_rename() {
        let main = scratch("shared-move");
        init(&main);
        shared::ensure(&main);
        let a = create(&main, "Mover", Engine::Melon).unwrap();
        set_shared(&main, &a.id, Kind::Mods, true).unwrap();
        shared::create(&main, "Modded").unwrap();
        let a = set_shared_folder(&main, &a.id, "Modded").unwrap();
        assert_eq!(a.shared_folder, "Modded");
        std::fs::write(shared::dir(&main, "Modded").join("mods").join("m.zip"), "x").unwrap();
        assert!(a.path.join("mods").join("m.zip").is_file());

        rename_shared_folder(&main, "Modded", "Old versions").unwrap();
        let a = find(&main, &a.id).unwrap();
        assert_eq!(a.shared_folder, "Old versions");
        assert!(shared::is_link(&a.path.join("mods")));
        assert!(a.path.join("mods").join("m.zip").is_file(), "the link points at the new name");

        let a = set_shared_folder(&main, &a.id, shared::GLOBAL).unwrap();
        assert_eq!(a.shared_folder, shared::GLOBAL);
        assert!(!a.path.join("mods").join("m.zip").exists());
        let _ = std::fs::remove_dir_all(main);
    }
}
