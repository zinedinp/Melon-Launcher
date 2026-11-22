//! The launcher's window and loop: draws the UI, runs downloads in the background, starts the
//! game and watches it.

use crate::catalog;
use crate::cdn;
use crate::java_play;
use crate::lang::Strings;
use crate::play::{self, Client, Facts, Install};
use crate::store::instances::{self, Instance};
use crate::store::settings::Settings;
use crate::store::shared;
use crate::ui::heads::Heads;
use crate::ui::kit::paint::{Painter, Textures};
use crate::ui::kit::{CursorKind, Cx, Input, Key, Popup, theme};
use crate::ui::login::LoginFlow;
use crate::ui::overlay::{self, Choice, CrashReport, Progress};
use crate::ui::shell::{Action, PlayRequest, Shell, Status};
use melon_base::auth;
use melon_base::gfx::quad::DrawList;
use melon_base::gfx::{Renderer2d, ascii8};
use melon_base::home;
use melon_base::prepare::{self, Found};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

/// The window, in logical pixels.
const WINDOW_SIZE: (f64, f64) = (1280.0, 720.0);
const MIN_WINDOW_SIZE: (f64, f64) = (1040.0, 600.0);
/// GUI pixels per logical pixel.
const GUI_SCALE: f64 = 2.0;
const FRAME_TIME: Duration = Duration::from_nanos(1_000_000_000 / 60);
/// How often to check whether the game process has exited.
const GAME_POLL: Duration = Duration::from_millis(100);
/// One wheel notch in pixels, for touchpads that report pixels.
const PIXELS_PER_NOTCH: f64 = 20.0;
/// How many of the game's last lines are kept for a crash report.
const KEPT_LINES: usize = 64;
/// Two presses this close in time and place are a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

struct RunningState {
    renderer: Renderer2d,
    window: Window,
    textures: Textures,
    strings: Strings,
    shell: Shell,
    input: Input,
    popup: Popup,
    login: LoginFlow,
    heads: Heads,
    /// Shown over everything after a game that ended with an error.
    crash: Option<CrashReport>,
    /// Shown while something is downloaded or prepared.
    progress: Option<Progress>,
    prepare_job: Option<PrepareJob>,
    install_job: Option<InstallJob>,
    cursor: PhysicalPosition<f64>,
    last_press: Option<(Instant, (f32, f32))>,
    modifiers: ModifiersState,
    draw_list: DrawList,
    /// When the next frame may start.
    next_frame: Instant,
}

/// A Java instance's assets being readied before it starts.
struct PrepareJob {
    request: PlayRequest,
    rx: Receiver<String>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

/// Assets and/or a client being fetched.
struct InstallJob {
    rx: Receiver<cdn::Progress>,
    thread: Option<JoinHandle<Result<(), String>>>,
}

impl RunningState {
    /// Physical pixels per GUI pixel.
    fn scale(&self) -> f32 {
        (GUI_SCALE * self.window.scale_factor()).round().max(1.0) as f32
    }

    /// The window's size in GUI pixels.
    fn size(&self) -> (f32, f32) {
        let size = self.window.inner_size();
        let scale = self.scale();
        (size.width as f32 / scale, size.height as f32 / scale)
    }

    /// The cursor in GUI pixels.
    fn pointer(&self) -> (f32, f32) {
        let scale = self.scale();
        (self.cursor.x as f32 / scale, self.cursor.y as f32 / scale)
    }

    /// A popup covers the window: nothing under it reacts.
    fn modal_open(&self) -> bool {
        self.crash.is_some() || self.progress.is_some() || self.login.is_modal_open()
    }

    fn busy(&self) -> bool {
        self.login.is_busy() || self.prepare_job.is_some() || self.install_job.is_some()
    }

    fn set_language(&mut self, home: &Path) {
        self.strings = Strings::new(self.shell.settings.language(home));
    }

    fn show_progress(&mut self, job: &str) {
        let key = |part: &str| format!("job.{job}.{part}");
        let tr = &self.strings;
        self.progress = Some(Progress::new(
            tr.get(&key("title")),
            tr.get(&key("working")),
            tr.get(&key("failed")),
        ));
    }

    /// Readies a Java instance's assets, then the game starts.
    fn start_prepare(&mut self, request: PlayRequest, instance: Instance, home: &Path) {
        if self.prepare_job.is_some() || self.install_job.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let assets = self.shell.settings.assets_dir(home);
        let home = home.to_path_buf();
        let thread = std::thread::spawn(move || {
            java_play::prepare_assets(&home, &instance, &assets, &|message| {
                let _ = tx.send(message);
            })
        });
        self.show_progress("prepare");
        self.prepare_job = Some(PrepareJob {
            request,
            rx,
            thread: Some(thread),
        });
    }

    /// Drains the job's lines. `Some` when it succeeded and the game may start.
    fn poll_prepare(&mut self) -> Option<PlayRequest> {
        let job = self.prepare_job.as_mut()?;
        while let Ok(line) = job.rx.try_recv() {
            if let Some(progress) = &mut self.progress {
                progress.push(&line);
            }
        }
        if !job.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            return None;
        }
        let mut job = self.prepare_job.take()?;
        let result = job
            .thread
            .take()
            .and_then(|t| t.join().ok())
            .unwrap_or_else(|| Err("the preparing thread failed".into()));
        match result {
            Ok(()) => {
                self.progress = None;
                Some(job.request)
            }
            Err(error) => {
                if let Some(progress) = &mut self.progress {
                    progress.fail(error);
                }
                None
            }
        }
    }

    /// Fetches the Minecraft assets and/or a game client in the background.
    fn start_install(&mut self, install: Install, found: Option<Found>, home: &Path) {
        if self.prepare_job.is_some() || self.install_job.is_some() {
            return;
        }
        if !install.assets && install.version.is_none() {
            return;
        }
        let home = home.to_path_buf();
        let extract = self.shell.settings.melon_extract(&home);
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            if install.assets {
                let log = |p: prepare::Progress| {
                    let _ = tx.send(cdn::Progress {
                        line: p.message,
                        bytes: None,
                    });
                };
                match found {
                    Some(found) if found.asset_index.is_some() => {
                        prepare::extract_into(found, &extract, &log)
                    }
                    Some(found) => prepare::from_path(&found.jar, &extract, &log),
                    None => prepare::from_local(&home, &extract, &log),
                }?;
            }
            if let Some(version) = &install.version {
                cdn::fetch_game(&home, version, &|progress| {
                    let _ = tx.send(progress);
                })?;
            }
            Ok(())
        });
        self.show_progress("install");
        self.install_job = Some(InstallJob {
            rx,
            thread: Some(thread),
        });
    }

    /// Drains the download's lines. `Some` when it ended; then what is on disk is looked at
    /// again.
    fn poll_install(&mut self) -> Option<()> {
        let job = self.install_job.as_mut()?;
        while let Ok(update) = job.rx.try_recv() {
            if let Some(progress) = &mut self.progress {
                match update.bytes {
                    Some(bytes) => progress.set_bytes(bytes),
                    None => progress.push(&update.line),
                }
            }
        }
        if !job.thread.as_ref().is_some_and(JoinHandle::is_finished) {
            return None;
        }
        let mut job = self.install_job.take()?;
        let result = job
            .thread
            .take()
            .and_then(|t| t.join().ok())
            .unwrap_or_else(|| Err("the download thread failed".into()));
        match result {
            Ok(()) => self.progress = None,
            Err(error) => {
                if let Some(progress) = &mut self.progress {
                    progress.fail(error);
                }
            }
        }
        self.shell.recheck();
        Some(())
    }

    /// A game ended while the window was open.
    fn game_ended(&mut self, finished: Finished) {
        self.shell
            .record_play(finished.account_key.as_deref(), &finished.instance, finished.secs);
        if let Some(report) = finished.crash {
            self.crash = Some(report);
        }
    }
}

/// A game that is running.
struct RunningGame {
    child: Child,
    started: Instant,
    account_key: Option<String>,
    instance: String,
    log: PathBuf,
    /// Its last lines, filled by the threads copying its output.
    tail: Arc<Mutex<VecDeque<String>>>,
    pumps: Vec<JoinHandle<()>>,
    /// The launcher ended it: its exit code is no crash.
    stopped: bool,
}

/// How a game ended.
struct Finished {
    account_key: Option<String>,
    instance: String,
    secs: u64,
    crash: Option<CrashReport>,
}

struct App {
    state: Option<RunningState>,
    home: PathBuf,
    validation: bool,
    client_id: Option<String>,
    game: Option<RunningGame>,
    /// A game that ended while the window was closed; the window shows it when it opens.
    finished: Option<Finished>,
    catalog_rx: Receiver<()>,
    /// The CDN was asked for the release list.
    catalog_checked: bool,
}

/// The Melon client an instance starts with.
fn melon_binary(home: &Path, settings: &Settings, instance: &Instance) -> Result<PathBuf, String> {
    let installed = cdn::installed_versions(home);
    let latest = catalog::latest_game();
    let local = settings.local_binary(home);
    let facts = Facts {
        instance: Some(instance),
        busy: false,
        running: false,
        account_locked: false,
        assets_ready: true,
        latest: latest.as_deref(),
        installed: &installed,
        local_binary: local.is_some(),
        java_jar: false,
        java_runtime: false,
    };
    match play::client(&facts) {
        Some(Client::Version(version)) => Ok(cdn::game_path(home, &version)),
        Some(Client::Local) => local.ok_or_else(|| "no local Melon client".into()),
        None => Err("no Melon client installed for this instance".into()),
    }
}

/// Starts the game. Its output is copied to the terminal, `logs/latest.log` and a tail.
fn spawn_game(request: &PlayRequest, validation: bool, home: &Path) -> Result<RunningGame, String> {
    let settings = Settings::load(home);
    let instance = instances::find(home, &request.instance)
        .ok_or_else(|| format!("no instance {}", request.instance))?;
    shared::apply_options(home, &instance);
    let mut cmd = if instance.is_java() {
        let accounts = auth::accounts::Accounts::load(home);
        let account = request
            .account_key
            .as_deref()
            .and_then(|key| accounts.list().iter().find(|a| a.key() == key));
        let player = java_play::Player::from_account(account);
        java_play::command(
            home,
            &settings,
            &instance,
            &settings.assets_dir(home),
            &player,
            request.join.as_deref(),
        )?
    } else {
        instances::ensure_dirs(&instance.path);
        let mut cmd = Command::new(melon_binary(home, &settings, &instance)?);
        cmd.arg("--game");
        cmd.args(["--instance", &instance.id]);
        if let Some(address) = &request.join {
            cmd.args(["--join", address]);
        }
        if validation || settings.validation {
            cmd.arg("--vulkan-validation");
        }
        if settings.probe {
            cmd.arg("--probe");
        }
        cmd
    };
    let io = |e: std::io::Error| e.to_string();
    let logs = instance.path.join("logs");
    std::fs::create_dir_all(&logs).map_err(io)?;
    let log = logs.join("latest.log");
    let file = Arc::new(Mutex::new(std::fs::File::create(&log).map_err(io)?));

    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(io)?;
    let tail = Arc::new(Mutex::new(VecDeque::new()));
    let mut pumps = Vec::new();
    if let Some(out) = child.stdout.take() {
        pumps.push(pump(out, false, &file, &tail));
    }
    if let Some(err) = child.stderr.take() {
        pumps.push(pump(err, true, &file, &tail));
    }
    Ok(RunningGame {
        child,
        started: Instant::now(),
        account_key: request.account_key.clone(),
        instance: instance.id,
        log,
        tail,
        pumps,
        stopped: false,
    })
}

/// Copies a pipe line by line to the terminal, the log file and the tail.
fn pump(
    pipe: impl std::io::Read + Send + 'static,
    to_stderr: bool,
    file: &Arc<Mutex<std::fs::File>>,
    tail: &Arc<Mutex<VecDeque<String>>>,
) -> JoinHandle<()> {
    let (file, tail) = (Arc::clone(file), Arc::clone(tail));
    std::thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut raw = Vec::new();
        while reader.read_until(b'\n', &mut raw).is_ok_and(|n| n > 0) {
            let line = String::from_utf8_lossy(&raw).trim_end().to_owned();
            raw.clear();
            if to_stderr {
                eprintln!("{line}");
            } else {
                println!("{line}");
            }
            if let Ok(mut file) = file.lock() {
                let _ = writeln!(file, "{line}");
            }
            if let Ok(mut tail) = tail.lock() {
                if tail.len() == KEPT_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        }
    })
}

/// The popup for a game that ended with `status`.
fn report(game: &RunningGame, status: std::process::ExitStatus) -> CrashReport {
    let lines = game
        .tail
        .lock()
        .map(|t| t.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    CrashReport {
        code: status
            .code()
            .map_or_else(|| "?".to_owned(), |c| c.to_string()),
        lines: lines[lines.len().saturating_sub(overlay::TAIL_LINES)..].to_vec(),
        log: game.log.clone(),
    }
}

/// The keys a text field takes for a key press.
fn keys_of(event: &KeyEvent, control: bool) -> Vec<Key> {
    let named = |key| vec![key];
    match &event.logical_key {
        WinitKey::Named(NamedKey::Backspace) => named(Key::Backspace),
        WinitKey::Named(NamedKey::Delete) => named(Key::Delete),
        WinitKey::Named(NamedKey::ArrowLeft) => named(Key::Left),
        WinitKey::Named(NamedKey::ArrowRight) => named(Key::Right),
        WinitKey::Named(NamedKey::Home) => named(Key::Home),
        WinitKey::Named(NamedKey::End) => named(Key::End),
        WinitKey::Named(NamedKey::Enter) => named(Key::Enter),
        WinitKey::Character(c) if control => match c.to_lowercase().as_str() {
            "a" => named(Key::SelectAll),
            "c" => named(Key::Copy),
            "x" => named(Key::Cut),
            "v" => named(Key::Paste(
                melon_base::platform::clipboard_text().unwrap_or_default(),
            )),
            _ => Vec::new(),
        },
        _ => event
            .text
            .as_ref()
            .map(|text| text.chars().filter(|c| !c.is_control()).map(Key::Char).collect())
            .unwrap_or_default(),
    }
}

impl App {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let window = melon_base::platform::create_centered_window(
            event_loop,
            Window::default_attributes()
                .with_title("Melon Launcher")
                .with_inner_size(LogicalSize::new(WINDOW_SIZE.0, WINDOW_SIZE.1))
                .with_min_inner_size(LogicalSize::new(MIN_WINDOW_SIZE.0, MIN_WINDOW_SIZE.1)),
        )?;
        window.set_ime_allowed(true);
        let mut renderer = Renderer2d::new(&window, self.validation)?;
        let textures = Textures {
            white: renderer.white_texture(),
            font: renderer.load_texture(&ascii8::atlas())?,
        };
        let home = self.home.as_path();
        let shell = Shell::new(home);

        let mut state = RunningState {
            renderer,
            window,
            textures,
            strings: Strings::new(shell.settings.language(home)),
            shell,
            input: Input::default(),
            popup: Popup::default(),
            login: LoginFlow::new(self.client_id.clone(), home.to_path_buf()),
            heads: Heads::new(home),
            crash: None,
            progress: None,
            prepare_job: None,
            install_job: None,
            cursor: PhysicalPosition::new(0.0, 0.0),
            last_press: None,
            modifiers: ModifiersState::empty(),
            draw_list: DrawList::new(),
            next_frame: Instant::now(),
        };
        if let Some(finished) = self.finished.take() {
            state.game_ended(finished);
        }
        self.state = Some(state);
        Ok(())
    }

    fn reopen(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(e) = self.init(event_loop) {
            eprintln!("The launcher window could not be opened: {e:#}");
            event_loop.exit();
        }
    }

    /// Looks whether the running game has ended, and takes in how.
    fn poll_game(&mut self, event_loop: &ActiveEventLoop) {
        let Some(game) = &mut self.game else {
            return;
        };
        let status = match game.child.try_wait() {
            Ok(None) => {
                if self.state.is_none() {
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + GAME_POLL));
                }
                return;
            }
            Ok(Some(status)) => Some(status),
            Err(e) => {
                eprintln!("game process: {e}");
                None
            }
        };
        let Some(mut game) = self.game.take() else {
            return;
        };
        for pump in game.pumps.drain(..) {
            let _ = pump.join();
        }
        let home = self.home.as_path();
        if let Some(instance) = instances::find(home, &game.instance) {
            shared::collect_options(home, &instance);
        }
        let crashed = status.filter(|s| !s.success() && !game.stopped);
        let finished = Finished {
            account_key: game.account_key.clone(),
            instance: game.instance.clone(),
            secs: game.started.elapsed().as_secs(),
            crash: crashed.map(|s| report(&game, s)),
        };
        if let Some(state) = &mut self.state {
            state.game_ended(finished);
            return;
        }
        if finished.crash.is_none() && Settings::load(home).exit_after_game {
            if let Some(key) = &finished.account_key {
                auth::accounts::record_play(home, key, finished.secs);
            }
            instances::record_play(home, &finished.instance, finished.secs);
            event_loop.exit();
            return;
        }
        self.finished = Some(finished);
        self.reopen(event_loop);
    }

    /// Starts the game; the window closes for it unless the launcher is set to stay open.
    fn play(&mut self, event_loop: &ActiveEventLoop, request: PlayRequest) {
        if self.game.is_some() {
            return;
        }
        let keep_open = Settings::load(&self.home).keep_open;
        match spawn_game(&request, self.validation, &self.home) {
            Ok(game) => {
                self.game = Some(game);
                if !keep_open {
                    self.state = None;
                    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + GAME_POLL));
                }
            }
            Err(error) => {
                eprintln!("The game could not be started: {error}");
                if let Some(state) = &mut self.state {
                    state.shell.notify(error);
                }
            }
        }
    }

    /// Does what the window asked for.
    fn apply(&mut self, event_loop: &ActiveEventLoop, actions: Vec<Action>) {
        let home = self.home.clone();
        for action in actions {
            let Some(state) = &mut self.state else {
                return;
            };
            match action {
                Action::Play(request) => match instances::find(&home, &request.instance) {
                    Some(instance) if instance.is_java() && self.game.is_none() => {
                        state.start_prepare(request, instance, &home);
                    }
                    // The Melon client extracts and builds its mods itself when it starts.
                    Some(_) => self.play(event_loop, request),
                    None => {}
                },
                Action::Stop => {
                    if let Some(game) = &mut self.game {
                        game.stopped = true;
                        if let Err(e) = game.child.kill() {
                            state.shell.notify(e.to_string());
                        }
                    }
                }
                Action::Install(install, found) => state.start_install(install, found, &home),
                Action::SignIn => state.login.start_login(),
                Action::LanguageChanged => state.set_language(&home),
            }
        }
    }

    /// One frame: lets the window react to what happened, and draws it.
    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &mut self.state else {
            return;
        };
        if let Some(outcome) = state.login.poll() {
            state.shell.finished(outcome);
        }
        if !state.login.is_busy() && state.shell.needs_check() {
            state.login.start_restore();
        }
        state
            .heads
            .update(state.shell.accounts.list(), &mut state.renderer);
        state.shell.textures.update(&mut state.renderer);
        let size = state.size();
        let modal = state.modal_open();
        let status = Status {
            busy: state.busy(),
            running: self.game.as_ref().map(|game| game.instance.as_str()),
            restoring: state.login.is_restoring(),
            catalog_checked: self.catalog_checked,
            heads: &state.heads,
        };

        state.draw_list.clear();
        let mut painter = Painter::new(&mut state.draw_list, state.textures, size);
        let mut cx = Cx::new(&mut painter, &state.strings, &state.input, &mut state.popup);
        let actions = state.shell.frame(&mut cx, &status, modal);
        cx.active = true;
        state.login.frame(&mut cx);
        let close_progress = state.progress.as_ref().is_some_and(|progress| progress.frame(&mut cx));
        let choice = state.crash.as_ref().map_or(Choice::None, |crash| crash.frame(&mut cx));
        cx.finish();
        let cursor = cx.cursor;

        if close_progress {
            state.progress = None;
        }
        match choice {
            Choice::Close => state.crash = None,
            Choice::OpenLog => {
                if let Some(crash) = &state.crash
                    && let Err(e) = open::that(&crash.log)
                {
                    eprintln!("the log could not be opened: {e}");
                }
            }
            Choice::None => {}
        }
        state.input.end_frame();
        state.window.set_cursor(match cursor {
            CursorKind::Default => CursorIcon::Default,
            CursorKind::Pointer => CursorIcon::Pointer,
            CursorKind::Text => CursorIcon::Text,
        });
        if let Err(e) = state.renderer.render(&state.draw_list, size, theme::BG) {
            eprintln!("render: {e:#}");
        }
        state.next_frame = Instant::now() + FRAME_TIME;
        self.apply(event_loop, actions);
    }
}

impl ApplicationHandler for App {
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.poll_game(event_loop);
        if self.catalog_rx.try_recv().is_ok() {
            self.catalog_checked = true;
        }
        let request = self.state.as_mut().and_then(RunningState::poll_prepare);
        if let Some(request) = request {
            self.play(event_loop, request);
        }
        let Some(state) = &mut self.state else { return };
        state.poll_install();
        if Instant::now() >= state.next_frame {
            state.next_frame = Instant::now() + FRAME_TIME;
            state.window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(state.next_frame));
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_none() && self.state.is_none() {
            self.reopen(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        let Some(state) = &mut self.state else { return };
        if state.window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Err(e) = state.renderer.resize(size.width, size.height) {
                    eprintln!("resize: {e:#}");
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor = position;
                state.input.mouse = state.pointer();
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = state.pointer();
                let now = Instant::now();
                state.input.double_click = state.last_press.is_some_and(|(at, (px, py))| {
                    now.duration_since(at) < DOUBLE_CLICK && (px - x).abs() < 4.0 && (py - y).abs() < 4.0
                });
                state.last_press = (!state.input.double_click).then_some((now, (x, y)));
                state.input.click = Some((x, y));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / PIXELS_PER_NOTCH) as f32,
                };
                if !state.modal_open() {
                    state.input.scroll += notches;
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => state.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let escape = event.logical_key == WinitKey::Named(NamedKey::Escape);
                if state.crash.is_some() {
                    if escape {
                        state.crash = None;
                    }
                } else if let Some(progress) = &state.progress {
                    if escape && progress.failed() {
                        state.progress = None;
                    }
                } else if state.login.is_modal_open() {
                    // The login popup closes with its own button.
                } else if escape {
                    state.shell.cancel(&mut state.popup);
                } else {
                    let control = state.modifiers.control_key();
                    state.input.keys.extend(keys_of(&event, control));
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if !state.modal_open() {
                    let typed = text.chars().filter(|c| !c.is_control());
                    state.input.keys.extend(typed.map(Key::Char));
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }
}

/// Opens the launcher window. `validation` is forwarded to the game.
pub fn run(validation: bool) -> anyhow::Result<()> {
    let home = home::dir();
    if let Err(e) = std::fs::create_dir_all(&home) {
        eprintln!("Melon folder {}: {e}", home.display());
    }
    let client_id = auth::microsoft_client_id(&home);

    catalog::load(&home);
    let (catalog_tx, catalog_rx) = mpsc::channel();
    let home_bg = home.clone();
    std::thread::spawn(move || {
        catalog::refresh(&home_bg);
        let _ = catalog_tx.send(());
    });

    let event_loop = EventLoop::new()?;
    let mut app = App {
        state: None,
        home,
        validation,
        client_id,
        game: None,
        finished: None,
        catalog_rx,
        catalog_checked: false,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
