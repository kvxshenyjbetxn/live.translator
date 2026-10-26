use crate::provider::{gemini::GeminiSender, ProviderError};
use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;

pub(super) trait Transport: Send + 'static {
    fn audio(&mut self, pcm: &[i16]) -> impl Future<Output = Result<(), ProviderError>> + Send;
    fn ping(&mut self, payload: Vec<u8>) -> impl Future<Output = Result<(), ProviderError>> + Send;
}
impl Transport for GeminiSender {
    async fn audio(&mut self, pcm: &[i16]) -> Result<(), ProviderError> {
        self.send_audio(pcm).await
    }
    async fn ping(&mut self, payload: Vec<u8>) -> Result<(), ProviderError> {
        self.send_ping(payload).await
    }
}
#[derive(Default)]
struct Probe {
    sequence: u64,
    pending: Option<(Vec<u8>, Instant, bool)>,
}
impl Probe {
    fn begin(&mut self, now: Instant) -> Option<Vec<u8>> {
        if self
            .pending
            .as_ref()
            .is_some_and(|(_, time, _)| now.duration_since(*time) < Duration::from_secs(5))
        {
            return None;
        }
        self.sequence += 1;
        let token = self.sequence.to_be_bytes().to_vec();
        self.pending = Some((token.clone(), now, false));
        Some(token)
    }
    fn written(&mut self, now: Instant) {
        if let Some((_, time, written)) = &mut self.pending {
            // A slow flush confounds network RTT with a local write stall.
            *written = now.duration_since(*time) <= Duration::from_millis(100);
        }
    }
    fn pong(&mut self, token: &[u8], now: Instant) -> Option<Duration> {
        let (expected, time, written) = self.pending.as_ref()?;
        if expected != token || !written || now.duration_since(*time) >= Duration::from_secs(5) {
            return None;
        }
        let rtt = now.duration_since(*time);
        self.pending = None;
        Some(rtt)
    }
}
#[derive(Clone, Default)]
pub(super) struct SenderStats {
    pub pending_samples: usize,
    pub sent_samples: u64,
    pub last_send: Option<(Instant, Duration)>,
}
#[derive(Default)]
struct Shared {
    stats: SenderStats,
    probe: Probe,
}
pub(super) struct AudioSender {
    input: mpsc::Sender<Vec<i16>>,
    shared: Arc<Mutex<Shared>>,
    task: JoinHandle<Result<(), ProviderError>>,
}
impl AudioSender {
    pub fn start(transport: impl Transport, cancel: CancellationToken) -> Self {
        let (input, receiver) = mpsc::channel(8);
        let shared = Arc::new(Mutex::new(Shared::default()));
        let task = tokio::spawn(Self::run(transport, receiver, shared.clone(), cancel));
        Self {
            input,
            shared,
            task,
        }
    }
    pub fn enqueue(&mut self, pcm: Vec<i16>) -> Result<(), ()> {
        let mut shared = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let count = pcm.len();
        self.input.try_send(pcm).map_err(|_| ())?;
        shared.stats.pending_samples += count;
        Ok(())
    }
    pub fn snapshot(&self) -> SenderStats {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .stats
            .clone()
    }
    pub fn pong(&self, payload: &[u8], now: Instant) -> Option<Duration> {
        self.shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .probe
            .pong(payload, now)
    }
    pub fn finished(&self) -> bool {
        self.task.is_finished()
    }
    pub async fn finish(self) -> Result<(), ProviderError> {
        self.task.await.unwrap_or(Err(ProviderError::Network))
    }
    async fn run(
        mut transport: impl Transport,
        mut input: mpsc::Receiver<Vec<i16>>,
        shared: Arc<Mutex<Shared>>,
        cancel: CancellationToken,
    ) -> Result<(), ProviderError> {
        let mut probe_tick = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(4),
            Duration::from_secs(4),
        );
        probe_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(()),
                pcm = input.recv() => {
                    let Some(pcm) = pcm else { return Ok(()); };
                    let started = Instant::now();
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return Ok(()),
                        result = transport.audio(&pcm) => result?,
                    }
                    let now = Instant::now();
                    let mut state = shared.lock().unwrap_or_else(|e| e.into_inner());
                    state.stats.pending_samples -= pcm.len();
                    state.stats.sent_samples += pcm.len() as u64;
                    state.stats.last_send = Some((now, now.duration_since(started)));
                }
                _ = probe_tick.tick() => {
                    let payload = shared.lock().unwrap_or_else(|e| e.into_inner()).probe.begin(Instant::now());
                    if let Some(payload) = payload {
                        tokio::select! {
                            biased;
                            _ = cancel.cancelled() => return Ok(()),
                            result = transport.ping(payload) => result?,
                        }
                        shared.lock().unwrap_or_else(|e| e.into_inner()).probe.written(Instant::now());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Blocked {
        calls: Arc<AtomicUsize>,
        gate: Arc<tokio::sync::Semaphore>,
    }
    impl Transport for Blocked {
        async fn audio(&mut self, _: &[i16]) -> Result<(), ProviderError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let _permit = self.gate.acquire().await.unwrap();
            Ok(())
        }
        async fn ping(&mut self, _: Vec<u8>) -> Result<(), ProviderError> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn blocked_writes_keep_caller_responsive_and_cancel_without_leaking() {
        let calls = Arc::new(AtomicUsize::new(0));
        let cancel = CancellationToken::new();
        let mut sender = AudioSender::start(
            Blocked {
                calls: calls.clone(),
                gate: Arc::new(tokio::sync::Semaphore::new(0)),
            },
            cancel.clone(),
        );
        sender.enqueue(vec![0; 1600]).unwrap();
        tokio::task::yield_now().await;
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            sender.snapshot().pending_samples,
            1600,
            "in-flight audio remains in queue metric"
        );
        // This task represents the owning worker, which must keep servicing playback/receive ticks.
        for _ in 0..3 {
            tokio::task::yield_now().await;
            assert!(!sender.finished());
        }
        for _ in 0..8 {
            sender.enqueue(vec![0; 1600]).unwrap();
        }
        assert!(
            sender.enqueue(vec![0; 1600]).is_err(),
            "bounded queue must fail instead of dropping audio"
        );
        assert_eq!(sender.snapshot().pending_samples, 14400);
        cancel.cancel();
        tokio::time::timeout(Duration::from_millis(100), sender.finish())
            .await
            .unwrap()
            .unwrap();
    }
    #[test]
    fn probe_matches_only_fresh_written_token_once() {
        let mut probe = Probe::default();
        let start = Instant::now();
        let token = probe.begin(start).unwrap();
        assert!(probe.begin(start).is_none());
        assert!(probe.pong(b"wrong", start).is_none());
        assert!(
            probe.pong(&token, start).is_none(),
            "unflushed send cannot yield valid RTT"
        );
        probe.written(start + Duration::from_millis(2));
        assert_eq!(
            probe.pong(&token, start + Duration::from_millis(90)),
            Some(Duration::from_millis(90))
        );
        assert!(probe
            .pong(&token, start + Duration::from_millis(91))
            .is_none());
        let token = probe.begin(start + Duration::from_secs(5)).unwrap();
        probe.written(start + Duration::from_secs(5));
        assert!(probe
            .pong(&token, start + Duration::from_secs(11))
            .is_none());
        assert!(probe.begin(start + Duration::from_secs(11)).is_some());
    }
    #[tokio::test(start_paused = true)]
    async fn absent_pong_never_stops_audio_or_accumulates_probes() {
        struct Counting {
            audio: Arc<AtomicUsize>,
            probes: Arc<AtomicUsize>,
        }
        impl Transport for Counting {
            async fn audio(&mut self, _: &[i16]) -> Result<(), ProviderError> {
                self.audio.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
            async fn ping(&mut self, _: Vec<u8>) -> Result<(), ProviderError> {
                self.probes.fetch_add(1, Ordering::Relaxed);
                Ok(())
            }
        }
        let calls = Arc::new(AtomicUsize::new(0));
        let probes = Arc::new(AtomicUsize::new(0));
        let cancel = CancellationToken::new();
        let mut sender = AudioSender::start(
            Counting {
                audio: calls.clone(),
                probes: probes.clone(),
            },
            cancel.clone(),
        );
        tokio::task::yield_now().await;
        for _ in 0..16 {
            tokio::time::advance(Duration::from_secs(1)).await;
            sender.enqueue(vec![0; 1600]).unwrap();
            tokio::task::yield_now().await;
        }
        assert_eq!(calls.load(Ordering::Relaxed), 16);
        assert!(
            probes.load(Ordering::Relaxed) <= 3,
            "missing replies must not queue probes"
        );
        assert!(probes.load(Ordering::Relaxed) >= 1);
        cancel.cancel();
        sender.finish().await.unwrap();
    }
    #[test]
    fn slowly_flushed_probe_cannot_be_mislabelled_network_latency() {
        let mut probe = Probe::default();
        let now = Instant::now();
        let token = probe.begin(now).unwrap();
        probe.written(now + Duration::from_millis(200));
        assert!(probe
            .pong(&token, now + Duration::from_millis(250))
            .is_none());
    }
    #[tokio::test]
    async fn completed_audio_records_duration_and_counts_only_successful_samples() {
        let cancel = CancellationToken::new();
        let mut sender = AudioSender::start(
            Blocked {
                calls: Arc::new(AtomicUsize::new(0)),
                gate: Arc::new(tokio::sync::Semaphore::new(10)),
            },
            cancel.clone(),
        );
        sender.enqueue(vec![0; 1600]).unwrap();
        tokio::task::yield_now().await;
        let stats = sender.snapshot();
        assert_eq!(stats.sent_samples, 1600);
        assert_eq!(stats.pending_samples, 0);
        assert!(stats.last_send.is_some());
        cancel.cancel();
        sender.finish().await.unwrap();
    }
}
