use super::sender::SenderStats;
use super::*;
use super::{diagnostics::Diagnostics, sender::AudioSender};
use crate::{
    audio::{buffer::PlaybackBuffer, resample::Resampler, AudioEngine, AudioMeters, FaultSignal},
    capture::{resolve_source, source_still_valid, CaptureHandle},
    provider::{
        gemini::{GeminiClient, GeminiReceiver},
        ProviderError, TranslationConfig, TranslationEvent,
    },
};
use std::{collections::VecDeque, sync::atomic::Ordering, time::Instant};
use tokio::task::{JoinError, JoinHandle};
pub struct LiveRunner;
impl SessionRunner for LiveRunner {
    fn run(
        &self,
        settings: Settings,
        source_id: String,
        cancel: CancellationToken,
        reporter: Reporter,
        gains: Arc<LiveGains>,
    ) -> Result<(), SessionError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| SessionError::Operation("Не вдалося запустити мережевий потік.".into()))?;
        runtime.block_on(run(settings, source_id, cancel, reporter, gains))
    }
}
enum Wake<T> {
    Cancelled,
    Tick,
    Event(T),
}
async fn next_io<T>(
    event: impl std::future::Future<Output = T>,
    tick: &mut tokio::time::Interval,
    cancel: &CancellationToken,
) -> Wake<T> {
    tokio::select! {
        _ = cancel.cancelled() => Wake::Cancelled,
        result = event => Wake::Event(result),
        _ = tick.tick() => Wake::Tick,
    }
}
fn error(error: impl std::fmt::Display) -> SessionError {
    SessionError::Operation(error.to_string())
}

// Gemini Live connections are limited to about 10 minutes, so the worker replaces the
// connection (planned on `goAway`, or after a recoverable loss) while capture and playback keep running.
const RECONNECT_ATTEMPTS: u32 = 5;
const STABLE_AFTER: Duration = Duration::from_secs(30);
const OLD_TAIL: Duration = Duration::from_secs(5);
const BUFFER_CHUNKS: usize = 50;

struct Link {
    sender: AudioSender,
    cancel: CancellationToken,
    receiver: GeminiReceiver,
    since: Instant,
}
struct Connecting {
    task: JoinHandle<Result<GeminiClient, ProviderError>>,
    cancel: CancellationToken,
    /// A handover keeps the old connection until the new one is ready; a reconnect has no old one.
    planned: bool,
}
enum Source {
    Primary(Result<TranslationEvent, ProviderError>),
    Old(Result<TranslationEvent, ProviderError>),
    Connected(Box<Result<Result<GeminiClient, ProviderError>, JoinError>>),
    Retry,
}
struct Connection {
    api_key: String,
    language: String,
    cancel: CancellationToken,
    link: Option<Link>,
    /// Receiver of a replaced connection, read a little longer so the translation tail is not cut.
    old: Option<(GeminiReceiver, Instant)>,
    connecting: Option<Connecting>,
    retry_at: Option<tokio::time::Instant>,
    attempts: u32,
    sent_base: u64,
    /// Chunks waiting for a sender (reconnecting or full queue); the oldest are dropped when full.
    buffer: VecDeque<Vec<i16>>,
}
impl Connection {
    fn new(api_key: String, language: String, cancel: CancellationToken) -> Self {
        Self {
            api_key,
            language,
            cancel,
            link: None,
            old: None,
            connecting: None,
            retry_at: None,
            attempts: 0,
            sent_base: 0,
            buffer: VecDeque::new(),
        }
    }
    fn new_link(&self, client: GeminiClient) -> Link {
        let (transport, receiver) = client.split();
        let cancel = self.cancel.child_token();
        Link {
            sender: AudioSender::start(transport, cancel.clone()),
            cancel,
            receiver,
            since: Instant::now(),
        }
    }
    /// Installs a connected client; a previous link is retired and its receiver kept for the tail.
    async fn install(&mut self, client: GeminiClient) {
        let link = self.new_link(client);
        if let Some(previous) = self.link.replace(link) {
            let Link {
                sender,
                cancel,
                receiver,
                ..
            } = previous;
            self.sent_base += sender.snapshot().sent_samples;
            cancel.cancel();
            let _ = sender.finish().await;
            self.old = Some((receiver, Instant::now() + OLD_TAIL));
        }
        self.flush();
    }
    fn spawn_connect(&mut self, planned: bool) {
        let cancel = self.cancel.child_token();
        let config = TranslationConfig {
            api_key: self.api_key.clone(),
            target_language: self.language.clone(),
        };
        let task = tokio::spawn(GeminiClient::connect(config, cancel.clone()));
        self.connecting = Some(Connecting {
            task,
            cancel,
            planned,
        });
    }
    fn schedule_retry(&mut self, cause: ProviderError) -> Result<(), ProviderError> {
        if self.attempts >= RECONNECT_ATTEMPTS {
            return Err(cause);
        }
        // 0.5 s, 1 s, 2 s, 4 s, 8 s.
        let delay = Duration::from_millis(500 << self.attempts);
        self.retry_at = Some(tokio::time::Instant::now() + delay);
        self.attempts += 1;
        Ok(())
    }
    fn sender_finished(&self) -> bool {
        self.link
            .as_ref()
            .is_some_and(|link| link.sender.finished())
    }
    /// Handles loss of the current connection. `None` takes the cause from the finished sender task.
    async fn lose(&mut self, cause: Option<ProviderError>) -> Result<(), ProviderError> {
        let mut cause = cause;
        if let Some(link) = self.link.take() {
            if link.since.elapsed() >= STABLE_AFTER {
                self.attempts = 0;
            }
            self.sent_base += link.sender.snapshot().sent_samples;
            link.cancel.cancel();
            let result = link.sender.finish().await;
            if cause.is_none() {
                cause = Some(result.err().unwrap_or(ProviderError::Closed));
            }
        }
        let cause = cause.unwrap_or(ProviderError::Closed);
        if !cause.is_recoverable() {
            return Err(cause);
        }
        if let Some(connecting) = &mut self.connecting {
            // A handover already in flight now serves as the reconnect.
            connecting.planned = false;
            return Ok(());
        }
        self.schedule_retry(cause)
    }
    async fn connected(
        &mut self,
        joined: Result<Result<GeminiClient, ProviderError>, JoinError>,
    ) -> Result<(), ProviderError> {
        let planned = self.connecting.take().is_some_and(|c| c.planned);
        match joined {
            Ok(Ok(client)) => self.install(client).await,
            Ok(Err(ProviderError::Cancelled)) => {}
            // A failed handover is harmless: the old connection keeps working until it closes.
            Ok(Err(_)) if planned => {}
            Ok(Err(cause)) if cause.is_recoverable() => self.schedule_retry(cause)?,
            Ok(Err(cause)) => return Err(cause),
            Err(_) if planned => {}
            Err(_) => self.schedule_retry(ProviderError::Network)?,
        }
        Ok(())
    }
    fn push(&mut self, chunk: Vec<i16>) {
        self.buffer.push_back(chunk);
        if self.buffer.len() > BUFFER_CHUNKS {
            self.buffer.pop_front();
        }
        self.flush();
    }
    /// Moves buffered chunks into the sender as its queue allows; the rest waits for the next call.
    fn flush(&mut self) {
        let Some(link) = &mut self.link else { return };
        while let Some(chunk) = self.buffer.front() {
            if link.sender.enqueue(chunk.clone()).is_err() {
                break;
            }
            self.buffer.pop_front();
        }
    }
    fn pong(&self, payload: &[u8], now: Instant) -> Option<Duration> {
        self.link.as_ref()?.sender.pong(payload, now)
    }
    fn expire_old(&mut self) {
        if self
            .old
            .as_ref()
            .is_some_and(|(_, until)| Instant::now() >= *until)
        {
            self.old = None;
        }
    }
    /// Cumulative over all connections; buffered chunks count as pending.
    fn stats(&self) -> SenderStats {
        let mut stats = self
            .link
            .as_ref()
            .map(|link| link.sender.snapshot())
            .unwrap_or_default();
        stats.sent_samples += self.sent_base;
        stats.pending_samples += self.buffer.iter().map(Vec::len).sum::<usize>();
        stats
    }
    async fn next_source(&mut self) -> Source {
        let Self {
            link,
            old,
            connecting,
            retry_at,
            ..
        } = self;
        tokio::select! {
            result = async { match link { Some(link) => link.receiver.next_event().await, None => std::future::pending().await } } => Source::Primary(result),
            result = async { match old { Some((receiver, _)) => receiver.next_event().await, None => std::future::pending().await } } => Source::Old(result),
            joined = async { match connecting { Some(connecting) => (&mut connecting.task).await, None => std::future::pending().await } } => Source::Connected(Box::new(joined)),
            _ = async { match retry_at { Some(at) => tokio::time::sleep_until(*at).await, None => std::future::pending().await } } => Source::Retry,
        }
    }
    /// Stops everything; returns the result of the current sender.
    async fn shutdown(&mut self) -> Result<(), ProviderError> {
        if let Some(connecting) = self.connecting.take() {
            connecting.cancel.cancel();
            connecting.task.abort();
        }
        self.old = None;
        match self.link.take() {
            Some(link) => {
                link.cancel.cancel();
                link.sender.finish().await
            }
            None => Ok(()),
        }
    }
}
async fn run(
    settings: Settings,
    source_id: String,
    cancel: CancellationToken,
    reporter: Reporter,
    gains: Arc<LiveGains>,
) -> Result<(), SessionError> {
    if cancel.is_cancelled() {
        return Ok(());
    }
    // Resolve immediately; there is no artificial startup countdown.
    let source = resolve_source(&source_id).map_err(error)?;
    if cancel.is_cancelled() {
        return Ok(());
    }
    let api_key: String = settings.api_key.trim().into();
    let language = settings.target_language;
    let client = match GeminiClient::connect(
        TranslationConfig {
            api_key: api_key.clone(),
            target_language: language.clone(),
        },
        cancel.clone(),
    )
    .await
    {
        Ok(client) => client,
        Err(ProviderError::Cancelled) => return Ok(()),
        Err(failure) => return Err(error(failure)),
    };
    if cancel.is_cancelled() {
        return Ok(());
    }
    let meters = Arc::new(AudioMeters::default());
    let fault = FaultSignal::default();
    let mut capture = CaptureHandle::prepare(source.clone()).map_err(error)?;
    let rate = capture.rate();
    let mut engine =
        AudioEngine::prepare(rate, gains, meters.clone(), fault.clone()).map_err(error)?;
    if cancel.is_cancelled() {
        return Ok(());
    }
    capture
        .start(
            engine.queues.take().expect("capture queues owned once"),
            meters.clone(),
            fault.clone(),
        )
        .map_err(error)?;
    let mut resampler = Resampler::new(rate).map_err(error)?;
    let started = Instant::now();
    let mut last_health = Instant::now();
    let mut last_status = Instant::now();
    let mut last_callback = Instant::now();
    let mut previous_callbacks = 0;
    let mut conn = Connection::new(api_key, language, cancel.clone());
    conn.link = Some(conn.new_link(client));
    let mut diagnostics = Diagnostics::default();
    let mut last_send_time = None;
    let mut received = 0u64;
    let mut chunk = Vec::with_capacity(1600);
    let mut playback = PlaybackBuffer::default();
    let mut tick = tokio::time::interval(Duration::from_millis(10));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    reporter.update(|s| s.phase = SessionPhase::Running);
    let result: Result<(), SessionError> = async {
        loop {
            if cancel.is_cancelled() { break; }
            if conn.sender_finished() { conn.lose(None).await.map_err(error)?; }
            if fault.get() != 0 { return Err(SessionError::Operation(match fault.get() {
                1 => "Черга вхідного аудіо переповнилася. Перевірте мережу й повторіть запуск.",
                2 => "Формат вхідного аудіо змінився. Запустіть переклад знову.",
                _ => "Аудіопристрій недоступний. Перевірте аудіовихід і повторіть запуск.",
            }.into())); }
            conn.expire_old();
            let mut event = None;
            match next_io(conn.next_source(), &mut tick, &cancel).await {
                Wake::Cancelled => break,
                Wake::Event(Source::Primary(Ok(next))) => event = Some((next, true)),
                Wake::Event(Source::Primary(Err(ProviderError::Cancelled))) => break,
                Wake::Event(Source::Primary(Err(failure))) => conn.lose(Some(failure)).await.map_err(error)?,
                Wake::Event(Source::Old(Ok(next))) => event = Some((next, false)),
                // The old connection ending is expected; its errors are ignored.
                Wake::Event(Source::Old(Err(_))) => conn.old = None,
                Wake::Event(Source::Connected(joined)) => conn.connected(*joined).await.map_err(error)?,
                Wake::Event(Source::Retry) => {
                    conn.retry_at = None;
                    conn.spawn_connect(false);
                }
                Wake::Tick => {
                    for _ in 0..rate / 5 {
                        let Ok(sample) = engine.input.pop() else { break; };
                        if let Some(sample) = resampler.push(sample) {
                            chunk.push(sample);
                            if chunk.len() == 1600 {
                                conn.push(std::mem::replace(&mut chunk, Vec::with_capacity(1600)));
                            }
                        }
                    }
                    conn.flush();
                }
            }
            if let Some((event, primary)) = event {
                match event {
                    TranslationEvent::Audio(pcm) => {
                        let count = pcm.len();
                        playback.receive(pcm, 120000 - engine.output.slots()).map_err(error)?;
                        received += count as u64;
                    }
                    TranslationEvent::Ready => {}
                    TranslationEvent::InputTranscription(text) => {
                        reporter.update(|s| s.transcripts.extend(true, &text));
                    }
                    TranslationEvent::OutputTranscription(text) => {
                        reporter.update(|s| s.transcripts.extend(false, &text));
                    }
                    TranslationEvent::Pong(payload) if primary => {
                        let now = Instant::now();
                        if let Some(rtt) = conn.pong(&payload, now) {
                            diagnostics.record_rtt(now.duration_since(started), rtt);
                        }
                    }
                    TranslationEvent::GoAway if primary => {
                        // One handover at a time; a repeated goAway while connecting is ignored.
                        if conn.connecting.is_none() && conn.link.is_some() {
                            conn.spawn_connect(true);
                        }
                    }
                    TranslationEvent::Pong(_) | TranslationEvent::GoAway => {}
                }
            }
            playback.pump(&mut engine.output);
            let backlog = engine.backlog() + playback.waiting() as f64 / 24000.;
            playback.observe(backlog, started.elapsed()).map_err(error)?;
            let speed = engine.set_backlog(backlog);
            let callbacks = meters.callbacks.load(Ordering::Relaxed);
            if callbacks != previous_callbacks { last_callback = Instant::now(); previous_callbacks = callbacks; }
            if last_callback.elapsed() > Duration::from_secs(3) {
                #[cfg(target_os = "windows")]
                let message = "Аудіо не надходить. Перевірте джерело та аудіопристрій Windows.";
                #[cfg(not(target_os = "windows"))]
                let message = "Аудіо не надходить. Перевірте дозвіл «Запис системного аудіо» macOS і джерело.";
                return Err(SessionError::Operation(message.into()));
            }
            if last_health.elapsed() >= Duration::from_secs(1) {
                if !source_still_valid(&source) { return Err(SessionError::Operation("Аудіопроцес програми завершився або перезапустився. Оновіть список і запустіть переклад знову.".into())); }
                if !engine.output_is_current() { return Err(SessionError::Operation("Аудіовихід змінився. Запустіть переклад знову з новим пристроєм.".into())); }
                last_health = Instant::now();
            }
            if last_status.elapsed() >= Duration::from_millis(100) {
                let stats = conn.stats();
                if let Some((time, duration)) = stats.last_send {
                    if last_send_time != Some(time) {
                        diagnostics.record_send(time.duration_since(started), duration);
                        last_send_time = Some(time);
                    }
                }
                let sending = engine.input.slots() as f64 / rate as f64
                    + (chunk.len() + stats.pending_samples) as f64 / 16000.;
                diagnostics.observe(started.elapsed(), sending, backlog);
                let snapshot = diagnostics.snapshot(started.elapsed());
                let peak = f32::from_bits(meters.peak.swap(0, Ordering::Relaxed)).clamp(0., 1.);
                reporter.update(|s| {
                    s.elapsed_seconds = started.elapsed().as_secs(); s.input_level = peak;
                    s.sent_audio_seconds = stats.sent_samples as f64 / 16000.; s.received_audio_seconds = received as f64 / 24000.;
                    s.playback_backlog_seconds = backlog; s.playback_speed = speed;
                    s.diagnostics = snapshot;
                });
                last_status = Instant::now();
            }
        }
        Ok(())
    }.await;
    let send_result = conn.shutdown().await;
    // Drop local mixed playback before removing source mute; never play a stale tail over original.
    drop(engine);
    let cleanup = capture
        .stop()
        .map_err(|failure| SessionError::Cleanup(failure.to_string()));
    drop(conn);
    cleanup?;
    match send_result {
        Err(ProviderError::Cancelled) | Ok(()) => result,
        Err(failure) => Err(error(failure)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn source_resolution_has_no_artificial_delay() {
        let manager = SessionManager::default();
        let reporter = Reporter {
            status: manager.status.clone(),
            id: 0,
        };
        let began = tokio::time::Instant::now();
        assert!(run(
            Settings::default(),
            "missing-source".into(),
            CancellationToken::new(),
            reporter,
            manager.gains.clone()
        )
        .await
        .is_err());
        assert!(began.elapsed() < Duration::from_secs(1));
    }
    #[tokio::test(start_paused = true)]
    async fn cancelling_startup_prevents_source_resolution() {
        let manager = SessionManager::default();
        let reporter = Reporter {
            status: manager.status.clone(),
            id: 0,
        };
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(run(
            Settings::default(),
            "missing-source".into(),
            cancel,
            reporter,
            manager.gains.clone()
        )
        .await
        .is_ok());
    }
    #[tokio::test(start_paused = true)]
    async fn repeated_six_second_packets_cannot_hide_lag_in_transport() {
        let mut playback = PlaybackBuffer::default();
        let (mut output, mut consumed) = rtrb::RingBuffer::new(120000);
        let started = tokio::time::Instant::now();
        let mut next_packet = started;
        let mut tick = tokio::time::interval(Duration::from_millis(100));
        let cancel = CancellationToken::new();
        let mut bounded_failure = false;
        while started.elapsed() < Duration::from_secs(240) {
            let incoming = async {
                tokio::time::sleep_until(next_packet).await;
                Ok::<_, ProviderError>(TranslationEvent::Audio(vec![0; 144000]))
            };
            match next_io(incoming, &mut tick, &cancel).await {
                Wake::Event(Ok(TranslationEvent::Audio(pcm))) => {
                    next_packet += Duration::from_secs(5);
                    if playback.receive(pcm, 120000 - output.slots()).is_err() {
                        bounded_failure = true;
                        break;
                    }
                }
                Wake::Tick => {
                    for _ in 0..2688 {
                        let _ = consumed.pop();
                    }
                } // max 1.12x speed, 100ms
                _ => panic!("unexpected event"),
            }
            playback.pump(&mut output);
            let backlog = (120000 - output.slots() + playback.waiting()) as f64 / 24000.;
            if playback.observe(backlog, started.elapsed()).is_err() {
                bounded_failure = true;
                break;
            }
        }
        assert!(
            bounded_failure,
            "overproduction must not build an unmeasured transport backlog forever"
        );
    }
}
