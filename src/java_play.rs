//! Start a Java Edition instance from a client jar and its version JSON.

use melon_base::auth::accounts::Account;
use melon_base::auth::alt_sessions::Provider;
use melon_base::home;
use crate::store::instances::Instance;
use crate::store::java;
use crate::store::settings::Settings;
use melon_base::prepare;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Player {
    pub name: String,
    pub uuid: String,
    pub token: String,
    /// `msa` for Microsoft, `mojang` for other services, as launchers fill `${user_type}`.
    pub user_type: &'static str,
    /// Another sign-in service: the game starts with authlib-injector pointed at it.
    pub provider: Option<Provider>,
}

impl Player {
    pub fn from_account(account: Option<&Account>) -> Self {
        match account {
            Some(Account::Microsoft(stored)) => Self {
                name: stored.username.clone(),
                uuid: stored.uuid.clone(),
                token: stored.access_token.clone(),
                user_type: "msa",
                provider: None,
            },
            Some(Account::Alternative(stored)) => Self {
                name: stored.username.clone(),
                uuid: stored.uuid.clone(),
                token: stored.access_token.clone(),
                user_type: "mojang",
                provider: Some(stored.provider.clone()),
            },
            Some(Account::Offline { username }) => {
                let uuid = melon_base::auth::accounts::offline_uuid(username).to_string();
                Self {
                    name: username.clone(),
                    uuid,
                    token: "0".into(),
                    user_type: "msa",
                    provider: None,
                }
            }
            None => Self {
                name: "Player".into(),
                uuid: "00000000-0000-0000-0000-000000000000".into(),
                token: "0".into(),
                user_type: "msa",
                provider: None,
            },
        }
    }
}

/// A jar plus version JSON the launcher can start.
pub struct Ready {
    pub jar: PathBuf,
    pub json: PathBuf,
    pub minecraft: PathBuf,
}

/// The version file is `version`'s own, or a loader profile on top of it.
fn is_version(json: &Path, version: &str) -> bool {
    read_json(json).is_ok_and(|file| file["id"] == version || file["inheritsFrom"] == version)
}

/// The instance's client jar with its version file, here or in another launcher's folder.
pub fn locate(home: &Path, instance: &Instance) -> Option<Ready> {
    let version = instance.java_version.as_deref().unwrap_or(home::VERSION);
    let cache = home::minecraft_cache(home);
    let mut candidates = vec![
        cache
            .join("versions")
            .join(version)
            .join(format!("{version}.jar")),
        instance.path.join(format!("{version}.jar")),
        instance.path.join("client.jar"),
    ];
    if let Some(found) = prepare::scan(home) {
        candidates.insert(0, found.jar);
    }
    for jar in candidates {
        if !jar.is_file() {
            continue;
        }
        let json = jar.with_extension("json");
        let json = if json.is_file() {
            json
        } else {
            jar.parent()?.join(format!("{version}.json"))
        };
        // Another launcher's newest client is no stand-in for the version the instance names.
        if !json.is_file() || !is_version(&json, version) {
            continue;
        }
        let minecraft = jar
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .filter(|p| p.join("libraries").is_dir())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| cache.clone());
        return Some(Ready {
            jar,
            json,
            minecraft,
        });
    }
    None
}

/// The Mojang `assets` folder beside the client, when it holds the version's index: the
/// game finds its sounds and languages there.
fn mojang_assets(ready: &Ready, version: &Value) -> Option<PathBuf> {
    let index = version["assetIndex"]["id"].as_str()?;
    let assets = ready.minecraft.join("assets");
    assets
        .join("indexes")
        .join(format!("{index}.json"))
        .is_file()
        .then_some(assets)
}

/// Extracts this version's assets from the jar into `assets_dir` before launch, unless the
/// client's own Mojang assets are there.
pub fn prepare_assets(
    home: &Path,
    instance: &Instance,
    assets_dir: &Path,
    log: &dyn Fn(String),
) -> Result<(), String> {
    let version = instance.java_version.as_deref().unwrap_or(home::VERSION);
    let dest = assets_dir.join(version);
    if instance.mojang_java {
        log(format!("Using Mojang assets for {version}."));
        return Ok(());
    }
    let marker = dest.join(".melon-extracted");
    if marker.is_file() {
        log(format!("Assets for {version} already extracted."));
        return Ok(());
    }
    let found = locate(home, instance).ok_or_else(|| {
        format!(
            "No Java {version} jar for this instance. Put {version}.jar in the instance folder."
        )
    })?;
    let file = resolve(read_json(&found.json)?, &found.minecraft)?;
    if let Some(assets) = mojang_assets(&found, &file) {
        log(format!("Using the Mojang assets in {}", assets.display()));
        return Ok(());
    }
    log(format!(
        "Extracting {} into {}",
        found.jar.display(),
        dest.display()
    ));
    melon_base::prepare::extract_jar_assets(&found.jar, &dest.join("minecraft"), &|p| {
        log(p.message);
    })?;
    fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    fs::write(&marker, version).map_err(|e| format!("{}: {e}", marker.display()))?;
    Ok(())
}

/// The `java` command for this instance.
pub fn command(
    home: &Path,
    settings: &Settings,
    instance: &Instance,
    assets_dir: &Path,
    player: &Player,
    join: Option<&str>,
) -> Result<Command, String> {
    let ready =
        locate(home, instance).ok_or("No Java client jar / version JSON on this machine.")?;
    let java = java::resolve(settings, instance)
        .ok_or("No Java runtime: set one in the settings, or install Java.")?;
    let version = read_json(&ready.json)?;
    let version = resolve(version, &ready.minecraft)?;
    let os = os_name();
    let natives = instance.path.join("natives");
    unpack_natives(&version, &ready.minecraft, os, &natives)?;
    let id = version["id"].as_str().unwrap_or(home::VERSION);
    let main = version["mainClass"]
        .as_str()
        .ok_or("The version file has no mainClass")?;
    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath = classpath(&version, &ready.minecraft, os)?;
    let assets = if instance.mojang_java {
        ready.minecraft.join("assets")
    } else {
        mojang_assets(&ready, &version).unwrap_or_else(|| assets_dir.join(id))
    };
    let path = |p: &Path| p.to_string_lossy().into_owned();
    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", player.name.clone()),
        ("auth_uuid", player.uuid.clone()),
        ("auth_access_token", player.token.clone()),
        (
            "auth_session",
            format!("token:{}:{}", player.token, player.uuid),
        ),
        ("auth_xuid", String::new()),
        ("clientid", String::new()),
        ("user_type", player.user_type.into()),
        ("user_properties", "{}".into()),
        ("version_name", id.to_string()),
        (
            "version_type",
            version["type"].as_str().unwrap_or("release").into(),
        ),
        ("game_directory", path(&instance.path)),
        ("assets_root", path(&assets)),
        ("game_assets", path(&assets)),
        (
            "assets_index_name",
            version["assetIndex"]["id"]
                .as_str()
                .unwrap_or("legacy")
                .into(),
        ),
        ("natives_directory", path(&natives)),
        (
            "library_directory",
            path(&ready.minecraft.join("libraries")),
        ),
        ("classpath_separator", separator.into()),
        (
            "classpath",
            classpath
                .iter()
                .map(|c| path(c))
                .collect::<Vec<_>>()
                .join(separator),
        ),
        ("launcher_name", "melon".into()),
        ("launcher_version", env!("CARGO_PKG_VERSION").into()),
        ("resolution_width", String::new()),
        ("resolution_height", String::new()),
        ("quickPlayMultiplayer", join.unwrap_or_default().to_string()),
    ]);
    let mut jvm = java::jvm_args(settings, instance);
    jvm.push("-Dlog4j2.formatMsgNoLookups=true".into());
    if let Some(provider) = &player.provider {
        let agent = crate::authlib::jvm_args(home, provider)
            .ok_or("authlib-injector is missing: it is downloaded when the game starts, check the connection")?;
        jvm.extend(agent);
    }
    let arguments = &version["arguments"];
    let mut jvm_list = Vec::new();
    let mut game = Vec::new();
    if let Some(line) = version["minecraftArguments"].as_str() {
        jvm_list.extend(
            [
                "-Djava.library.path=${natives_directory}",
                "-cp",
                "${classpath}",
            ]
            .map(String::from),
        );
        game.extend(line.split_whitespace().map(String::from));
    }
    let features = Features {
        quick_play_multiplayer: join.is_some(),
    };
    jvm_list.extend(list(&arguments["jvm"], os, features));
    game.extend(list(&arguments["game"], os, features));
    if let Some(server) = join
        && !game
            .iter()
            .any(|a| a == "--server" || a.contains("quickPlayMultiplayer"))
    {
        let (host, port) = host_and_port(server);
        game.extend([
            "--server".into(),
            host.into(),
            "--port".into(),
            port.to_string(),
        ]);
    }
    jvm.extend(jvm_list.iter().map(|a| fill(a, &vars)));
    if !jvm.iter().any(|a| a == "-cp" || a.starts_with("-cp")) {
        jvm.extend(["-cp".into(), fill("${classpath}", &vars)]);
    }
    let mut cmd = Command::new(java);
    cmd.current_dir(&instance.path)
        .args(&jvm)
        .arg(main)
        .args(game.iter().map(|a| fill(a, &vars)));
    Ok(cmd)
}

fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "osx"
    } else if cfg!(windows) {
        "windows"
    } else {
        "linux"
    }
}

fn host_and_port(address: &str) -> (&str, u16) {
    match address.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') || host.ends_with(']') => match port.parse() {
            Ok(port) => (host.trim_matches(['[', ']']), port),
            Err(_) => (address, 25565),
        },
        _ => (address, 25565),
    }
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn safe_rel(rel: &str) -> Result<&str, String> {
    if rel.is_empty() || rel.contains("..") || rel.starts_with('/') || rel.contains('\0') {
        return Err(format!("bad path {rel}"));
    }
    Ok(rel)
}

/// The launcher features a version file's rules ask about. Only joining a server is ever on:
/// no demo, no custom window size, no quick play other than multiplayer.
#[derive(Clone, Copy, Default)]
struct Features {
    quick_play_multiplayer: bool,
}

impl Features {
    fn has(self, name: &str) -> bool {
        name == "is_quick_play_multiplayer" && self.quick_play_multiplayer
    }
}

/// `os.arch` of a rule: Mojang's `x86` means a 32-bit x86 runtime.
fn arch_matches(arch: &str) -> bool {
    match arch {
        "x86" => cfg!(target_arch = "x86"),
        "x86_64" => cfg!(target_arch = "x86_64"),
        "arm64" | "aarch64" => cfg!(target_arch = "aarch64"),
        _ => false,
    }
}

/// A version file's `rules`: the last rule whose OS name, CPU and features all match decides;
/// none matching disallows. `os.version` patterns are taken as matching.
fn allowed(rules: &Value, os: &str, features: Features) -> bool {
    let Some(rules) = rules.as_array() else {
        return true;
    };
    let mut allow = false;
    for rule in rules {
        let system = &rule["os"];
        let os_matches = system["name"].as_str().is_none_or(|name| name == os)
            && system["arch"].as_str().is_none_or(arch_matches);
        let features_match = rule["features"]
            .as_object()
            .is_none_or(|wanted| wanted.iter().all(|(name, on)| on.as_bool() == Some(features.has(name))));
        if os_matches && features_match {
            allow = rule["action"] == "allow";
        }
    }
    allow
}

fn library_key(library: &Value) -> String {
    let mut parts = library["name"].as_str().unwrap_or_default().split(':');
    let (group, artifact) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    match parts.nth(1) {
        Some(classifier) => format!("{group}:{artifact}:{classifier}"),
        None => format!("{group}:{artifact}"),
    }
}

fn resolve(version: Value, minecraft: &Path) -> Result<Value, String> {
    resolve_from(version, minecraft, &mut Vec::new())
}

fn resolve_from(version: Value, minecraft: &Path, seen: &mut Vec<String>) -> Result<Value, String> {
    let Some(parent) = version["inheritsFrom"].as_str() else {
        return Ok(version);
    };
    let parent = parent.to_string();
    let id = version["id"].as_str().unwrap_or_default().to_string();
    seen.push(id.clone());
    if seen.iter().any(|s| s == &parent) {
        return Err(format!("{id}: inheritsFrom loops"));
    }
    let file = minecraft
        .join("versions")
        .join(safe_rel(&parent)?)
        .join(format!("{parent}.json"));
    let mut merged = resolve_from(read_json(&file)?, minecraft, seen)?;
    let jar = version["jar"]
        .as_str()
        .or(merged["jar"].as_str())
        .or(merged["id"].as_str())
        .unwrap_or(&parent)
        .to_string();
    let own: Vec<Value> = version["libraries"].as_array().cloned().unwrap_or_default();
    let named: std::collections::HashSet<String> = own.iter().map(library_key).collect();
    let mut libraries = own;
    libraries.extend(
        merged["libraries"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| !named.contains(&library_key(l)))
            .cloned(),
    );
    for side in ["game", "jvm"] {
        let mut list = merged["arguments"][side]
            .as_array()
            .cloned()
            .unwrap_or_default();
        list.extend(
            version["arguments"][side]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        );
        if !list.is_empty() {
            merged["arguments"][side] = Value::Array(list);
        }
    }
    for (key, value) in version.as_object().into_iter().flatten() {
        if !matches!(key.as_str(), "libraries" | "arguments" | "inheritsFrom") {
            merged[key] = value.clone();
        }
    }
    merged["libraries"] = Value::Array(libraries);
    merged["jar"] = jar.into();
    Ok(merged)
}

fn classpath(version: &Value, minecraft: &Path, os: &str) -> Result<Vec<PathBuf>, String> {
    let id = version["id"].as_str().ok_or("The version file has no id")?;
    let jar = version["jar"].as_str().unwrap_or(id);
    let mut paths: Vec<PathBuf> = Vec::new();
    for library in version["libraries"].as_array().into_iter().flatten() {
        if !allowed(&library["rules"], os, Features::default()) {
            continue;
        }
        if let Some(rel) = library["downloads"]["artifact"]["path"].as_str() {
            let path = minecraft.join("libraries").join(safe_rel(rel)?);
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths.push(
        minecraft
            .join("versions")
            .join(safe_rel(jar)?)
            .join(format!("{jar}.jar")),
    );
    Ok(paths)
}

fn unpack_natives(version: &Value, minecraft: &Path, os: &str, to: &Path) -> Result<(), String> {
    let mut any = false;
    for library in version["libraries"].as_array().into_iter().flatten() {
        if !allowed(&library["rules"], os, Features::default()) {
            continue;
        }
        let Some(natives) = library["natives"][os].as_str() else {
            continue;
        };
        let key = natives.replace(
            "${arch}",
            if cfg!(target_pointer_width = "64") {
                "64"
            } else {
                "32"
            },
        );
        let Some(rel) = library["downloads"]["classifiers"][&key]["path"].as_str() else {
            continue;
        };
        let jar = minecraft.join("libraries").join(safe_rel(rel)?);
        if !jar.is_file() {
            continue;
        }
        any = true;
        fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        let file = fs::File::open(&jar).map_err(|e| format!("{}: {e}", jar.display()))?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", jar.display()))?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().map_err(|e| e.to_string())?.replace('\\', "/");
            if name.ends_with('/') || name.contains("..") || name.starts_with("META-INF/") {
                continue;
            }
            let out = to.join(&name);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut dest = fs::File::create(&out).map_err(|e| format!("{}: {e}", out.display()))?;
            std::io::copy(&mut entry, &mut dest).map_err(|e| e.to_string())?;
        }
    }
    if !any {
        let _ = fs::create_dir_all(to);
    }
    Ok(())
}

fn list(args: &Value, os: &str, features: Features) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args.as_array().into_iter().flatten() {
        match arg {
            Value::String(s) => out.push(s.clone()),
            Value::Object(o) if allowed(&o["rules"], os, features) => match &o["value"] {
                Value::String(s) => out.push(s.clone()),
                Value::Array(values) => {
                    out.extend(values.iter().filter_map(Value::as_str).map(String::from))
                }
                _ => {}
            },
            _ => {}
        }
    }
    out
}

fn fill(arg: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::new();
    let mut rest = arg;
    while let Some(start) = rest.find("${") {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        out.push_str(&rest[..start]);
        let key = &rest[start + 2..start + len];
        match vars.get(key) {
            Some(value) => out.push_str(value),
            None => out.push_str(&rest[start..start + len + 1]),
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule-bound arguments of a current version file (26.2).
    fn arguments() -> Value {
        serde_json::json!({
            "game": [
                "--username", "${auth_player_name}",
                {"rules": [{"action": "allow", "features": {"is_demo_user": true}}], "value": "--demo"},
                {"rules": [{"action": "allow", "features": {"has_custom_resolution": true}}],
                 "value": ["--width", "${resolution_width}", "--height", "${resolution_height}"]},
                {"rules": [{"action": "allow", "features": {"has_quick_plays_support": true}}],
                 "value": ["--quickPlayPath", "${quickPlayPath}"]},
                {"rules": [{"action": "allow", "features": {"is_quick_play_singleplayer": true}}],
                 "value": ["--quickPlaySingleplayer", "${quickPlaySingleplayer}"]},
                {"rules": [{"action": "allow", "features": {"is_quick_play_multiplayer": true}}],
                 "value": ["--quickPlayMultiplayer", "${quickPlayMultiplayer}"]},
                {"rules": [{"action": "allow", "features": {"is_quick_play_realms": true}}],
                 "value": ["--quickPlayRealms", "${quickPlayRealms}"]}
            ],
            "jvm": [
                {"rules": [{"action": "allow", "os": {"name": "osx"}}], "value": ["-XstartOnFirstThread"]},
                {"rules": [{"action": "allow", "os": {"name": "windows"}}], "value": "-XX:HeapDumpPath=dump"},
                {"rules": [{"action": "allow", "os": {"arch": "x86"}}], "value": "-Xss1M"},
                "-cp", "${classpath}"
            ]
        })
    }

    #[test]
    fn feature_rules_stay_off_unless_joining_a_server() {
        let args = arguments();
        let off = Features::default();
        assert_eq!(list(&args["game"], "windows", off), ["--username", "${auth_player_name}"]);
        let joining = Features {
            quick_play_multiplayer: true,
        };
        assert_eq!(
            list(&args["game"], "windows", joining),
            ["--username", "${auth_player_name}", "--quickPlayMultiplayer", "${quickPlayMultiplayer}"]
        );
    }

    #[test]
    fn os_and_cpu_rules_pick_the_jvm_arguments() {
        let jvm = list(&arguments()["jvm"], "windows", Features::default());
        assert!(jvm.contains(&"-XX:HeapDumpPath=dump".to_owned()));
        assert!(!jvm.contains(&"-XstartOnFirstThread".to_owned()));
        assert_eq!(jvm.contains(&"-Xss1M".to_owned()), cfg!(target_arch = "x86"));
        let osx = list(&arguments()["jvm"], "osx", Features::default());
        assert!(osx.contains(&"-XstartOnFirstThread".to_owned()));
    }

    #[test]
    fn only_the_named_version_or_a_profile_on_it_is_taken() {
        let dir = crate::store::scratch("java-version");
        let write = |name: &str, json: Value| {
            let path = dir.join(name);
            std::fs::write(&path, json.to_string()).unwrap();
            path
        };
        assert!(is_version(&write("a.json", serde_json::json!({"id": "26.2"})), "26.2"));
        assert!(!is_version(&write("b.json", serde_json::json!({"id": "26.3"})), "26.2"));
        let profile = serde_json::json!({"id": "fabric-loader-0.17-26.2", "inheritsFrom": "26.2"});
        assert!(is_version(&write("c.json", profile), "26.2"));
        assert!(!is_version(&dir.join("missing.json"), "26.2"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn mojang_assets_are_used_when_they_hold_the_index() {
        let minecraft = crate::store::scratch("java-assets");
        let ready = Ready {
            jar: minecraft.join("versions/26.3/26.3.jar"),
            json: minecraft.join("versions/26.3/26.3.json"),
            minecraft: minecraft.clone(),
        };
        let version = serde_json::json!({"assetIndex": {"id": "34"}});
        assert_eq!(mojang_assets(&ready, &version), None);
        std::fs::create_dir_all(minecraft.join("assets/indexes")).unwrap();
        std::fs::write(minecraft.join("assets/indexes/34.json"), "{}").unwrap();
        assert_eq!(mojang_assets(&ready, &version), Some(minecraft.join("assets")));
        let _ = std::fs::remove_dir_all(minecraft);
    }

    #[test]
    fn a_disallow_rule_after_an_allow_wins() {
        let rules = serde_json::json!([
            {"action": "allow"},
            {"action": "disallow", "os": {"name": "osx"}}
        ]);
        assert!(allowed(&rules, "windows", Features::default()));
        assert!(!allowed(&rules, "osx", Features::default()));
        assert!(allowed(&Value::Null, "linux", Features::default()));
    }
}
