//! Pre-launcher window: pick an account (Microsoft or offline) and an

use melon::app::input::text_events;
use melon::assets::Assets;
use melon::auth::{self, accounts};
use crate::cdn;
use melon::gui::assets::GuiAssets;
use melon::gui::background;
use crate::ui::compile::CompileOverlay;
use crate::ui::crash::{self, CrashModal, CrashReport};
use crate::ui::heads::Heads;
use crate::ui::login::LoginFlow;
use crate::ui::window::{Action, LauncherUi, PlayRequest};
use melon::gui::screen::{CursorKind, GuiScale};
use melon::gui::widgets::Ui;
use melon::home;
use melon::i18n::Lang;
use melon::instances::{self, Engine, LauncherSettings};
use crate::java_play;
use melon::prepare;
use melon::renderer::{Frame, Renderer, quad::DrawList};
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
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

/// The window, in logical pixels.
const WINDOW_SIZE: (f64, f64) = (1280.0, 720.0);
/// The GUI scale per logical pixel: a notch under the Auto scale, whose
const GUI_SCALE: u8 = 2;
/// Menus run at 60 frames a second, like the game's own without a world.
const FRAME_TIME: Duration = Duration::from_nanos(1_000_000_000 / 60);
/// How often to check whether the game process has exited.
const GAME_POLL: Duration = Duration::from_millis(100);
/// One wheel notch in pixels, for touchpads that report pixels.
const PIXELS_PER_NOTCH: f64 = 20.0;
/// How many of the game's last lines are kept for a crash report.
const KEPT_LINES: usize = 64;

struct RunningState {
    renderer: Renderer,
    window: Window,
    gui_assets: GuiAssets,
    lang: Lang,
    ui: LauncherUi,
    login: LoginFlow,
    heads: Heads,
    /// Shown over everything after a game that ended with an error.
    crash: Option<CrashModal>,
    /// Zip extract / rustc while Play waits.
    compile: Option<CompileOverlay>,
    compile_job: Option<CompileJob>,
    prepare_job: Option<PrepareJob>,
    download_job: Option<DownloadJob>,
    cursor: PhysicalPosition<f64>,
    modifiers: ModifiersState,
    draw_list: DrawList,
    /// When the next frame may start.
    next_frame: Instant,
}

impl RunningState {
    /// The launcher draws at [`GUI_SCALE`] per logical pixel.
    fn scale(&self) -> GuiScale {
        scale_for(&self.window)
    }

    /// The cursor in GUI pixels.
    fn pointer(&self) -> (f32, f32) {
        self.scale().to_virtual((self.cursor.x, self.cursor.y))
    }

    /// A popup covers the page: nothing under it reacts.
    fn modal_open(&self) -> bool {
        self.crash.is_some()
            || self.compile.is_some()
            || self.download_job.is_some()
            || self.login.is_modal_open()
    }

    fn update_cursor_icon(&self) {
        let size = self.scale().virtual_size();
        let font = &self.gui_assets.font;
        let icon = if let Some(crash) = &self.crash {
            if crash.over_button(size, |t, w| font.wrap(t, w)) {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            }
        } else if let Some(compile) = &self.compile {
            if compile.over_button() {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            }
        } else if self.login.is_modal_open() {
            if self.login.modal_button_hovered() {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            }
        } else {
            match self.ui.cursor() {
                CursorKind::Default => CursorIcon::Default,
                CursorKind::Pointer => CursorIcon::Pointer,
                CursorKind::Text => CursorIcon::Text,
                CursorKind::ResizeNs => CursorIcon::NsResize,
                CursorKind::ResizeEw => CursorIcon::EwResize,
            }
        };
        self.window.set_cursor(icon);
    }

    /// Does what the page asked for; a request to play is handed back.
    fn apply(
        &mut self,
        action: Action,
        assets: Option<&Assets>,
        event_loop: &ActiveEventLoop,
        home: &Path,
    ) -> Option<PlayRequest> {
        match action {
            Action::Close => event_loop.exit(),
            Action::Play(request) => {
                self.start_compile(request);
            }
            Action::SignIn => {
                self.ui.dismiss_welcome();
                if self.login.has_client_id() {
                    self.login.start_login();
                }
                let assets = LauncherSettings::load(home).melon_play_assets(home);
                if !home::is_prepared_at(&assets) && prepare::scan(home).is_some() {
                    self.start_prepare(home);
                }
            }
            Action::PrepareLocal => self.start_prepare(home),
            Action::LanguageChanged(language) => {
                self.lang = match assets {
                    Some(assets) => Lang::load(assets, language),
                    None => Lang::launcher(language),
                };
            }
            Action::DownloadClient => self.start_download(home),
            Action::None => {}
        }
        None
    }

    fn start_prepare(&mut self, home: &Path) {
        if self.prepare_job.is_some() {
            return;
        }
        if let Some(welcome) = self.ui.welcome_mut() {
            welcome.set_progress("Looking for a client…".into());
        }
        let home = home.to_path_buf();
        let assets = LauncherSettings::load(&home).melon_extract(&home);
        let found = self.ui.welcome_mut().and_then(|w| w.found().cloned());
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            if let Some(found) = found.filter(|f| f.asset_index.is_some()) {
                prepare::extract_into(found, &assets, &|p| {
                    let _ = tx.send(p);
                })
            } else {
                prepare::from_local(&home, &assets, &|p| {
                    let _ = tx.send(p);
                })
            }
        });
        self.prepare_job = Some(PrepareJob {
            rx,
            thread: Some(thread),
        });
    }

    /// Drain prepare lines. `Some(Ok)` when assets are ready.
    fn poll_prepare(&mut self) -> Option<Result<prepare::Report, String>> {
        let Some(job) = &mut self.prepare_job else {
            return None;
        };
        while let Ok(progress) = job.rx.try_recv() {
            if let Some(welcome) = self.ui.welcome_mut() {
                welcome.set_progress(progress.message);
            }
        }
        let finished = job
            .thread
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished);
        if !finished {
            return None;
        }
        let mut job = self.prepare_job.take()?;
        job.thread.take().and_then(|t| t.join().ok())
    }

    fn start_compile(&mut self, request: PlayRequest) {
        if self.compile_job.is_some() || self.download_job.is_some() {
            return;
        }
        let main = self.ui_main_dir();
        let id = request
            .instance
            .clone()
            .or_else(|| LauncherSettings::load(&main).instance_in(&main))
            .unwrap_or_else(|| instances::MELON.into());
        if let Some(instance) = instances::find(&main, &id)
            && instance.engine == Engine::Java
        {
            let settings = LauncherSettings::load(&main);
            let assets = settings.assets_dir(&main);
            let home = main.clone();
            let (tx, rx) = mpsc::channel();
            let thread = std::thread::spawn(move || {
                java_play::prepare_assets(&home, &instance, &assets, &|message| {
                    let _ = tx.send(("-".into(), message));
                })
                .map(|_| 0u128)
            });
            self.compile = Some(CompileOverlay::new());
            self.compile_job = Some(CompileJob {
                request,
                rx,
                thread: Some(thread),
            });
            return;
        }
        let game_dir = instances::dir(&main, &id);
        instances::ensure_dirs(&game_dir);
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let opts = pumpkin_mod::PrepareOpts::for_game_dir(&game_dir);
            pumpkin_mod::prepare(&opts, |event| {
                let _ = tx.send((event.id, event.message));
            })
            .map(|report| {
                pumpkin_mod::activate(&report.mods);
                report.total_millis
            })
            .map_err(|e| e.to_string())
        });
        self.compile = Some(CompileOverlay::new());
        self.compile_job = Some(CompileJob {
            request,
            rx,
            thread: Some(thread),
        });
    }

    fn ui_main_dir(&self) -> PathBuf {
        self.ui.main_dir().to_path_buf()
    }

    /// Drain rustc/extract lines. `Some` when compile succeeded and the game
    fn poll_compile(&mut self) -> Option<PlayRequest> {
        let Some(job) = &mut self.compile_job else {
            return None;
        };
        while let Ok((id, message)) = job.rx.try_recv() {
            if let Some(overlay) = &mut self.compile {
                overlay.push(&id, &message);
            }
        }
        let finished = job
            .thread
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished);
        if !finished {
            return None;
        }
        let mut job = self.compile_job.take()?;
        let result = job
            .thread
            .take()
            .and_then(|t| t.join().ok())
            .unwrap_or_else(|| Err("compile thread failed".into()));
        match result {
            Ok(millis) => {
                if let Some(overlay) = &mut self.compile {
                    overlay.push("-", &format!("ready {millis}ms"));
                }
                self.compile = None;
                Some(job.request)
            }
            Err(error) => {
                if let Some(overlay) = &mut self.compile {
                    overlay.fail(error);
                }
                None
            }
        }
    }

    fn start_download(&mut self, home: &Path) {
        if self.download_job.is_some() || self.compile_job.is_some() {
            return;
        }
        let home = home.to_path_buf();
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            crate::cdn::fetch_game(&home, &|line| {
                let _ = tx.send(line.to_string());
            })
        });
        self.compile = Some(CompileOverlay::download());
        self.download_job = Some(DownloadJob {
            rx,
            thread: Some(thread),
        });
    }

    /// Drain download lines. `Some(Ok)` when the client is on disk.
    fn poll_download(&mut self) -> Option<Result<PathBuf, String>> {
        let Some(job) = &mut self.download_job else {
            return None;
        };
        while let Ok(line) = job.rx.try_recv() {
            if let Some(overlay) = &mut self.compile {
                overlay.push("-", &line);
            }
        }
        let finished = job
            .thread
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished);
        if !finished {
            return None;
        }
        let mut job = self.download_job.take()?;
        let result = job
            .thread
            .take()
            .and_then(|t| t.join().ok())
            .unwrap_or_else(|| Err("download thread failed".into()));
        match &result {
            Ok(_) => self.compile = None,
            Err(error) => {
                if let Some(overlay) = &mut self.compile {
                    overlay.fail(error.clone());
                }
            }
        }
        Some(result)
    }
}

struct CompileJob {
    request: PlayRequest,
    rx: Receiver<(String, String)>,
    thread: Option<JoinHandle<Result<u128, String>>>,
}

struct PrepareJob {
    rx: Receiver<prepare::Progress>,
    thread: Option<JoinHandle<Result<prepare::Report, String>>>,
}

struct DownloadJob {
    rx: Receiver<String>,
    thread: Option<JoinHandle<Result<PathBuf, String>>>,
}

/// The GUI scale for a window: [`GUI_SCALE`] times the display's scaling.
fn scale_for(window: &Window) -> GuiScale {
    let size = window.inner_size();
    let setting = GUI_SCALE.saturating_mul(window.scale_factor().round().max(1.0) as u8);
    GuiScale::new(setting, size.width, size.height)
}

/// A game that is running: the process, and what to do when it ends.
struct RunningGame {
    child: Child,
    started: Instant,
    /// The account playing, to count its play time.
    account_key: Option<String>,
    instance: Option<String>,
    log: PathBuf,
    /// Its last lines, filled by the threads copying its output.
    tail: Arc<Mutex<VecDeque<String>>>,
    pumps: Vec<JoinHandle<()>>,
}

/// How a game ended, to show once the window is back.
struct Finished {
    account_key: Option<String>,
    instance: Option<String>,
    secs: u64,
    crash: Option<CrashReport>,
}

struct App {
    state: Option<RunningState>,
    assets: Option<Assets>,
    home: PathBuf,
    validation: bool,
    client_id: Option<String>,
    /// Set while the game is running; the window is dropped for that time.
    game: Option<RunningGame>,
    /// A game that just ended; the window shows it when it opens.
    finished: Option<Finished>,
    /// Catalog fetch finished; apply on the next wait with a window.
    discover_rx: Receiver<()>,
    discover_fresh: bool,
}

/// Starts the game client (`melon_exe --game`). Output is copied to the
fn spawn_game(
    request: &PlayRequest,
    validation: bool,
    main_dir: &Path,
) -> std::io::Result<RunningGame> {
    let settings = LauncherSettings::load(main_dir);
    let id = request
        .instance
        .clone()
        .or_else(|| settings.instance_in(main_dir))
        .unwrap_or_else(|| instances::MELON.into());
    let instance = instances::find(main_dir, &id);
    if let Some(instance) = &instance {
        instances::apply_global_options(main_dir, instance);
    }
    let game_dir = instance
        .as_ref()
        .map(|i| i.path.clone())
        .unwrap_or_else(|| instances::dir(main_dir, &id));

    let mut cmd = if instance.as_ref().is_some_and(|i| i.engine == Engine::Java) {
        let inst = instance.as_ref().unwrap();
        let accounts = melon::auth::accounts::Accounts::load(main_dir);
        let account = request
            .account_key
            .as_deref()
            .and_then(|key| accounts.list().iter().find(|a| a.key() == key));
        let player = java_play::Player::from_account(account);
        java_play::command(
            main_dir,
            inst,
            &settings.assets_dir(main_dir),
            &player,
            request.join.as_deref(),
        )
        .map_err(std::io::Error::other)?
    } else {
        let mut cmd = Command::new(settings.melon_exe(main_dir));
        cmd.arg("--game");
        cmd.args(["--instance", &id]);
        if let Some(address) = &request.join {
            cmd.args(["--join", address]);
        }
        if validation || request.validation {
            cmd.arg("--vulkan-validation");
        }
        if settings.probe {
            cmd.arg("--probe");
        }
        cmd
    };
    let logs = game_dir.join("logs");
    std::fs::create_dir_all(&logs)?;
    let log = logs.join("latest.log");
    let file = Arc::new(Mutex::new(std::fs::File::create(&log)?));

    let mut child = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
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
        instance: request.instance.clone(),
        log,
        tail,
        pumps,
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

impl App {
    fn init(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let window = melon::platform::create_centered_window(
            event_loop,
            Window::default_attributes()
                .with_title("Melon Launcher")
                .with_inner_size(LogicalSize::new(WINDOW_SIZE.0, WINDOW_SIZE.1)),
        )?;
        window.set_ime_allowed(true);
        let mut renderer = Renderer::new(&window, self.validation)?;
        let gui_assets = GuiAssets::launcher(&mut renderer)?;
        let main_dir = self.home.as_path();

        let mut ui = LauncherUi::new(main_dir);
        let lang = match &self.assets {
            Some(assets) => Lang::load(assets, ui.language()),
            None => Lang::launcher(ui.language()),
        };
        ui.resize(scale_for(&window).virtual_size());

        let mut crash = None;
        if let Some(finished) = self.finished.take() {
            if let Some(key) = &finished.account_key {
                ui.record_play(key, finished.secs);
            }
            if let Some(id) = &finished.instance {
                instances::record_play(&self.home, id, finished.secs);
            }
            crash = finished.crash.map(CrashModal::new);
        }

        self.state = Some(RunningState {
            renderer,
            window,
            gui_assets,
            lang,
            ui,
            login: LoginFlow::new(self.client_id.clone(), main_dir.to_path_buf()),
            heads: Heads::new(main_dir),
            crash,
            compile: None,
            compile_job: None,
            prepare_job: None,
            download_job: None,
            cursor: PhysicalPosition::new(0.0, 0.0),
            modifiers: ModifiersState::empty(),
            draw_list: DrawList::new(),
            next_frame: Instant::now(),
        });
        Ok(())
    }

    /// `true` when the game is running (or just ended and the window was
    fn poll_game(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(game) = &mut self.game else {
            return false;
        };
        let status = match game.child.try_wait() {
            Ok(None) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + GAME_POLL));
                return true;
            }
            Ok(Some(status)) => Some(status),
            Err(e) => {
                eprintln!("Spielprozess: {e}");
                None
            }
        };
        if let Some(mut game) = self.game.take() {
            for pump in game.pumps.drain(..) {
                let _ = pump.join();
            }
            instances::collect_global_options(self.home.as_path(), game.instance.as_deref());
            let finished = Finished {
                account_key: game.account_key.clone(),
                instance: game.instance.clone(),
                secs: game.started.elapsed().as_secs(),
                crash: status.filter(|s| !s.success()).map(|s| report(&game, s)),
            };
            let main_dir = self.home.as_path();
            if finished.crash.is_none() && LauncherSettings::load(main_dir).exit_after_game {
                if let Some(key) = &finished.account_key {
                    accounts::record_play(main_dir, key, finished.secs);
                }
                if let Some(id) = &finished.instance {
                    instances::record_play(main_dir, id, finished.secs);
                }
                event_loop.exit();
                return true;
            }
            self.finished = Some(finished);
        }
        if let Err(e) = self.init(event_loop) {
            eprintln!("Initialisierung fehlgeschlagen: {e:#}");
            event_loop.exit();
        }
        true
    }

    fn play(&mut self, event_loop: &ActiveEventLoop, request: PlayRequest) {
        self.state = None;
        match spawn_game(&request, self.validation, &self.home) {
            Ok(game) => self.game = Some(game),
            Err(e) => {
                eprintln!("Spiel konnte nicht gestartet werden: {e:#}");
                if let Err(e) = self.init(event_loop) {
                    eprintln!("Initialisierung fehlgeschlagen: {e:#}");
                    event_loop.exit();
                }
            }
        }
    }
}

/// The popup for a game that ended with `status`.
fn report(game: &RunningGame, status: std::process::ExitStatus) -> CrashReport {
    let lines = game
        .tail
        .lock()
        .map(|t| t.iter().cloned().collect::<Vec<_>>());
    let lines = lines.unwrap_or_default();
    CrashReport {
        code: status
            .code()
            .map_or_else(|| "?".to_owned(), |c| c.to_string()),
        lines: lines[lines.len().saturating_sub(crash::TAIL_LINES)..].to_vec(),
        log: game.log.clone(),
    }
}

impl ApplicationHandler for App {
    /// The next frame, once its time has come.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.poll_game(event_loop) {
            return;
        }
        let request = self.state.as_mut().and_then(RunningState::poll_compile);
        if let Some(request) = request {
            self.play(event_loop, request);
            return;
        }
        let downloaded = self.state.as_mut().and_then(RunningState::poll_download);
        if let Some(Ok(path)) = downloaded {
            let mut settings = LauncherSettings::load(&self.home);
            settings.melon_binary = Some(path.display().to_string());
            settings.save(&self.home);
        }
        if self.discover_rx.try_recv().is_ok() {
            self.discover_fresh = true;
        }
        if self.discover_fresh
            && let Some(state) = &mut self.state
        {
            state.ui.note_discover();
            self.discover_fresh = false;
        }
        let prepared = self.state.as_mut().and_then(RunningState::poll_prepare);
        if let Some(result) = prepared {
            match result {
                Ok(_) => {
                    let assets_dir =
                        LauncherSettings::load(&self.home).melon_play_assets(&self.home);
                    self.assets = Some(Assets::from_root(assets_dir));
                    if let Some(bin) = prepare::scan_binaries(&self.home) {
                        let mut settings = LauncherSettings::load(&self.home);
                        settings.melon_binary = Some(bin.path.display().to_string());
                        settings.save(&self.home);
                    }
                    if let Some(state) = &mut self.state {
                        state.ui.set_prepared(true);
                        state.ui.dismiss_welcome();
                        if let Some(assets) = &self.assets {
                            state.lang = Lang::load(assets, state.ui.language());
                        }
                    }
                }
                Err(error) => {
                    if let Some(state) = &mut self.state
                        && let Some(welcome) = state.ui.welcome_mut()
                    {
                        welcome.set_error(error);
                    }
                }
            }
        }
        let Some(state) = &mut self.state else { return };
        if Instant::now() >= state.next_frame {
            state.next_frame = Instant::now() + FRAME_TIME;
            event_loop.set_control_flow(ControlFlow::WaitUntil(state.next_frame));
            state.window.request_redraw();
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.game.is_some() || self.state.is_some() {
            return;
        }
        if let Err(e) = self.init(event_loop) {
            eprintln!("Initialisierung fehlgeschlagen: {e:#}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let request = {
            let Some(state) = &mut self.state else { return };
            if state.window.id() != window_id {
                return;
            }
            match event {
                WindowEvent::CloseRequested => {
                    event_loop.exit();
                    return;
                }
                WindowEvent::Resized(size) => {
                    if let Err(e) = state.renderer.resize(size.width, size.height) {
                        eprintln!("Resize fehlgeschlagen: {e:#}");
                    }
                    state.ui.resize(state.scale().virtual_size());
                    None
                }
                WindowEvent::CursorMoved { position, .. } => {
                    state.cursor = position;
                    let (x, y) = state.pointer();
                    state.login.update_hover(x, y);
                    if let Some(crash) = &mut state.crash {
                        crash.hover(x, y);
                    }
                    let size = state.scale().virtual_size();
                    if let Some(compile) = &mut state.compile {
                        compile.hover(x, y, size);
                    }
                    let modal = state.modal_open();
                    state.ui.hover(x, y, !modal);
                    state.update_cursor_icon();
                    None
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    melon::gui::input_state::set_left_held(true);
                    let (x, y) = state.pointer();
                    let size = state.scale().virtual_size();
                    if let Some(popup) = &state.crash {
                        let font = &state.gui_assets.font;
                        match popup.click(x, y, size, |t, w| font.wrap(t, w)) {
                            crash::Choice::Close => state.crash = None,
                            crash::Choice::OpenLog => {
                                if let Err(e) = open::that(popup.log()) {
                                    eprintln!("Log konnte nicht geöffnet werden: {e}");
                                }
                            }
                            crash::Choice::None => {}
                        }
                        None
                    } else if let Some(overlay) = &mut state.compile {
                        if overlay.click(x, y, size) {
                            state.compile = None;
                            state.compile_job = None;
                            state.download_job = None;
                        }
                        None
                    } else if state.login.is_modal_open() {
                        let font = &state.gui_assets.font;
                        state.login.on_modal_click(x, y, size.0, size.1, font);
                        None
                    } else {
                        let action = state.ui.click(x, y);
                        state.apply(action, self.assets.as_ref(), event_loop, &self.home)
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    melon::gui::input_state::set_left_held(false);
                    state.ui.release();
                    None
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let notches = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => (p.y / PIXELS_PER_NOTCH) as f32,
                    };
                    if !state.modal_open() {
                        state.ui.scroll(notches);
                    }
                    None
                }
                WindowEvent::ModifiersChanged(modifiers) => {
                    state.modifiers = modifiers.state();
                    melon::gui::input_state::set_shift(state.modifiers.shift_key());
                    None
                }
                WindowEvent::KeyboardInput { event, .. }
                    if event.state == ElementState::Pressed =>
                {
                    let escape = event.logical_key == Key::Named(NamedKey::Escape);
                    if state.crash.is_some() {
                        if escape {
                            state.crash = None;
                        }
                        return;
                    }
                    if state.compile.is_some() {
                        if escape && state.compile.as_ref().is_some_and(CompileOverlay::failed) {
                            state.compile = None;
                            state.compile_job = None;
                            state.download_job = None;
                        }
                        return;
                    }
                    if state.login.is_modal_open() {
                        return;
                    }
                    if escape {
                        state.ui.cancel();
                        return;
                    }
                    let shift = state.modifiers.shift_key();
                    let control = state.modifiers.control_key();
                    let mut request = None;
                    for key in text_events(&event, shift, control) {
                        let action = state.ui.key(key);
                        request = request.or(state.apply(
                            action,
                            self.assets.as_ref(),
                            event_loop,
                            &self.home,
                        ));
                    }
                    request
                }
                WindowEvent::Ime(Ime::Preedit(text, _)) => {
                    let action = state.ui.key(melon::gui::screen::TextEvent::Preedit(text));
                    state.apply(action, self.assets.as_ref(), event_loop, &self.home)
                }
                WindowEvent::Ime(Ime::Commit(text)) => {
                    if state.crash.is_some()
                        || state.compile.is_some()
                        || state.login.is_modal_open()
                    {
                        return;
                    }
                    let mut request = None;
                    let ended = melon::gui::screen::TextEvent::Preedit(String::new());
                    state.ui.key(ended);
                    for c in text.chars().filter(|c| !c.is_control()) {
                        let action = state.ui.key(melon::gui::screen::TextEvent::Char(c));
                        request = request.or(state.apply(
                            action,
                            self.assets.as_ref(),
                            event_loop,
                            &self.home,
                        ));
                    }
                    request
                }
                WindowEvent::RedrawRequested => {
                    state.gui_assets.sync_font(&mut state.renderer);
                    if let Some(outcome) = state.login.poll() {
                        state.ui.finished(outcome);
                    }
                    if !state.login.is_busy() && state.ui.needs_check() {
                        state.login.start_restore();
                    }
                    state.ui.set_busy(
                        state.login.is_busy()
                            || state.compile_job.is_some()
                            || state.download_job.is_some(),
                    );
                    state.heads.update(
                        state.ui.accounts(),
                        self.assets.as_ref(),
                        &mut state.renderer,
                    );
                    let size = state.scale().virtual_size();

                    state.draw_list.clear();
                    background::push_launcher(
                        &mut state.draw_list,
                        state.gui_assets.white,
                        size.0,
                        size.1,
                    );
                    let mut ui =
                        Ui::new(&mut state.draw_list, &state.gui_assets, &state.lang, size);
                    state
                        .ui
                        .draw(&mut ui, &state.heads, state.login.is_restoring());
                    state.login.draw(&mut ui);
                    if let Some(overlay) = &mut state.compile {
                        overlay.draw(&mut ui);
                    }
                    if let Some(popup) = &state.crash {
                        popup.draw(&mut ui);
                    }

                    let frame = Frame {
                        scene: None,
                        ui: &state.draw_list,
                        ui_size: size,
                        clear_color: background::CLEAR_COLOR,
                        blur: None,
                    };
                    if let Err(e) = state.renderer.render(&frame) {
                        eprintln!("Render-Fehler: {e:#}");
                    }
                    if let Some((x, y, height)) = melon::gui::ime::take_cursor() {
                        let scale = state.scale();
                        let (px, py) = scale.to_physical((x, y));
                        let (_, below) = scale.to_physical((0.0, y + height));
                        state.window.set_ime_cursor_area(
                            PhysicalPosition::new(px, py),
                            winit::dpi::PhysicalSize::new(1.0, (below - py).max(1.0)),
                        );
                    }
                    state.next_frame = Instant::now() + FRAME_TIME;
                    event_loop.set_control_flow(ControlFlow::WaitUntil(state.next_frame));
                    None
                }
                _ => None,
            }
        };
        if let Some(request) = request {
            self.play(event_loop, request);
        }
    }
}

/// Opens the launcher window. `validation` is forwarded to the game process.
pub fn run(validation: bool) -> anyhow::Result<()> {
    let home = home::dir();
    if let Err(e) = std::fs::create_dir_all(&home) {
        eprintln!("Melon-Ordner {}: {e}", home.display());
    }
    let settings = LauncherSettings::load(&home);
    let assets_dir = settings.melon_play_assets(&home);
    let assets = home::is_prepared_at(&assets_dir).then(|| Assets::from_root(assets_dir));
    let client_id = auth::microsoft_client_id(&home);
    if client_id.is_none() {
        eprintln!(
            "Hinweis: Microsoft-Anmeldung nicht konfiguriert (MELON_MS_CLIENT_ID oder client_id.txt im Melon-Datenordner). Offline-Spiel braucht kein Konto."
        );
    }

    cdn::discover::set_current(cdn::discover::load(&home));
    let (discover_tx, discover_rx) = mpsc::channel();
    let home_bg = home.clone();
    std::thread::spawn(move || {
        let discover = cdn::discover::refresh(&home_bg);
        cdn::discover::set_current(discover);
        let _ = discover_tx.send(());
    });

    let event_loop = EventLoop::new()?;
    let mut app = App {
        state: None,
        assets,
        home,
        validation,
        client_id,
        game: None,
        finished: None,
        discover_rx,
        discover_fresh: false,
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
