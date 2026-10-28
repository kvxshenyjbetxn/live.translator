use super::*;
use super::{diagnostics::Diagnostics, sender::AudioSender};
use crate::{
    audio::{buffer::PlaybackBuffer, resample::Resampler, AudioEngine, AudioMeters, FaultSignal},
    capture::{resolve_source, source_still_valid, CaptureHandle},
    provider::{gemini::GeminiClient, ProviderError, TranslationConfig, TranslationEvent},
};
use std::{sync::atomic::Ordering, time::Instant};
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
enum Wake {
    Cancelled,
    Tick,
    Event(Result<TranslationEvent, ProviderError>),
}
async fn next_io(
    event: impl std::future::Future<Output = Result<TranslationEvent, ProviderError>>,
    tick: &mut tokio::time::Interval,
    cancel: &CancellationToken,
) -> Wake {
    tokio::select! {
        _ = cancel.cancelled() => Wake::Cancelled,
        result = event => Wake::Event(result),
        _ = tick.tick() => Wake::Tick,
    }
}
fn error(error: impl std::fmt::Display) -> SessionError {
    SessionError::Operation(error.to_string())
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
    let client = match GeminiClient::connect(
        TranslationConfig {
            api_key: settings.api_key.trim().into(),
            target_language: settings.target_language,
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
    let (transport, mut receiver) = client.split();
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
    let sender_cancel = cancel.child_token();
    let mut sender = AudioSender::start(transport, sender_cancel.clone());
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
            if sender.finished() { return Err(SessionError::Operation("Надсилання аудіо завершилося.".into())); }
            if fault.get() != 0 { return Err(SessionError::Operation(match fault.get() {
                1 => "Черга вхідного аудіо переповнилася. Перевірте мережу й повторіть запуск.",
                2 => "Формат вхідного аудіо змінився. Запустіть переклад знову.",
                _ => "Аудіопристрій недоступний. Перевірте аудіовихід і повторіть запуск.",
            }.into())); }
            match next_io(receiver.next_event(), &mut tick, &cancel).await {
                Wake::Cancelled => break,
                Wake::Event(event) => {
                    match event {
                        Ok(TranslationEvent::Audio(pcm)) => {
                            let count = pcm.len();
                            playback.receive(pcm, 120000 - engine.output.slots()).map_err(error)?;
                            received += count as u64;
                        }
                        Ok(TranslationEvent::Ready) => {}
                        Ok(TranslationEvent::InputTranscription(text)) => {
                            reporter.update(|s| s.transcripts.extend(true, &text));
                        }
                        Ok(TranslationEvent::OutputTranscription(text)) => {
                            reporter.update(|s| s.transcripts.extend(false, &text));
                        }
                        Ok(TranslationEvent::Pong(payload)) => {
                            let now = Instant::now();
                            if let Some(rtt) = sender.pong(&payload, now) {
                                diagnostics.record_rtt(now.duration_since(started), rtt);
                            }
                        }
                        Err(ProviderError::Cancelled) => break,
                        Err(failure) => return Err(error(failure)),
                    }
                }
                Wake::Tick => {
                    for _ in 0..rate / 5 {
                        let Ok(sample) = engine.input.pop() else { break; };
                        if let Some(sample) = resampler.push(sample) {
                            chunk.push(sample);
                            if chunk.len() == 1600 {
                                sender.enqueue(std::mem::replace(&mut chunk, Vec::with_capacity(1600)))
                                    .map_err(|_| SessionError::Operation("Черга надсилання аудіо переповнилася. З’єднання не встигає передавати звук; повторіть запуск.".into()))?;
                            }
                        }
                    }
                }
            }
            playback.pump(&mut engine.output);
            let backlog = engine.backlog() + playback.waiting() as f64 / 24000.;
            playback.observe(backlog, started.elapsed()).map_err(error)?;
            let speed = engine.set_backlog(backlog);
            let callbacks = meters.callbacks.load(Ordering::Relaxed);
            if callbacks != previous_callbacks { last_callback = Instant::now(); previous_callbacks = callbacks; }
            if last_callback.elapsed() > Duration::from_secs(3) { return Err(SessionError::Operation("Аудіо не надходить. Перевірте дозвіл «Запис системного аудіо» macOS і джерело.".into())); }
            if last_health.elapsed() >= Duration::from_secs(1) {
                if !source_still_valid(&source) { return Err(SessionError::Operation("Аудіопроцес програми завершився або перезапустився. Оновіть список і запустіть переклад знову.".into())); }
                if !engine.output_is_current() { return Err(SessionError::Operation("Аудіовихід змінився. Запустіть переклад знову з новим пристроєм.".into())); }
                last_health = Instant::now();
            }
            if last_status.elapsed() >= Duration::from_millis(100) {
                let stats = sender.snapshot();
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
    sender_cancel.cancel();
    let send_result = sender.finish().await;
    // Drop local mixed playback before removing source mute; never play a stale tail over original.
    drop(engine);
    let cleanup = capture
        .stop()
        .map_err(|failure| SessionError::Cleanup(failure.to_string()));
    drop(receiver);
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
                Ok(TranslationEvent::Audio(vec![0; 144000]))
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
