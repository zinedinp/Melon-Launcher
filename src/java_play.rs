//! Start a Java Edition instance from a client jar and its version JSON.

use melon::auth::accounts::Account;
use melon::home;
use melon::instances::Instance;
use melon::prepare;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Player {
    pub name: String,
    pub uuid: String,
    pub token: String,
}

impl Player {
    pub fn from_account(account: Option<&Account>) -> Self {
        match account {
            Some(Account::Microsoft(stored)) => Self {
                name: stored.username.clone(),
                uuid: stored.uuid.clone(),
                token: stored.access_token.clone(),
            },
            Some(Account::Offline { username }) => {
                let uuid = melon::auth::accounts::offline_uuid(username).to_string();
                Self {
                    name: username.clone(),
                    uuid,
                    token: "0".into(),
                }
            }
            None => Self {
                name: "Player".into(),
                uuid: "00000000-0000-0000-0000-000000000000".into(),
                token: "0".into(),
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

/// Whether this Java instance has a jar we can launch.
pub fn ready(home: &Path, instance: &Instance) -> bool {
    locate(home, instance).is_some() && find_java().is_some()
}

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
        if !json.is_file() {
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

pub fn find_java() -> Option<PathBuf> {
    let name = if cfg!(windows) { "java.exe" } else { "java" };
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let bin = PathBuf::from(home).join("bin").join(name);
        if bin.is_file() {
            return Some(bin);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// Copy or extract this version's assets into `assets_dir` before launch.
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
    log(format!(
        "Extracting {} into {}",
        found.jar.display(),
        dest.display()
    ));
    melon::prepare::extract_jar_assets(&found.jar, &dest.join("minecraft"), &|p| {
        log(p.message);
    })?;
    fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    fs::write(&marker, version).map_err(|e| format!("{}: {e}", marker.display()))?;
    Ok(())
}

/// The `java` command for this instance.
pub fn command(
    home: &Path,
    instance: &Instance,
    assets_dir: &Path,
    player: &Player,
    join: Option<&str>,
) -> Result<Command, String> {
    let ready =
        locate(home, instance).ok_or("No Java client jar / version JSON on this machine.")?;
    let java = find_java().ok_or("No java on PATH (or JAVA_HOME).")?;
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
        assets_dir.join(id)
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
        ("user_type", "msa".into()),
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
    let mut jvm = vec!["-Xmx2G".into(), "-Dlog4j2.formatMsgNoLookups=true".into()];
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
    jvm_list.extend(list(&arguments["jvm"], os));
    game.extend(list(&arguments["game"], os));
    if let Some(server) = join {
        if !game
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

fn allowed(rules: &Value, os: &str) -> bool {
    let Some(rules) = rules.as_array() else {
        return true;
    };
    let mut allow = false;
    for rule in rules {
        let system = &rule["os"];
        let matches = system["name"].as_str().is_none_or(|n| n == os);
        if matches {
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
        if !allowed(&library["rules"], os) {
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
        if !allowed(&library["rules"], os) {
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

fn list(args: &Value, os: &str) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args.as_array().into_iter().flatten() {
        match arg {
            Value::String(s) => out.push(s.clone()),
            Value::Object(o) if allowed(&o["rules"], os) => match &o["value"] {
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
