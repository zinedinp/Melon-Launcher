//! authlib-injector: the Java agent that points a Java Edition client at another sign-in
//! service, so it joins servers and loads skins through that service.

use crate::cdn;
use melon_base::auth::alt_sessions::{self, Provider};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The newest release, as authlib-injector's own download API names it.
const LATEST: &str = "https://authlib-injector.yushi.moe/artifact/latest.json";
const PREFIX: &str = "authlib-injector-";

#[derive(Deserialize)]
struct Artifact {
    version: String,
    download_url: String,
    checksums: Checksums,
}

#[derive(Deserialize)]
struct Checksums {
    sha256: String,
}

fn dir(home: &Path) -> PathBuf {
    home.join("cache").join("authlib-injector")
}

fn metadata_path(home: &Path, provider: &Provider) -> PathBuf {
    dir(home).join(format!("{}.json", provider.host()))
}

/// The newest authlib-injector, downloaded once and checked against its sha256. Offline, the
/// one downloaded before.
pub fn ensure(home: &Path, log: &dyn Fn(String)) -> Result<PathBuf, String> {
    let latest = cdn::fetch_text(LATEST)
        .and_then(|text| serde_json::from_str::<Artifact>(&text).map_err(|e| e.to_string()));
    let artifact = match latest {
        Ok(artifact) => artifact,
        Err(e) => {
            return installed(home).ok_or_else(|| format!("authlib-injector could not be downloaded: {e}"));
        }
    };
    let safe = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_');
    if !artifact.version.chars().all(safe) || !artifact.download_url.starts_with("https://") {
        return Err("authlib-injector's download information looks wrong".into());
    }
    let jar = dir(home).join(format!("{PREFIX}{}.jar", artifact.version));
    let expected = artifact.checksums.sha256.to_ascii_lowercase();
    if cdn::sha256_file(&jar).as_deref() == Some(expected.as_str()) {
        return Ok(jar);
    }
    log(format!("Downloading authlib-injector {}", artifact.version));
    cdn::download_verified(&artifact.download_url, &jar, &expected, &|progress| log(progress.line))?;
    Ok(jar)
}

/// The newest authlib-injector downloaded so far.
pub fn installed(home: &Path) -> Option<PathBuf> {
    std::fs::read_dir(dir(home))
        .ok()?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(PREFIX) && name.ends_with(".jar")
        })
        .max_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok())
        .map(|entry| entry.path())
}

/// Keeps the service's metadata, so the game need not fetch it at start.
pub fn save_metadata(home: &Path, provider: &Provider) -> Result<(), String> {
    let text = alt_sessions::fetch_metadata(&provider.root).map_err(|e| format!("{e:#}"))?;
    let path = metadata_path(home, provider);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The JVM arguments that load the agent for `provider`; `None` when it was never downloaded.
pub fn jvm_args(home: &Path, provider: &Provider) -> Option<Vec<String>> {
    let jar = installed(home)?;
    let mut args = vec![format!("-javaagent:{}={}", jar.display(), provider.root)];
    if let Ok(metadata) = std::fs::read(metadata_path(home, provider)) {
        let encoded = melon_base::util::base64::encode(&metadata);
        args.push(format!("-Dauthlibinjector.yggdrasil.prefetched={encoded}"));
    }
    Some(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_points_at_the_root_and_carries_the_metadata() {
        let home = crate::store::scratch("authlib");
        let provider = Provider {
            root: "https://littleskin.cn/api/yggdrasil".into(),
            name: "LittleSkin".into(),
            skin_domains: Vec::new(),
            register_url: None,
            name_login: true,
        };
        assert!(jvm_args(&home, &provider).is_none(), "nothing downloaded yet");
        std::fs::create_dir_all(dir(&home)).unwrap();
        std::fs::write(dir(&home).join("authlib-injector-1.2.5.jar"), "jar").unwrap();
        std::fs::write(metadata_path(&home, &provider), "{}").unwrap();
        let args = jvm_args(&home, &provider).unwrap();
        assert!(args[0].starts_with("-javaagent:") && args[0].ends_with("=https://littleskin.cn/api/yggdrasil"));
        assert_eq!(args[1], "-Dauthlibinjector.yggdrasil.prefetched=e30=");
        let _ = std::fs::remove_dir_all(home);
    }

    /// Needs the network: `cargo test -- --ignored the_newest_agent`.
    #[test]
    #[ignore]
    fn the_newest_agent_downloads_and_checks_out() {
        let home = crate::store::scratch("authlib-download");
        let jar = ensure(&home, &|line| eprintln!("{line}")).unwrap();
        assert!(jar.is_file() && std::fs::metadata(&jar).unwrap().len() > 100_000);
        assert_eq!(ensure(&home, &|_| panic!("downloaded twice")).unwrap(), jar);
        let _ = std::fs::remove_dir_all(home);
    }
}
