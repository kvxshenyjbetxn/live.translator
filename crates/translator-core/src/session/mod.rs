pub mod diagnostics;
mod sender;
mod worker;
use crate::{audio::LiveGains, settings::Settings};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionPhase {
    Idle,
    Starting,
    Running,
    Stopping,
    Error,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub session_id: u64,
    pub phase: SessionPhase,
    pub elapsed_seconds: u64,
    pub input_level: f32,
    pub sent_audio_seconds: f64,
    pub received_audio_seconds: f64,
    pub playback_backlog_seconds: f64,
    pub playback_speed: f32,
    pub diagnostics: diagnostics::DiagnosticsSnapshot,
    pub message: Option<String>,
    #[serde(skip)]
    cleanup_error: Option<String>,
}
impl Default for SessionStatus {
    fn default() -> Self {
        Self {
            session_id: 0,
            phase: SessionPhase::Idle,
            elapsed_seconds: 0,
            input_level: 0.,
            sent_audio_seconds: 0.,
            received_audio_seconds: 0.,
            playback_backlog_seconds: 0.,
            playback_speed: 1.,
            diagnostics: diagnostics::DiagnosticsSnapshot::default(),
            message: None,
            cleanup_error: None,
        }
    }
}
#[derive(Debug, Clone, thiserror::Error)]
pub enum SessionError {
    #[error("{0}")]
    Operation(String),
    #[error("{0}")]
    Cleanup(String),
}
#[derive(Clone)]
pub struct Reporter {
    status: Arc<Mutex<SessionStatus>>,
    id: u64,
}
impl Reporter {
    pub fn update(&self, update: impl FnOnce(&mut SessionStatus)) {
        let mut status = self.status.lock().unwrap_or_else(|e| e.into_inner());
        if status.session_id == self.id && status.phase != SessionPhase::Stopping {
            update(&mut status);
        }
    }
}
pub trait SessionRunner: Send + Sync {
    fn run(
        &self,
        settings: Settings,
        source_id: String,
        cancel: CancellationToken,
        reporter: Reporter,
        gains: Arc<LiveGains>,
    ) -> Result<(), SessionError>;
}
struct Active {
    cancel: CancellationToken,
    thread: JoinHandle<()>,
}
#[derive(Default)]
struct Control {
    active: Option<Active>,
    closing: bool,
    next_id: u64,
    probe: Option<(u64, CancellationToken)>,
}
struct ConnectionProbe<'a> {
    control: &'a Mutex<Control>,
    id: u64,
    token: CancellationToken,
}
impl Drop for ConnectionProbe<'_> {
    fn drop(&mut self) {
        let mut control = self
            .control
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if control.probe.as_ref().is_some_and(|(id, _)| *id == self.id) {
            control.probe = None;
        }
    }
}
pub struct SessionManager {
    control: Mutex<Control>,
    status: Arc<Mutex<SessionStatus>>,
    gains: Arc<LiveGains>,
    runner: Arc<dyn SessionRunner>,
}
impl Default for SessionManager {
    fn default() -> Self {
        Self::with_runner(Arc::new(worker::LiveRunner))
    }
}
impl SessionManager {
    pub fn with_runner(runner: Arc<dyn SessionRunner>) -> Self {
        Self {
            control: Mutex::default(),
            status: Arc::new(Mutex::new(SessionStatus::default())),
            gains: Arc::new(LiveGains::new(0.15, 0.7).expect("valid defaults")),
            runner,
        }
    }
    fn reserve_probe(&self) -> Result<ConnectionProbe<'_>, SessionError> {
        let mut control = self
            .control
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if control.closing
            || control.probe.is_some()
            || control
                .active
                .as_ref()
                .is_some_and(|active| !active.thread.is_finished())
        {
            return Err(SessionError::Operation(
                "Спочатку зупиніть переклад або дочекайтеся завершення перевірки.".into(),
            ));
        }
        if let Some(message) = self
            .status
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .cleanup_error
            .clone()
        {
            return Err(SessionError::Cleanup(message));
        }
        control.next_id += 1;
        let id = control.next_id;
        let token = CancellationToken::new();
        control.probe = Some((id, token.clone()));
        Ok(ConnectionProbe {
            control: &self.control,
            id,
            token,
        })
    }
    pub async fn check_connection(
        &self,
        settings: Settings,
    ) -> Result<crate::provider::ConnectionReport, SessionError> {
        settings
            .validate()
            .map_err(|error| SessionError::Operation(error.to_string()))?;
        let probe = self.reserve_probe()?;
        crate::provider::gemini::GeminiClient::check_connection(
            crate::provider::TranslationConfig {
                api_key: settings.api_key.trim().into(),
                target_language: settings.target_language,
            },
            probe.token.clone(),
        )
        .await
        .map_err(|error| SessionError::Operation(error.to_string()))
    }
    pub fn start(&self, settings: Settings, source_id: String) -> Result<u64, SessionError> {
        settings
            .validate()
            .map_err(|e| SessionError::Operation(e.to_string()))?;
        if settings.api_key.trim().is_empty() || source_id.is_empty() || source_id.len() > 4096 {
            return Err(SessionError::Operation(
                "Вкажіть API-ключ і виберіть програму.".into(),
            ));
        }
        let mut control = self.control.lock().unwrap_or_else(|e| e.into_inner());
        if control.closing {
            return Err(SessionError::Operation("Програма завершує роботу.".into()));
        }
        if control.probe.is_some() {
            return Err(SessionError::Operation(
                "Зачекайте завершення перевірки з'єднання.".into(),
            ));
        }
        if control
            .active
            .as_ref()
            .is_some_and(|active| !active.thread.is_finished())
        {
            return Err(SessionError::Operation(
                "Попередня сесія ще працює або зупиняється.".into(),
            ));
        }
        if let Some(active) = control.active.take() {
            let _ = active.thread.join();
        }
        if let Some(message) = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cleanup_error
            .clone()
        {
            return Err(SessionError::Cleanup(message));
        }
        self.set_gains(settings.original_volume, settings.translation_volume)?;
        control.next_id += 1;
        let id = control.next_id;
        let cancel = CancellationToken::new();
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = SessionStatus {
            session_id: id,
            phase: SessionPhase::Starting,
            ..SessionStatus::default()
        };
        let runner = self.runner.clone();
        let gains = self.gains.clone();
        let status = self.status.clone();
        let worker_cancel = cancel.clone();
        let thread = std::thread::Builder::new()
            .name("live-translator-session".into())
            .spawn(move || {
                let reporter = Reporter {
                    status: status.clone(),
                    id,
                };
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runner.run(settings, source_id, worker_cancel.clone(), reporter, gains)
                }))
                .unwrap_or_else(|_| {
                    Err(SessionError::Cleanup(
                        "Внутрішня помилка аудіосесії. Перевірте повернення оригіналу.".into(),
                    ))
                });
                let mut state = status.lock().unwrap_or_else(|e| e.into_inner());
                if state.session_id == id {
                    state.phase = if result.is_ok() {
                        SessionPhase::Idle
                    } else {
                        SessionPhase::Error
                    };
                    state.cleanup_error = match &result {
                        Err(SessionError::Cleanup(message)) => Some(message.clone()),
                        _ => None,
                    };
                    state.message = result.err().map(|e| e.to_string());
                    state.input_level = 0.;
                    state.playback_backlog_seconds = 0.;
                    state.playback_speed = 1.;
                    state.diagnostics = diagnostics::DiagnosticsSnapshot::default();
                }
            })
            .map_err(|_| {
                self.status.lock().unwrap_or_else(|e| e.into_inner()).phase = SessionPhase::Error;
                SessionError::Operation("Не вдалося створити потік аудіосесії.".into())
            })?;
        control.active = Some(Active { cancel, thread });
        Ok(id)
    }
    pub fn stop(&self) -> Result<(), SessionError> {
        let control = self.control.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, token)) = &control.probe {
            token.cancel();
        }
        if let Some(active) = &control.active {
            if !active.thread.is_finished() {
                self.status.lock().unwrap_or_else(|e| e.into_inner()).phase =
                    SessionPhase::Stopping;
                active.cancel.cancel();
            }
        }
        Ok(())
    }
    pub fn begin_close(&self) {
        self.control
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .closing = true;
        let _ = self.stop();
    }
    pub fn cancel_close(&self) {
        self.control
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .closing = false;
    }
    pub fn status(&self) -> SessionStatus {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub fn set_gains(&self, original: f32, translation: f32) -> Result<(), SessionError> {
        self.gains
            .set(original, translation)
            .map_err(|e| SessionError::Operation(e.to_string()))
    }
    pub async fn wait_for_stop(&self) -> Result<(), SessionError> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
        loop {
            let finished = {
                let mut control = self.control.lock().unwrap_or_else(|e| e.into_inner());
                if control
                    .active
                    .as_ref()
                    .is_some_and(|a| a.thread.is_finished())
                {
                    if let Some(active) = control.active.take() {
                        let _ = active.thread.join();
                    }
                }
                control.active.is_none() && control.probe.is_none()
            };
            if finished {
                if let Some(message) = self
                    .status
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .cleanup_error
                    .clone()
                {
                    return Err(SessionError::Cleanup(message));
                }
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(SessionError::Operation(
                    "Очищення аудіо триває довше очікуваного. Зачекайте й повторіть зупинку."
                        .into(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Fake {
        entered: Arc<AtomicUsize>,
        released: Arc<AtomicUsize>,
        fail: bool,
    }
    impl SessionRunner for Fake {
        fn run(
            &self,
            _: Settings,
            _: String,
            cancel: CancellationToken,
            reporter: Reporter,
            _: Arc<LiveGains>,
        ) -> Result<(), SessionError> {
            self.entered.fetch_add(1, Ordering::SeqCst);
            reporter.update(|status| status.phase = SessionPhase::Running);
            while !self.fail && !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(1));
            }
            self.released.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(SessionError::Operation("fake device failure".into()))
            } else {
                Ok(())
            }
        }
    }
    fn settings() -> Settings {
        Settings {
            api_key: "test".into(),
            ..Settings::default()
        }
    }
    #[tokio::test]
    async fn single_session_cancel_cleanup_and_restart() {
        let entered = Arc::new(AtomicUsize::new(0));
        let released = Arc::new(AtomicUsize::new(0));
        let manager = SessionManager::with_runner(Arc::new(Fake {
            entered: entered.clone(),
            released: released.clone(),
            fail: false,
        }));
        assert_eq!(entered.load(Ordering::SeqCst), 0);
        let first = manager.start(settings(), "source".into()).unwrap();
        assert!(manager.start(settings(), "source".into()).is_err());
        manager.stop().unwrap();
        manager.wait_for_stop().await.unwrap();
        assert_eq!(released.load(Ordering::SeqCst), 1);
        assert_eq!(manager.status().phase, SessionPhase::Idle);
        let next = manager.start(settings(), "source".into()).unwrap();
        assert!(next > first);
        manager.stop().unwrap();
        manager.wait_for_stop().await.unwrap();
    }
    #[tokio::test]
    async fn failure_cleans_and_closing_rejects_start() {
        let released = Arc::new(AtomicUsize::new(0));
        let manager = SessionManager::with_runner(Arc::new(Fake {
            entered: Arc::default(),
            released: released.clone(),
            fail: true,
        }));
        manager.start(settings(), "source".into()).unwrap();
        manager.wait_for_stop().await.unwrap();
        assert_eq!(released.load(Ordering::SeqCst), 1);
        assert_eq!(manager.status().phase, SessionPhase::Error);
        manager.begin_close();
        assert!(manager.start(settings(), "source".into()).is_err());
        manager.cancel_close();
        assert!(manager.start(settings(), "source".into()).is_ok());
        manager.wait_for_stop().await.unwrap();
    }
    #[tokio::test]
    async fn probe_excludes_streaming_and_is_cancelled_by_close() {
        let manager = SessionManager::default();
        let probe = manager.reserve_probe().unwrap();
        assert!(manager.reserve_probe().is_err());
        assert!(manager.start(settings(), "source".into()).is_err());
        manager.begin_close();
        assert!(probe.token.is_cancelled());
        drop(probe);
        manager.wait_for_stop().await.unwrap();
    }
    #[tokio::test]
    async fn cleanup_failure_blocks_exit_and_new_start() {
        struct BrokenCleanup;
        impl SessionRunner for BrokenCleanup {
            fn run(
                &self,
                _: Settings,
                _: String,
                _: CancellationToken,
                _: Reporter,
                _: Arc<LiveGains>,
            ) -> Result<(), SessionError> {
                Err(SessionError::Cleanup("native teardown failed".into()))
            }
        }
        let manager = SessionManager::with_runner(Arc::new(BrokenCleanup));
        manager.start(settings(), "source".into()).unwrap();
        assert!(matches!(
            manager.wait_for_stop().await,
            Err(SessionError::Cleanup(_))
        ));
        manager.begin_close();
        assert!(manager.wait_for_stop().await.is_err());
        manager.cancel_close();
        assert!(manager.start(settings(), "source".into()).is_err());
    }
    #[test]
    fn stale_reporter_cannot_change_new_session() {
        let status = Arc::new(Mutex::new(SessionStatus {
            session_id: 2,
            ..SessionStatus::default()
        }));
        Reporter {
            status: status.clone(),
            id: 1,
        }
        .update(|s| s.phase = SessionPhase::Running);
        assert_eq!(status.lock().unwrap().phase, SessionPhase::Idle);
    }
}
