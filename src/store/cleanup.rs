//! Finds what nothing uses any more: old clients, old assets, caches.

use super::instances::{self, Instance, LOCAL_VERSION};
use super::settings::Settings;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A client in `versions/` no instance is set to.
    UnusedVersion,
    /// Extracted assets of a Minecraft version nothing plays.
    OldAssets,
    HeadsCache,
    /// Extracted and compiled mods; rebuilt on the next start.
    ModCache,
    /// Unpacked Java natives; unpacked again on the next start.
    Natives,
    /// A download that never finished.
    Partial,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub kind: Kind,
    /// The version or instance it belongs to.
    pub label: String,
    pub path: PathBuf,
    pub bytes: u64,
}

/// Bytes under `path`, links not followed.
fn size_of(path: &Path) -> u64 {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    if !meta.is_dir() || super::shared::is_link(path) {
        return 0;
    }
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| size_of(&entry.path()))
        .sum()
}

fn children(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect()
}

fn name_of(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Versions an instance needs: pinned ones, and the newest for those that follow the latest.
fn used_versions(home: &Path, instances: &[Instance], latest: Option<&str>) -> Vec<String> {
    let mut used: Vec<String> = instances
        .iter()
        .filter(|i| !i.is_java())
        .filter_map(|i| i.version.clone())
        .filter(|v| v != LOCAL_VERSION)
        .collect();
    if instances.iter().any(|i| !i.is_java() && i.version.is_none()) {
        used.extend(latest.map(String::from));
        used.extend(crate::cdn::installed_versions(home).into_iter().next());
    }
    used
}

/// Everything that can go, with its size. Reads a lot of files: run it off the UI thread.
pub fn scan(home: &Path, settings: &Settings, latest: Option<&str>) -> Vec<Item> {
    let instances = instances::list(home);
    let mut items = Vec::new();
    let mut add = |kind, label: String, path: PathBuf| {
        let bytes = size_of(&path);
        items.push(Item {
            kind,
            label,
            path,
            bytes,
        });
    };

    let used = used_versions(home, &instances, latest);
    for dir in children(&crate::cdn::versions_dir(home)) {
        let version = name_of(&dir);
        for file in children(&dir) {
            if file.extension().is_some_and(|e| e == "part") {
                add(Kind::Partial, version.clone(), file);
            }
        }
        if dir.is_dir() && !used.contains(&version) {
            add(Kind::UnusedVersion, version, dir);
        }
    }

    let mut played = vec![melon_base::home::VERSION.to_string()];
    played.extend(instances.iter().filter_map(|i| i.java_version.clone()));
    for dir in children(&settings.assets_dir(home)) {
        let version = name_of(&dir);
        let extracted = dir.join("minecraft").is_dir();
        if dir.is_dir() && extracted && !played.contains(&version) {
            add(Kind::OldAssets, version, dir);
        }
    }

    let heads = home.join("cache").join("heads");
    if heads.is_dir() {
        add(Kind::HeadsCache, String::new(), heads);
    }
    for instance in &instances {
        let cache = instance.path.join("mods").join(".cache");
        if cache.is_dir() && !instance.shared.mods {
            add(Kind::ModCache, instance.name.clone(), cache);
        }
        let natives = instance.path.join("natives");
        if natives.is_dir() {
            add(Kind::Natives, instance.name.clone(), natives);
        }
    }
    items.retain(|item| item.bytes > 0 || item.kind == Kind::UnusedVersion);
    items
}

/// Deletes one found item.
pub fn remove(item: &Item) -> std::io::Result<()> {
    if item.path.is_dir() {
        std::fs::remove_dir_all(&item.path)
    } else {
        std::fs::remove_file(&item.path)
    }
}

/// "12.3 MB"
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::scratch;
    use crate::store::instances::Engine;

    #[test]
    fn only_versions_no_instance_uses_are_offered() {
        let home = scratch("cleanup");
        instances::init(&home);
        let pinned = instances::create(&home, "Pinned", Engine::Melon).unwrap();
        instances::set_version(&home, &pinned.id, Some("0.7.5".into())).unwrap();
        for version in ["0.7.4", "0.7.5", "0.7.6"] {
            let path = crate::cdn::game_path(&home, version);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "bin").unwrap();
        }
        std::fs::write(crate::cdn::versions_dir(&home).join("0.7.6").join("melon.part"), "half")
            .unwrap();
        let found = scan(&home, &Settings::default(), Some("0.7.6"));
        let unused: Vec<_> = found
            .iter()
            .filter(|i| i.kind == Kind::UnusedVersion)
            .map(|i| i.label.as_str())
            .collect();
        assert_eq!(unused, ["0.7.4"]);
        assert!(found.iter().any(|i| i.kind == Kind::Partial));
        remove(found.iter().find(|i| i.kind == Kind::UnusedVersion).unwrap()).unwrap();
        assert!(!crate::cdn::is_installed(&home, "0.7.4"));
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn sizes_read_well() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }
}
