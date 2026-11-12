//! Application state and logic (port of the script part of `src/App.svelte` plus the Tauri
//! commands in `src-tauri/src/commands.rs`). Views read `AppState` and call its methods.

use crate::{
    logic::{
        autosave::{save_failure, AutosaveStore, SaveState, NOT_LOADED, SAVE_DELAY_MS},
        close::{close_steps, CloseStep},
        desktop::{
            decide_desktop, ActionKind, Decision, DesktopAction, DesktopContext, MenuModel,
            MenuSource, MenuSync,
        },
        i18n::{translate, Locale},
        patch::Patch,
        session_store::newer_status,
        sources::{
            is_favorite, preference_error, record_name, source_catalog, source_ref,
            toggle_favorite, toggle_hidden, PreferencePatch, SourceCatalog, SourceView,
        },
    },
    services::{
        config::ConfigStore,
        lifecycle::CloseState,
        runtime,
        shortcut::{fallback, SaveResult, Shortcuts},
        tray::{Tray, TrayCommand},
    },
};
use futures::{
    channel::{mpsc, oneshot},
    StreamExt,
};
use gpui::{AsyncApp, Context, EventEmitter, WeakEntity};
use std::{sync::Arc, time::Duration};
use translator_core::{
    capture,
    session::{SessionManager, SessionPhase, SessionStatus},
    settings::{Settings, LANGUAGES},
    sources::AudioSource,
};

pub type This = WeakEntity<AppState>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Translation,
    Texts,
    Settings,
}

/// A source the user explicitly asked for (id plus the label to show while it is not listed).
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub id: String,
    pub name: String,
}

/// Read by the Settings page (phase B).
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct ShortcutStatus {
    pub active: Option<String>,
    pub error: Option<String>,
    pub integration_error: Option<String>,
}

pub enum AppEvent {
    /// Bring the main window to the front (tray, errors, failed close).
    ShowWindow,
}

impl EventEmitter<AppEvent> for AppState {}

const POLL_MS: u64 = 150;
const SOURCES_MS: u64 = 3000;
const UNAVAILABLE_APP: &str =
    "Програма недоступна для захоплення. Увімкніть у ній звук і спробуйте ще раз.";

pub struct AppState {
    pub settings: Settings,
    pub status: SessionStatus,
    pub sources: Vec<AudioSource>,
    pub is_windows: bool,
    #[allow(dead_code)] // shown on the Settings page (phase B)
    pub directory: String,
    pub tab: Tab,
    pub loaded: bool,
    pub loading: bool,
    pub closing: bool,
    pub close_failed: bool,
    pub error: String,
    pub source_error: String,
    pub save_state: SaveState,
    pub checking: bool,
    pub connection_message: String,
    pub connection_error: bool,
    pub started: Option<Target>,
    /// An explicit start of a favorite that is not listed right now. Not persisted: it exists
    /// only until it is cancelled, satisfied, or the user starts something else.
    pub waiting: Option<Target>,
    /// The explicit start target captured before discovery; keeps the label when a preference
    /// edit removes the row while the attempt is in flight.
    start_target: Option<Target>,
    preference_refusal: String,
    pub shortcut_status: ShortcutStatus,
    pub recording: bool,
    shortcut_warning: String,
    fetching_sources: bool,
    pub starting: bool,
    start_epoch: u64,
    menu_revision: u64,
    captured: Option<(u64, String)>,

    autosave: AutosaveStore,
    autosave_generation: u64,
    flush_waiters: Vec<oneshot::Sender<Result<(), String>>>,
    close_running: bool,

    config: ConfigStore,
    session: Arc<SessionManager>,
    close_state: Arc<CloseState>,
    shortcuts: Shortcuts,
    tray: Option<Tray>,
    menu_sync: MenuSync,
}

// ---------------------------------------------------------------------------------------------
// Async helpers shared by the state methods. They never hold a state borrow across an await.
// ---------------------------------------------------------------------------------------------

async fn fetch_sources() -> Result<Vec<AudioSource>, String> {
    runtime::blocking(capture::list_sources)
        .await?
        .map_err(|error| error.to_string())
}

async fn stop_native(session: Arc<SessionManager>) -> Result<(), String> {
    runtime::run(async move {
        session.stop().map_err(|e| e.to_string())?;
        session.wait_for_stop().await.map_err(|e| e.to_string())
    })
    .await?
}

async fn start_native(
    config: ConfigStore,
    session: Arc<SessionManager>,
    close_state: Arc<CloseState>,
    source_id: String,
) -> Result<u64, String> {
    runtime::blocking(move || {
        if !close_state.is_ready() {
            return Err("Інтерфейс ще завантажується.".to_string());
        }
        let settings = config.load()?;
        if settings.selected_source_id.as_ref() != Some(&source_id) {
            return Err("Спочатку збережіть вибір програми.".into());
        }
        session.start(settings, source_id).map_err(|e| e.to_string())
    })
    .await?
}

/// The `save_settings` command: validation, shortcut registration with fallback, and the write.
async fn save_settings(
    this: &This,
    cx: &mut AsyncApp,
    mut settings: Settings,
) -> Result<SaveResult, String> {
    let config = this.update(cx, |s, _| s.config.clone()).map_err(|_| "closed")?;
    let validated = {
        let (config, settings) = (config.clone(), settings.clone());
        runtime::blocking(move || config.validate_against_stored(&settings)).await??
    };
    let previous = validated;
    let needs_registry = this
        .update(cx, |s, _| {
            s.shortcuts.needs_registry(settings.capture_shortcut.as_deref())
        })
        .map_err(|_| "closed")?;
    let registration: Result<(), String> = if needs_registry {
        // Registration is bound to the UI thread; the small file write happens inside it.
        this.update(cx, |s, _| {
            let (config, to_save) = (s.config.clone(), settings.clone());
            s.shortcuts
                .configure(settings.capture_shortcut.as_deref(), move || config.save(&to_save))
        })
        .map_err(|_| "closed")?
    } else {
        let (config, to_save) = (config.clone(), settings.clone());
        let outcome = runtime::blocking(move || config.save(&to_save)).await?;
        this.update(cx, |s, _| s.shortcuts.set_error(outcome.clone().err()))
            .map_err(|_| "closed")?;
        outcome
    };
    let shortcut_error = registration.err();
    if shortcut_error.is_some() {
        let active = this
            .update(cx, |s, _| s.shortcuts.control.active())
            .map_err(|_| "closed")?;
        settings.capture_shortcut = fallback(
            previous.capture_shortcut.as_deref(),
            settings.capture_shortcut.as_deref(),
            active.as_deref(),
        );
        // Registration failure is not a settings-disk failure. Save all unrelated
        // edits while returning the effective binding for reconciliation.
        let (config, to_save) = (config, settings.clone());
        runtime::blocking(move || config.save(&to_save)).await??;
    }
    Ok(SaveResult {
        capture_shortcut: settings.capture_shortcut,
        shortcut_error,
    })
}

/// Flushes the autosave store; resolves when disk is up to date or the write failed.
pub async fn flush(this: &This, cx: &mut AsyncApp) -> Result<(), String> {
    let receiver = this
        .update(cx, |s, cx| s.request_flush(cx))
        .map_err(|_| "closed".to_string())?;
    receiver.await.unwrap_or_else(|_| Err("closed".into()))
}

pub async fn refresh_sources(this: This, cx: &mut AsyncApp) {
    let Ok(epoch) = this.update(cx, |s, _| {
        if s.fetching_sources || s.busy() || s.closing {
            None
        } else {
            s.fetching_sources = true;
            Some(s.start_epoch)
        }
    }) else {
        return;
    };
    let Some(epoch) = epoch else { return };
    let result = fetch_sources().await;
    let pending = this
        .update(cx, |s, cx| {
            s.fetching_sources = false;
            let mut pending = None;
            match result {
                Ok(list) => {
                    s.sources = list;
                    s.source_error.clear();
                    pending = s.waiting.clone().map(|target| (target, s.start_epoch));
                }
                Err(reason) => s.source_error = reason,
            }
            s.notify_all(cx);
            pending
        })
        .ok()
        .flatten();
    // Only the exact waited-for id starts, and only while that same wait is still alive. A stop or
    // a selection change during discovery replaced it, so the late listing must not start anything.
    let Some((target, wait_epoch)) = pending else { return };
    let still_waiting = this
        .update(cx, |s, _| {
            s.waiting.as_ref() == Some(&target)
                && s.start_epoch == wait_epoch
                && wait_epoch >= epoch
                && s.sources.iter().any(|source| source.id == target.id)
        })
        .unwrap_or(false);
    if still_waiting {
        start(this, cx, Some(target.id.clone()), Some(target)).await;
    }
}

pub async fn start(this: This, cx: &mut AsyncApp, source_id: Option<String>, continued: Option<Target>) {
    // Capture the requested id/name and waiting eligibility before the first await: star/hide
    // edits made during discovery must not rewrite what the user explicitly asked to capture.
    let Ok(Some((source_id, intent, epoch))) = this.update(cx, |s, cx| {
        let source_id = source_id.or_else(|| s.settings.selected_source_id.clone())?;
        if !s.loaded
            || s.busy()
            || s.starting
            || s.checking
            || s.recording
            || source_id.is_empty()
            || s.closing
        {
            return None;
        }
        let intent = continued.clone().or_else(|| {
            is_favorite(&s.settings.favorite_sources, &source_id).then(|| Target {
                name: record_name(&s.settings.favorite_sources, &[], &source_id)
                    .unwrap_or_else(|| source_id.clone()),
                id: source_id.clone(),
            })
        });
        s.start_target = intent.clone();
        s.starting = true;
        s.error.clear();
        s.waiting = None;
        s.start_epoch += 1;
        let epoch = s.start_epoch;
        s.notify_all(cx);
        Some((source_id, intent, epoch))
    }) else {
        return;
    };
    let outcome = start_attempt(&this, cx, source_id, intent, continued.is_some(), epoch).await;
    let _ = this.update(cx, |s, cx| {
        if let Err(reason) = outcome {
            s.error = reason;
        }
        s.starting = false;
        s.notify_all(cx);
    });
}

async fn start_attempt(
    this: &This,
    cx: &mut AsyncApp,
    source_id: String,
    intent: Option<Target>,
    was_continued: bool,
    epoch: u64,
) -> Result<(), String> {
    // Re-discover the exact requested identity; never substitute another source.
    let available = match fetch_sources().await {
        Ok(list) => list,
        Err(reason) => {
            // A transient rediscovery failure must not consume an armed explicit wait. Only Stop,
            // close, selection change or a completed session end the wait; those bump the epoch
            // or set closing, and then the late failure belongs to a cancelled attempt and is not
            // reported either.
            let current = this
                .update(cx, |s, _| epoch == s.start_epoch && !s.closing)
                .unwrap_or(false);
            if current {
                if was_continued {
                    let _ = this.update(cx, |s, _| s.waiting = intent.clone());
                }
                return Err(reason);
            }
            return Ok(());
        }
    };
    let step = this
        .update(cx, |s, cx| -> Result<Option<(String, Arc<SessionManager>, ConfigStore, Arc<CloseState>)>, String> {
            if epoch != s.start_epoch || s.closing {
                return Ok(None);
            }
            s.sources = available.clone();
            s.source_error.clear();
            let Some(found) = available.iter().find(|source| source.id == source_id) else {
                // Only the captured explicit intent can wait; current favorite membership is not
                // re-read after discovery, so unstarring during the await cannot cancel the request.
                let Some(intent) = intent.clone() else {
                    return Err(UNAVAILABLE_APP.into());
                };
                s.waiting = Some(intent.clone());
                if s.settings.selected_source_id.as_ref() != Some(&intent.id) {
                    s.settings.selected_source_id = Some(intent.id);
                    s.autosave_update(cx);
                }
                s.notify_all(cx);
                return Ok(None);
            };
            let name = found.name.clone();
            s.start_target = Some(Target {
                id: source_id.clone(),
                name: name.clone(),
            });
            s.settings.selected_source_id = Some(source_id.clone());
            s.autosave_update(cx);
            s.notify_all(cx);
            Ok(Some((name, s.session.clone(), s.config.clone(), s.close_state.clone())))
        })
        .map_err(|_| "closed".to_string())??;
    let Some((name, session, config, close_state)) = step else {
        return Ok(());
    };
    flush(this, cx).await?;
    let stale = |this: &This, cx: &mut AsyncApp| {
        this.update(cx, |s, _| epoch != s.start_epoch || s.closing)
            .unwrap_or(true)
    };
    if stale(this, cx) {
        return Ok(());
    }
    let id = start_native(config, session.clone(), close_state, source_id.clone()).await?;
    // Stopped or closed while the native start was in flight: close exactly that late session
    // instead of adopting it, and never adopt it into newer UI state.
    if stale(this, cx) {
        let _ = stop_native(session).await;
        return Ok(());
    }
    let _ = this.update(cx, |s, cx| {
        s.started = Some(Target {
            id: source_id.clone(),
            name,
        });
        s.captured = Some((id, source_id));
        let mut starting = SessionStatus::default();
        starting.session_id = id;
        starting.phase = SessionPhase::Starting;
        s.status = newer_status(std::mem::take(&mut s.status), starting);
        s.poll(cx);
    });
    Ok(())
}

pub async fn stop(this: This, cx: &mut AsyncApp) {
    let Ok(session) = this.update(cx, |s, cx| {
        s.start_epoch += 1;
        s.waiting = None;
        s.error.clear();
        s.notify_all(cx);
        s.session.clone()
    }) else {
        return;
    };
    let result = stop_native(session).await;
    let _ = this.update(cx, |s, cx| match result {
        Ok(()) => s.poll(cx),
        Err(reason) => {
            s.error = reason;
            s.notify_all(cx);
        }
    });
}

async fn desktop_action(this: This, cx: &mut AsyncApp, action: DesktopAction) {
    // Native check items auto-toggle even when the selected value is unchanged.
    // Reconcile after a menu click, not on every session-meter update.
    let Ok(decision) = this.update(cx, |s, cx| {
        if matches!(action.kind, ActionKind::Source | ActionKind::Language) {
            s.menu_revision += 1;
            s.notify_all(cx);
        }
        decide_desktop(
            &action,
            &DesktopContext {
                loaded: s.loaded,
                closing: s.closing,
                checking: s.checking,
                recording: s.recording,
                starting: s.starting,
                waiting: s.waiting.is_some(),
                phase: s.status.phase,
                settings: &s.settings,
            },
        )
    }) else {
        return;
    };
    match decision {
        Decision::Ignore => return,
        Decision::Stop => stop(this.clone(), cx).await,
        Decision::Start(id) => start(this.clone(), cx, Some(id), None).await,
        Decision::Error(message) => {
            let _ = this.update(cx, |s, cx| {
                s.error = message;
                s.notify_all(cx);
            });
        }
        Decision::Source(id) => {
            let _ = this.update(cx, |s, cx| {
                if s.catalog().visible.iter().any(|source| source.id == id) {
                    s.change(Patch::selected_source(Some(id)), cx);
                }
            });
        }
        Decision::Language(code) => {
            let _ = this.update(cx, |s, cx| {
                if LANGUAGES.iter().any(|language| language.code == code) {
                    s.change(Patch::target_language(code), cx);
                }
            });
        }
        Decision::Refresh => refresh_sources(this.clone(), cx).await,
    }
    let _ = this.update(cx, |s, cx| {
        if !s.error.is_empty() {
            cx.emit(AppEvent::ShowWindow);
        }
    });
}

async fn run_close(this: This, cx: &mut AsyncApp, save: bool) {
    let Ok((session, close_state)) = this.update(cx, |s, _| (s.session.clone(), s.close_state.clone())) else {
        return;
    };
    let mut outcome: Result<(), String> = Ok(());
    for step in close_steps(save) {
        let result = match step {
            CloseStep::Begin => {
                session.begin_close();
                Ok(())
            }
            CloseStep::Flush => flush(&this, cx).await,
            CloseStep::Stop => stop_native(session.clone()).await,
            CloseStep::Finish => {
                session.begin_close();
                let waited = {
                    let session = session.clone();
                    runtime::run(async move { session.wait_for_stop().await.map_err(|e| e.to_string()) })
                        .await
                        .and_then(|result| result)
                };
                if waited.is_ok() {
                    close_state.allow_exit();
                    let _ = this.update(cx, |_, cx| cx.quit());
                }
                waited
            }
        };
        if let Err(reason) = result {
            outcome = Err(reason);
            break;
        }
    }
    let _ = this.update(cx, |s, cx| {
        s.close_running = false;
        if outcome.is_err() {
            s.close_failed = true;
            cx.emit(AppEvent::ShowWindow);
        }
        s.notify_all(cx);
    });
}

async fn check_connection(this: This, cx: &mut AsyncApp) {
    let Ok(Some((session, config))) = this.update(cx, |s, cx| {
        if !s.loaded
            || s.busy()
            || s.starting
            || s.checking
            || s.closing
            || s.waiting.is_some()
            || s.settings.api_key.trim().is_empty()
        {
            return None;
        }
        s.checking = true;
        s.connection_message.clear();
        s.connection_error = false;
        s.notify_all(cx);
        Some((s.session.clone(), s.config.clone()))
    }) else {
        return;
    };
    let result: Result<f64, String> = async {
        flush(&this, cx).await?;
        runtime::run(async move {
            let settings = config.load()?;
            let report = session
                .check_connection(settings)
                .await
                .map_err(|error| error.to_string())?;
            Ok::<f64, String>(report.elapsed_ms as f64 / 1000.0)
        })
        .await?
    }
    .await;
    let _ = this.update(cx, |s, cx| {
        match result {
            Ok(seconds) => {
                s.connection_message = translate(
                    "Google Live Translation доступний ({seconds} с). Сесію закрито, аудіо не передавалося. Це не гарантія тривалої роботи чи обсягу квоти.",
                    Locale::Uk,
                    &[("seconds", format!("{seconds:.2}"))],
                );
            }
            Err(reason) => {
                s.connection_error = true;
                s.connection_message = reason;
            }
        }
        s.checking = false;
        s.notify_all(cx);
    });
}

impl AppState {
    /// Creates the services and starts the background loops. Must run on the UI thread after the
    /// platform loop exists.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let config = ConfigStore::new();
        let session = Arc::new(SessionManager::default());
        let close_state = Arc::new(CloseState::default());
        let (action_tx, mut action_rx) = mpsc::unbounded::<DesktopAction>();
        let (tray_tx, mut tray_rx) = mpsc::unbounded::<TrayCommand>();

        let mut shortcuts = Shortcuts::initialize(session.clone(), close_state.clone(), action_tx);
        let stored = config.load().ok();
        let language = stored
            .as_ref()
            .map(|settings| settings.interface_language.clone())
            .unwrap_or_else(|| Settings::default().interface_language);
        let tray = match Tray::initialize(&language, tray_tx) {
            Ok(tray) => Some(tray),
            Err(_) => {
                shortcuts
                    .control
                    .status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .integration_error = Some(
                    "Трей недоступний у цьому середовищі; використовуйте основне вікно.".into(),
                );
                None
            }
        };
        if let Some(stored) = &stored {
            let _ = shortcuts.configure(stored.capture_shortcut.as_deref(), || Ok(()));
        }

        let state = Self {
            settings: Settings::default(),
            status: SessionStatus::default(),
            sources: Vec::new(),
            is_windows: cfg!(target_os = "windows"),
            directory: config.directory.to_string_lossy().into_owned(),
            tab: Tab::Translation,
            loaded: false,
            loading: false,
            closing: false,
            close_failed: false,
            error: String::new(),
            source_error: String::new(),
            save_state: SaveState::Loading,
            checking: false,
            connection_message: String::new(),
            connection_error: false,
            started: None,
            waiting: None,
            start_target: None,
            preference_refusal: String::new(),
            shortcut_status: ShortcutStatus::default(),
            recording: false,
            shortcut_warning: String::new(),
            fetching_sources: false,
            starting: false,
            start_epoch: 0,
            menu_revision: 0,
            captured: None,
            autosave: AutosaveStore::default(),
            autosave_generation: 0,
            flush_waiters: Vec::new(),
            close_running: false,
            config,
            session,
            close_state,
            shortcuts,
            tray,
            menu_sync: MenuSync::default(),
        };

        // Tray menu clicks and hotkey presses -> foreground tasks.
        cx.spawn(async move |this, cx| {
            while let Some(command) = tray_rx.next().await {
                match command {
                    TrayCommand::Show => {
                        let _ = this.update(cx, |_, cx| cx.emit(AppEvent::ShowWindow));
                    }
                    TrayCommand::Exit => {
                        let _ = this.update(cx, |s, cx| s.request_close(false, cx));
                    }
                    TrayCommand::Action(action) => {
                        desktop_action(this.clone(), cx, action).await;
                    }
                }
            }
        })
        .detach();
        cx.spawn(async move |this, cx| {
            while let Some(action) = action_rx.next().await {
                desktop_action(this.clone(), cx, action).await;
            }
        })
        .detach();

        // Status polling and source discovery.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(POLL_MS))
                .await;
            if this.update(cx, |s, cx| s.poll(cx)).is_err() {
                break;
            }
        })
        .detach();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(SOURCES_MS))
                .await;
            if this.upgrade().is_none() {
                break;
            }
            refresh_sources(this.clone(), cx).await;
        })
        .detach();

        state.close_state.set_ready();
        state.load(cx);
        state
    }

    // ---- derived values ---------------------------------------------------------------------

    pub fn locale(&self) -> Locale {
        Locale::from_code(&self.settings.interface_language)
    }

    pub fn busy(&self) -> bool {
        matches!(
            self.status.phase,
            SessionPhase::Starting | SessionPhase::Running | SessionPhase::Stopping
        )
    }

    /// The phase to show: an in-flight start reads as "starting" before the core reports it.
    pub fn display_phase(&self) -> SessionPhase {
        if self.starting && !self.busy() {
            SessionPhase::Starting
        } else {
            self.status.phase
        }
    }

    pub fn catalog(&self) -> SourceCatalog {
        source_catalog(
            &self.sources,
            &self.settings.favorite_sources,
            &self.settings.hidden_sources,
        )
    }

    fn listed(&self) -> Option<SourceView> {
        let selected = self.settings.selected_source_id.as_ref()?;
        self.catalog().all.into_iter().find(|source| &source.id == selected)
    }

    pub fn can_start_source(&self) -> bool {
        self.listed()
            .is_some_and(|listed| listed.available || listed.favorite)
            && !self.settings.api_key.trim().is_empty()
            && self.source_error.is_empty()
    }

    pub fn selected_name(&self) -> Option<String> {
        let selected = self.settings.selected_source_id.as_deref();
        let matching = |target: &Option<Target>| {
            target
                .as_ref()
                .filter(|target| Some(target.id.as_str()) == selected)
                .map(|target| target.name.clone())
        };
        self.listed()
            .map(|listed| listed.name)
            .or_else(|| matching(&self.waiting))
            .or_else(|| matching(&self.start_target))
            .or_else(|| matching(&self.started))
            .or_else(|| {
                selected.and_then(|id| {
                    record_name(&self.settings.favorite_sources, &self.settings.hidden_sources, id)
                })
            })
    }

    /// Tray available: the close button only hides the window.
    pub fn has_tray(&self) -> bool {
        self.tray.is_some()
    }

    pub fn close_state(&self) -> Arc<CloseState> {
        self.close_state.clone()
    }

    // ---- notification and tray sync ---------------------------------------------------------

    /// Re-renders and pushes the semantic model to the tray menu.
    pub fn notify_all(&mut self, cx: &mut Context<Self>) {
        self.sync_menu();
        cx.notify();
    }

    fn sync_menu(&mut self) {
        if !self.loaded {
            return;
        }
        let allowed = !self.closing && !self.checking && !self.recording;
        let busy = self.busy();
        let waiting = self.waiting.is_some();
        let free = allowed && !busy && !self.starting && !waiting;
        let model = MenuModel {
            revision: self.menu_revision,
            interface_language: self.settings.interface_language.clone(),
            sources: self
                .catalog()
                .visible
                .into_iter()
                .map(|view| MenuSource {
                    id: view.id,
                    name: view.name,
                })
                .collect(),
            selected_source_id: self.settings.selected_source_id.clone(),
            target_language: self.settings.target_language.clone(),
            can_select: free,
            can_start: free && self.can_start_source(),
            can_stop: allowed
                && (busy || self.starting || waiting)
                && self.status.phase != SessionPhase::Stopping,
            waiting,
        };
        let Some(model) = self.menu_sync.changed(&model) else {
            return;
        };
        if let Some(tray) = &mut self.tray {
            if tray.sync(&model).is_err() {
                self.menu_sync.invalidate();
                self.error = "Не вдалося оновити меню трея.".into();
            }
        }
    }

    // ---- loading ------------------------------------------------------------------------------

    pub fn load(&self, cx: &mut Context<Self>) {
        let config = self.config.clone();
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |s, cx| {
                s.loading = true;
                s.loaded = false;
                s.error.clear();
                s.notify_all(cx);
            });
            match runtime::blocking(move || config.load()).await.and_then(|r| r) {
                Ok(settings) => {
                    let _ = this.update(cx, |s, cx| {
                        s.settings = settings;
                        s.save_state = s.autosave.init(&s.settings);
                        s.loaded = true;
                        s.loading = false;
                        s.update_shortcut_status();
                        s.notify_all(cx);
                    });
                    refresh_sources(this.clone(), cx).await;
                }
                Err(reason) => {
                    let _ = this.update(cx, |s, cx| {
                        s.error = reason;
                        s.loading = false;
                        s.notify_all(cx);
                    });
                }
            }
        })
        .detach();
    }

    /// The "Повторити завантаження" button.
    pub fn retry_load(&mut self, cx: &mut Context<Self>) {
        self.load(cx);
    }

    fn update_shortcut_status(&mut self) {
        let status = self.shortcuts.control.status();
        self.shortcut_status = ShortcutStatus {
            active: status.active,
            error: status.error.or_else(|| {
                (!self.shortcut_warning.is_empty()).then(|| self.shortcut_warning.clone())
            }),
            integration_error: status.integration_error,
        };
    }

    // ---- polling ----------------------------------------------------------------------------

    pub fn poll(&mut self, cx: &mut Context<Self>) {
        if !self.loaded || self.closing {
            return;
        }
        let incoming = self.session.status();
        if let Some(tray) = &mut self.tray {
            tray.update(&incoming);
        }
        let was_busy = self.busy();
        let (old_id, old_phase) = (self.status.session_id, self.status.phase);
        let old_message = self.status.message.clone();
        self.status = newer_status(std::mem::take(&mut self.status), incoming);
        if let Some((id, source)) = self.captured.clone() {
            if id == self.status.session_id
                && self.status.phase == SessionPhase::Running
                && self.settings.last_captured_source_id.as_ref() != Some(&source)
            {
                self.change(
                    Patch {
                        last_captured_source_id: Some(Some(source)),
                        ..Patch::default()
                    },
                    cx,
                );
            }
        }
        if was_busy
            || self.busy()
            || old_id != self.status.session_id
            || old_phase != self.status.phase
            || old_message != self.status.message
        {
            self.notify_all(cx);
        }
    }

    // ---- settings edits ---------------------------------------------------------------------

    pub fn change(&mut self, patch: Patch, cx: &mut Context<Self>) {
        let locked_edit = patch.selected_source_id.is_some() || patch.target_language.is_some();
        if !self.loaded
            || self.closing
            || self.checking
            || (locked_edit && (self.busy() || self.starting))
        {
            return;
        }
        if let Some(selected) = &patch.selected_source_id {
            if selected != &self.settings.selected_source_id {
                self.start_epoch += 1;
                self.waiting = None;
            }
        }
        if patch.capture_shortcut.is_some() {
            self.shortcut_warning.clear();
        }
        patch.apply(&mut self.settings);
        if patch.api_key.is_some() || patch.target_language.is_some() {
            self.connection_message.clear();
            self.connection_error = false;
        }
        self.autosave_update(cx);
        if patch.volumes()
            && self
                .session
                .set_gains(self.settings.original_volume, self.settings.translation_volume)
                .is_err()
        {
            self.error =
                "Не вдалося застосувати гучність. Зупиніть переклад і спробуйте ще раз.".into();
        }
        self.notify_all(cx);
    }

    fn change_preference(&mut self, patch: Patch, problem: Option<&'static str>, cx: &mut Context<Self>) {
        if let Some(problem) = problem {
            self.preference_refusal = problem.into();
            self.error = problem.into();
            self.notify_all(cx);
            return;
        }
        // The refusal banner belongs to preference edits only; a later unrelated error stays visible.
        if !self.preference_refusal.is_empty() && self.error == self.preference_refusal {
            self.error.clear();
        }
        self.preference_refusal.clear();
        self.change(patch, cx);
    }

    pub fn toggle_favorite(&mut self, view: &SourceView, cx: &mut Context<Self>) {
        let favorites = toggle_favorite(
            &self.settings.favorite_sources,
            &source_ref(&view.id, &view.name),
        );
        let problem = preference_error(
            &self.settings,
            &PreferencePatch {
                favorite_sources: Some(favorites.clone()),
                hidden_sources: None,
            },
        );
        self.change_preference(
            Patch {
                favorite_sources: Some(favorites),
                ..Patch::default()
            },
            problem,
            cx,
        );
    }

    pub fn hide_source(&mut self, view: &SourceView, cx: &mut Context<Self>) {
        let hidden = toggle_hidden(&self.settings.hidden_sources, &source_ref(&view.id, &view.name));
        self.set_hidden(hidden, cx);
    }

    pub fn restore_source(&mut self, view: &SourceView, cx: &mut Context<Self>) {
        let hidden: Vec<_> = self
            .settings
            .hidden_sources
            .iter()
            .filter(|record| record.id != view.id)
            .cloned()
            .collect();
        self.set_hidden(hidden, cx);
    }

    fn set_hidden(&mut self, hidden: Vec<translator_core::settings::SourceRef>, cx: &mut Context<Self>) {
        let problem = preference_error(
            &self.settings,
            &PreferencePatch {
                favorite_sources: None,
                hidden_sources: Some(hidden.clone()),
            },
        );
        self.change_preference(
            Patch {
                hidden_sources: Some(hidden),
                ..Patch::default()
            },
            problem,
            cx,
        );
    }

    /// Selecting a source from the sidebar also returns to the translation page.
    pub fn select_source(&mut self, id: String, cx: &mut Context<Self>) {
        self.change(Patch::selected_source(Some(id)), cx);
        self.tab = Tab::Translation;
        self.notify_all(cx);
    }

    pub fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        self.notify_all(cx);
    }

    pub fn refresh(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| refresh_sources(this, cx).await).detach();
    }

    pub fn start_selected(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| start(this, cx, None, None).await).detach();
    }

    pub fn stop_translation(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| stop(this, cx).await).detach();
    }

    // ---- autosave ---------------------------------------------------------------------------

    fn autosave_update(&mut self, cx: &mut Context<Self>) {
        let Some(update) = self.autosave.update(&self.settings) else {
            return;
        };
        self.autosave_generation += 1;
        if let Some(state) = update.state {
            self.save_state = state;
        }
        if update.arm_timer {
            let generation = self.autosave_generation;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(SAVE_DELAY_MS))
                    .await;
                let _ = this.update(cx, |s, cx| {
                    if s.autosave_generation == generation {
                        s.kick_drain(cx);
                    }
                });
            })
            .detach();
        }
    }

    /// Starts a flush now; the receiver resolves when disk is current or the write failed.
    fn request_flush(&mut self, cx: &mut Context<Self>) -> oneshot::Receiver<Result<(), String>> {
        self.autosave_generation += 1; // cancels the debounce timer
        let (sender, receiver) = oneshot::channel();
        if !self.autosave.is_initialized() {
            let _ = sender.send(Err(NOT_LOADED.into()));
        } else if !self.autosave.is_dirty() && !self.autosave.is_writing() {
            let _ = sender.send(Ok(()));
        } else {
            self.flush_waiters.push(sender);
            self.kick_drain(cx);
        }
        receiver
    }

    fn finish_waiters(&mut self, result: Result<(), String>) {
        for waiter in self.flush_waiters.drain(..) {
            let _ = waiter.send(result.clone());
        }
    }

    fn kick_drain(&mut self, cx: &mut Context<Self>) {
        if !self.autosave.is_dirty() && !self.autosave.is_writing() {
            return;
        }
        if !self.autosave.start_drain() {
            return;
        }
        cx.spawn(async move |this, cx| loop {
            let next = this.update(cx, |s, cx| {
                let next = s.autosave.next_write();
                if next.is_some() {
                    s.save_state = SaveState::Saving;
                    cx.notify();
                }
                next
            });
            let Ok(next) = next else { break };
            let Some((key, value)) = next else {
                let _ = this.update(cx, |s, cx| {
                    s.save_state = s.autosave.end_drain(false);
                    s.finish_waiters(Ok(()));
                    s.notify_all(cx);
                });
                break;
            };
            let requested = value.capture_shortcut.clone();
            let result = save_settings(&this, cx, value).await;
            let keep_going = this.update(cx, |s, cx| {
                s.update_shortcut_status_after_save(&result, requested, cx)
                    .map(|()| s.autosave.wrote(key))
            });
            match keep_going {
                Ok(Ok(())) => {}
                Ok(Err(reason)) => {
                    let _ = this.update(cx, |s, cx| {
                        s.save_state = s.autosave.end_drain(true);
                        s.finish_waiters(Err(save_failure(&reason)));
                        s.update_shortcut_status();
                        s.notify_all(cx);
                    });
                    break;
                }
                Err(_) => break,
            }
        })
        .detach();
    }

    /// Reconciles the shortcut after a write; `Err` carries the failure of the write itself.
    fn update_shortcut_status_after_save(
        &mut self,
        result: &Result<SaveResult, String>,
        requested: Option<String>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(reason) => return Err(reason.clone()),
        };
        if let Some(error) = &outcome.shortcut_error {
            self.shortcut_warning = error.clone();
        }
        if outcome.capture_shortcut != requested && self.settings.capture_shortcut == requested {
            self.settings.capture_shortcut = outcome.capture_shortcut.clone();
            self.autosave_update(cx);
        }
        self.update_shortcut_status();
        cx.notify();
        Ok(())
    }

    #[allow(dead_code)] // the Settings page "retry" button (phase B)
    pub fn retry_save(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let result = flush(&this, cx).await;
            let _ = this.update(cx, |s, cx| {
                match result {
                    Ok(()) => s.error.clear(),
                    Err(reason) => s.error = reason,
                }
                s.notify_all(cx);
            });
        })
        .detach();
    }

    // ---- phase B surface --------------------------------------------------------------------

    /// Suspends the global shortcut while the settings page records a new one.
    #[allow(dead_code)]
    pub fn record_shortcut(&mut self, recording: bool, cx: &mut Context<Self>) -> Result<(), String> {
        if recording
            && (self.closing
                || self.checking
                || self.starting
                || self.busy()
                || self.waiting.is_some())
        {
            return Err("Спочатку зупиніть переклад.".into());
        }
        self.recording = recording;
        self.shortcuts.control.set_suspended(recording);
        self.notify_all(cx);
        Ok(())
    }

    #[allow(dead_code)]
    pub fn check_connection(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| check_connection(this, cx).await).detach();
    }

    #[allow(dead_code)]
    pub fn open_directory(&mut self, cx: &mut Context<Self>) {
        if let Err(reason) = self.config.open_directory() {
            self.error = reason;
            self.notify_all(cx);
        }
    }

    // ---- close flow -------------------------------------------------------------------------

    pub fn request_close(&mut self, discard: bool, cx: &mut Context<Self>) {
        if self.closing && !self.close_failed {
            return;
        }
        // Gate new starts immediately (the close sequence repeats this as its first step).
        self.session.begin_close();
        self.start_epoch += 1;
        self.waiting = None;
        self.closing = true;
        self.close_failed = false;
        self.notify_all(cx);
        if self.close_running {
            return;
        }
        self.close_running = true;
        cx.spawn(async move |this, cx| run_close(this, cx, !discard).await)
            .detach();
    }

    pub fn cancel_close(&mut self, cx: &mut Context<Self>) {
        if self.close_running {
            return;
        }
        self.session.cancel_close();
        self.closing = false;
        self.close_failed = false;
        self.poll(cx);
        self.notify_all(cx);
    }
}
