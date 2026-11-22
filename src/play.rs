//! What the Play button is: locked, download, update or start.

use crate::store::instances::{Instance, LOCAL_VERSION};

/// Why Play cannot be pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    NoInstance,
    /// A login, download or mod build is in flight.
    Busy,
    /// A game is already running.
    Running,
    AccountLocked,
    /// A Java instance without a client jar and version file on this machine.
    NoJar,
    NoJava,
    /// The instance is set to a local binary and there is none.
    NoBinary,
    /// No release list, and no client on disk.
    Offline,
}

impl Blocked {
    /// The text key that says why.
    pub fn key(self) -> &'static str {
        match self {
            Blocked::NoInstance => "play.blocked.no_instance",
            Blocked::Busy => "play.blocked.busy",
            Blocked::Running => "play.blocked.running",
            Blocked::AccountLocked => "play.blocked.account_locked",
            Blocked::NoJar => "play.blocked.no_jar",
            Blocked::NoJava => "play.blocked.no_java",
            Blocked::NoBinary => "play.blocked.no_binary",
            Blocked::Offline => "play.blocked.offline",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayState {
    Locked(Blocked),
    /// Assets or the client are missing: pressing fetches them.
    Download,
    /// The instance follows the latest version and a newer one is published.
    Update,
    Start,
}

/// What a download fetches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Install {
    /// Prepare the Minecraft assets.
    pub assets: bool,
    /// The game client version to fetch.
    pub version: Option<String>,
}

/// The client a Melon instance starts with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Client {
    /// A version in `versions/`.
    Version(String),
    /// The local binary from the settings, or one found nearby.
    Local,
}

/// What the Play button looks at.
pub struct Facts<'a> {
    pub instance: Option<&'a Instance>,
    pub busy: bool,
    pub running: bool,
    pub account_locked: bool,
    /// The Minecraft assets the Melon client reads are extracted.
    pub assets_ready: bool,
    pub latest: Option<&'a str>,
    /// Installed client versions, newest first.
    pub installed: &'a [String],
    pub local_binary: bool,
    /// A Java instance's client jar and version file are on this machine.
    pub java_jar: bool,
    pub java_runtime: bool,
}

/// The version an instance wants, when one is known.
fn wanted<'a>(instance: &'a Instance, latest: Option<&'a str>) -> Option<&'a str> {
    match instance.version.as_deref() {
        Some(LOCAL_VERSION) => None,
        Some(pinned) => Some(pinned),
        None => latest,
    }
}

/// The client that starts now, if one is on disk.
pub fn client(facts: &Facts) -> Option<Client> {
    let instance = facts.instance?;
    let has = |version: &str| facts.installed.iter().any(|v| v == version);
    match instance.version.as_deref() {
        Some(LOCAL_VERSION) => facts.local_binary.then_some(Client::Local),
        Some(pinned) => has(pinned).then(|| Client::Version(pinned.to_owned())),
        None => match facts.latest.filter(|latest| has(latest)) {
            Some(latest) => Some(Client::Version(latest.to_owned())),
            None => match facts.installed.first() {
                Some(newest) => Some(Client::Version(newest.clone())),
                None => facts.local_binary.then_some(Client::Local),
            },
        },
    }
}

/// What a press on Download or Update fetches.
pub fn install(facts: &Facts) -> Install {
    let Some(instance) = facts.instance.filter(|i| !i.is_java()) else {
        return Install::default();
    };
    let version = wanted(instance, facts.latest)
        .filter(|version| !facts.installed.iter().any(|v| v == version))
        .map(str::to_owned);
    Install {
        assets: !facts.assets_ready,
        version,
    }
}

pub fn play_state(facts: &Facts) -> PlayState {
    use PlayState::{Download, Locked, Start, Update};
    let Some(instance) = facts.instance else {
        return Locked(Blocked::NoInstance);
    };
    if facts.running {
        return Locked(Blocked::Running);
    }
    if facts.busy {
        return Locked(Blocked::Busy);
    }
    if facts.account_locked {
        return Locked(Blocked::AccountLocked);
    }
    if instance.is_java() {
        return if !facts.java_jar {
            Locked(Blocked::NoJar)
        } else if !facts.java_runtime {
            Locked(Blocked::NoJava)
        } else {
            Start
        };
    }
    let has_client = client(facts).is_some();
    let fetch = install(facts);
    match (&instance.version, has_client, fetch.version.is_some()) {
        (Some(local), false, _) if local == LOCAL_VERSION => Locked(Blocked::NoBinary),
        (_, false, false) if facts.latest.is_none() => Locked(Blocked::Offline),
        (_, false, _) => Download,
        (_, true, _) if fetch.assets => Download,
        (None, true, true) => Update,
        (_, true, _) => Start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::instances::Engine;

    fn instance(version: Option<&str>, engine: Engine) -> Instance {
        Instance {
            id: "a".into(),
            name: "A".into(),
            path: "a".into(),
            engine,
            java_version: None,
            mojang_java: false,
            group: None,
            played_secs: 0,
            last_played: None,
            use_global_options: false,
            version: version.map(str::to_owned),
            java_path: None,
            memory_mb: None,
            shared: Default::default(),
        }
    }

    fn facts<'a>(instance: &'a Instance, latest: Option<&'a str>, installed: &'a [String]) -> Facts<'a> {
        Facts {
            instance: Some(instance),
            busy: false,
            running: false,
            account_locked: false,
            assets_ready: true,
            latest,
            installed,
            local_binary: false,
            java_jar: false,
            java_runtime: false,
        }
    }

    #[test]
    fn the_button_follows_what_is_on_disk() {
        let latest = instance(None, Engine::Melon);
        let none: [String; 0] = [];
        let old = ["0.7.5".to_string()];
        let both = ["0.7.6".to_string(), "0.7.5".to_string()];

        assert_eq!(play_state(&facts(&latest, Some("0.7.6"), &none)), PlayState::Download);
        assert_eq!(play_state(&facts(&latest, Some("0.7.6"), &old)), PlayState::Update);
        assert_eq!(play_state(&facts(&latest, Some("0.7.6"), &both)), PlayState::Start);
        assert_eq!(
            play_state(&facts(&latest, None, &none)),
            PlayState::Locked(Blocked::Offline)
        );
        assert_eq!(play_state(&facts(&latest, None, &old)), PlayState::Start, "offline plays what is there");
        assert_eq!(
            client(&facts(&latest, Some("0.7.6"), &old)),
            Some(Client::Version("0.7.5".into()))
        );
    }

    #[test]
    fn a_pinned_version_never_asks_to_update() {
        let pinned = instance(Some("0.7.5"), Engine::Melon);
        let old = ["0.7.5".to_string()];
        let newer = ["0.7.6".to_string()];
        assert_eq!(play_state(&facts(&pinned, Some("0.7.6"), &old)), PlayState::Start);
        assert_eq!(play_state(&facts(&pinned, Some("0.7.6"), &newer)), PlayState::Download);
        assert_eq!(
            install(&facts(&pinned, Some("0.7.6"), &newer)).version.as_deref(),
            Some("0.7.5")
        );
    }

    #[test]
    fn missing_assets_download_first() {
        let latest = instance(None, Engine::Melon);
        let both = ["0.7.6".to_string()];
        let mut f = facts(&latest, Some("0.7.6"), &both);
        f.assets_ready = false;
        assert_eq!(play_state(&f), PlayState::Download);
        assert_eq!(install(&f), Install { assets: true, version: None });
    }

    #[test]
    fn locks_say_why() {
        let latest = instance(None, Engine::Melon);
        let both = ["0.7.6".to_string()];
        let mut f = facts(&latest, Some("0.7.6"), &both);
        f.account_locked = true;
        assert_eq!(play_state(&f), PlayState::Locked(Blocked::AccountLocked));
        f.running = true;
        assert_eq!(play_state(&f), PlayState::Locked(Blocked::Running));

        let local = instance(Some(LOCAL_VERSION), Engine::Melon);
        assert_eq!(
            play_state(&facts(&local, Some("0.7.6"), &both)),
            PlayState::Locked(Blocked::NoBinary)
        );

        let java = instance(None, Engine::Java);
        let mut f = facts(&java, None, &[]);
        assert_eq!(play_state(&f), PlayState::Locked(Blocked::NoJar));
        f.java_jar = true;
        assert_eq!(play_state(&f), PlayState::Locked(Blocked::NoJava));
        f.java_runtime = true;
        assert_eq!(play_state(&f), PlayState::Start);
    }
}
