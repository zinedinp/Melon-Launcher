//! Java runtimes on this machine, for Java instances.
// TODO: download a matching Mojang Java runtime when none is installed.

use super::instances::Instance;
use super::settings::Settings;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    /// The `java` executable.
    pub path: PathBuf,
    /// `JAVA_VERSION` of the runtime's `release` file.
    pub version: Option<String>,
}

impl Runtime {
    /// "Java 21.0.3  C:\…\bin\java.exe"
    pub fn label(&self) -> String {
        match &self.version {
            Some(version) => format!("Java {version}  {}", self.path.display()),
            None => self.path.display().to_string(),
        }
    }
}

fn exe() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

fn subdirs(dir: PathBuf) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

/// Java homes in the places installers and other launchers put them.
fn candidate_homes() -> Vec<PathBuf> {
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let mut homes = Vec::new();
    homes.extend(env("JAVA_HOME"));
    let mut roots = Vec::new();
    if cfg!(windows) {
        for base in ["ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(|n| env(n)) {
            for vendor in ["Java", "Eclipse Adoptium", "Microsoft", "Zulu", "BellSoft", "Amazon Corretto"] {
                roots.push(base.join(vendor));
            }
        }
        if let Some(appdata) = env("APPDATA") {
            roots.push(appdata.join("PrismLauncher").join("java"));
            roots.extend(mojang_runtimes(appdata.join(".minecraft").join("runtime")));
        }
        if let Some(local) = env("LOCALAPPDATA") {
            roots.extend(mojang_runtimes(
                local
                    .join("Packages")
                    .join("Microsoft.4297127D64EC6_8wekyb3d8bbwe")
                    .join("LocalCache")
                    .join("Local")
                    .join("runtime"),
            ));
        }
    } else if cfg!(target_os = "macos") {
        for jvm in subdirs("/Library/Java/JavaVirtualMachines".into()) {
            homes.push(jvm.join("Contents").join("Home"));
        }
    } else {
        roots.push("/usr/lib/jvm".into());
        if let Some(home) = env("HOME") {
            roots.push(home.join(".local/share/PrismLauncher/java"));
            roots.extend(mojang_runtimes(home.join(".minecraft/runtime")));
        }
    }
    for root in roots {
        homes.extend(subdirs(root));
    }
    homes
}

/// Mojang's layout: `runtime/<component>/<platform>/<component>/bin/java`.
fn mojang_runtimes(runtime: PathBuf) -> Vec<PathBuf> {
    subdirs(runtime).into_iter().flat_map(subdirs).collect()
}

fn version_of(home: &Path) -> Option<String> {
    let text = std::fs::read_to_string(home.join("release")).ok()?;
    let line = text.lines().find_map(|l| l.strip_prefix("JAVA_VERSION="))?;
    Some(line.trim().trim_matches('"').to_string())
}

/// Every Java runtime found, each once.
pub fn detect() -> Vec<Runtime> {
    let mut found: Vec<Runtime> = Vec::new();
    let mut add = |bin: PathBuf| {
        if !bin.is_file() {
            return;
        }
        let bin = std::fs::canonicalize(&bin).map(strip_verbatim).unwrap_or(bin);
        if found.iter().any(|r| r.path == bin) {
            return;
        }
        let version = bin.parent().and_then(Path::parent).and_then(version_of);
        found.push(Runtime { path: bin, version });
    };
    for home in candidate_homes() {
        add(home.join("bin").join(exe()));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            add(dir.join(exe()));
        }
    }
    found
}

/// `\\?\C:\…` as `C:\…`.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(plain) if !plain.starts_with("UNC") => PathBuf::from(plain),
        _ => path,
    }
}

/// `java` from `JAVA_HOME`, else from `PATH`.
fn default_java() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let bin = PathBuf::from(home).join("bin").join(exe());
        if bin.is_file() {
            return Some(bin);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe()))
        .find(|p| p.is_file())
}

/// The runtime an instance starts with: its own, the launcher's, then the system's.
pub fn resolve(settings: &Settings, instance: &Instance) -> Option<PathBuf> {
    [instance.java_path.as_deref(), settings.java_path.as_deref()]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .or_else(default_java)
}

/// JVM arguments for an instance: heap first, then the launcher's extra arguments.
pub fn jvm_args(settings: &Settings, instance: &Instance) -> Vec<String> {
    let memory = instance.memory_mb.unwrap_or_else(|| settings.memory_mb());
    let mut args = vec![format!("-Xmx{memory}M")];
    if let Some(extra) = &settings.jvm_args {
        args.extend(extra.split_whitespace().map(String::from));
    }
    args
}
