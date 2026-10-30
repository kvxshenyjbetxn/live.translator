use super::*;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
use windows::{
    core::{implement, IUnknown, Ref, HRESULT},
    Win32::{
        Media::Audio::{
            ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
            IActivateAudioInterfaceCompletionHandler,
            IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient,
            AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
            AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
            AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
            PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
        },
        System::{
            Com::{
                CoTaskMemAlloc,
                StructuredStorage::{
                    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
                },
                BLOB,
            },
            Variant::VT_BLOB,
        },
    },
};

const FAILURE: CaptureError = CaptureError("Не вдалося захопити звук програми Windows.");
const MULTIPLE_ROOTS: CaptureError = CaptureError(
    "Програма має кілька незалежних аудіопроцесів. Закрийте зайві вікна або виберіть інше джерело.",
);
const CHANGED: CaptureError = CaptureError("Джерело змінилося. Оновіть список.");

// The PROPVARIANT owns a CoTaskMemAlloc blob and clears it exactly once on drop.
// Completion retains this immutable allocation until Windows releases its handler reference,
// including when the worker times out before an asynchronous completion arrives.
struct ActivationPayload {
    variant: PROPVARIANT,
}
// The variant and its blob are immutable after construction; only the last Arc drop clears them.
unsafe impl Send for ActivationPayload {}
unsafe impl Sync for ActivationPayload {}
impl ActivationPayload {
    fn new(root: u32) -> Result<Self, CaptureError> {
        unsafe {
            let params = AUDIOCLIENT_ACTIVATION_PARAMS {
                ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
                Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                    ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                        TargetProcessId: root,
                        ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                    },
                },
            };
            let blob = CoTaskMemAlloc(size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>())
                .cast::<AUDIOCLIENT_ACTIVATION_PARAMS>();
            if blob.is_null() {
                return Err(FAILURE);
            }
            blob.write(params);
            Ok(Self {
                variant: PROPVARIANT {
                    Anonymous: PROPVARIANT_0 {
                        Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                            vt: VT_BLOB,
                            wReserved1: 0,
                            wReserved2: 0,
                            wReserved3: 0,
                            Anonymous: PROPVARIANT_0_0_0 {
                                blob: BLOB {
                                    cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                                    pBlobData: blob.cast(),
                                },
                            },
                        }),
                    },
                },
            })
        }
    }
}

#[implement(
    IActivateAudioInterfaceCompletionHandler,
    windows::core::imp::IAgileObject
)]
struct Completion {
    signal: mpsc::Sender<()>,
    _payload: Arc<ActivationPayload>,
}
impl IActivateAudioInterfaceCompletionHandler_Impl for Completion_Impl {
    fn ActivateCompleted(
        &self,
        _: Ref<IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let _ = self.signal.send(());
        Ok(())
    }
}
impl windows::core::imp::IAgileObject_Impl for Completion_Impl {}

fn roots(
    selected: &[u32],
    parents: &[(u32, u32)],
    excluded: &HashSet<u32>,
) -> Result<Vec<u32>, CaptureError> {
    let selected: HashSet<_> = selected.iter().copied().collect();
    if selected.is_empty() || selected.iter().any(|p| excluded.contains(p)) {
        return Err(CHANGED);
    }
    let mut roots: Vec<_> = selected
        .iter()
        .copied()
        .filter(|pid| {
            !selected
                .iter()
                .any(|other| other != pid && descendants(*other, parents).contains(pid))
        })
        .collect();
    roots.sort_unstable();
    if roots
        .iter()
        .any(|root| !descendants(*root, parents).is_disjoint(excluded))
    {
        return Err(CHANGED);
    }
    Ok(roots)
}

pub(super) fn selected_root(source: &ResolvedSource) -> Result<u32, CaptureError> {
    if !source_still_valid(source)
        || source
            .processes
            .iter()
            .any(|p| identity(&p.executable).0 != source.id)
    {
        return Err(CHANGED);
    }
    let parents = process_parents()?;
    let excluded = descendants(std::process::id(), &parents);
    let selected: Vec<_> = source.processes.iter().map(|p| p.pid as u32).collect();
    let roots = roots(&selected, &parents, &excluded)?;
    // Mixing independently-clocked process trees is not yet supported.
    if roots.len() != 1 {
        return Err(MULTIPLE_ROOTS);
    }
    Ok(roots[0])
}

fn enqueue(
    queues: &mut CaptureQueues,
    meters: &AudioMeters,
    fault: &FaultSignal,
    samples: Option<&[f32]>,
    frames: usize,
) -> bool {
    if samples.is_some_and(|s| s.len() != frames.saturating_mul(2)) {
        fault.raise(2);
        return false;
    }
    if queues.network.slots() < frames {
        fault.raise(1);
        return false;
    }
    let mut peak = 0f32;
    for n in 0..frames {
        let sample = samples.map_or(0., |s| (s[n * 2] + s[n * 2 + 1]) * 0.5);
        if !sample.is_finite() {
            fault.raise(1);
            return false;
        }
        if queues.network.push(sample).is_err() {
            fault.raise(1);
            return false;
        }
        peak = peak.max(sample.abs());
    }
    meters.peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
    meters.callbacks.fetch_add(1, Ordering::Relaxed);
    true
}

pub(super) struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<(), CaptureError>>>,
}
impl Worker {
    pub(super) fn start(
        source: ResolvedSource,
        queues: CaptureQueues,
        meters: Arc<AudioMeters>,
        fault: FaultSignal,
    ) -> Result<Self, CaptureError> {
        let root = selected_root(&source)?;
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let result = run(
                root,
                source,
                queues,
                meters,
                fault.clone(),
                cancel,
                ready_tx.clone(),
            );
            if let Err(error) = result {
                let _ = ready_tx.send(Err(error));
                fault.raise(2);
            }
            result
        });
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                stop,
                thread: Some(thread),
            }),
            result => {
                stop.store(true, Ordering::Relaxed);
                let _ = thread.join();
                Err(match result {
                    Ok(Err(error)) => error,
                    _ => FAILURE,
                })
            }
        }
    }
    pub(super) fn stop(&mut self) -> Result<(), CaptureError> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().map_err(|_| FAILURE)??;
        }
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn packet_available(
    stop: &AtomicBool,
    next: impl FnOnce() -> Result<u32, CaptureError>,
) -> Result<bool, CaptureError> {
    if stop.load(Ordering::Relaxed) {
        Ok(false)
    } else {
        Ok(next()? > 0)
    }
}

fn run(
    root: u32,
    source: ResolvedSource,
    mut queues: CaptureQueues,
    meters: Arc<AudioMeters>,
    fault: FaultSignal,
    stop: Arc<AtomicBool>,
    ready: mpsc::SyncSender<Result<(), CaptureError>>,
) -> Result<(), CaptureError> {
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(|_| FAILURE)?;
        let _com = ComGuard;
        let payload = Arc::new(ActivationPayload::new(root)?);
        let (tx, rx) = mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = Completion {
            signal: tx,
            _payload: payload.clone(),
        }
        .into();
        if selected_root(&source)? != root {
            return Err(CHANGED);
        }
        let operation = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&payload.variant),
            &handler,
        )
        .map_err(|_| FAILURE)?;
        rx.recv_timeout(Duration::from_secs(10))
            .map_err(|_| FAILURE)?;
        let mut status = HRESULT(0);
        let mut object: Option<IUnknown> = None;
        operation
            .GetActivateResult(&mut status, &mut object)
            .map_err(|_| FAILURE)?;
        status.ok().map_err(|_| FAILURE)?;
        let client: IAudioClient = object.ok_or(FAILURE)?.cast().map_err(|_| FAILURE)?;
        let format = WAVEFORMATEX {
            wFormatTag: 3,
            nChannels: 2,
            nSamplesPerSec: 48000,
            nAvgBytesPerSec: 384000,
            nBlockAlign: 8,
            wBitsPerSample: 32,
            cbSize: 0,
        };
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                0,
                0,
                &format,
                None,
            )
            .map_err(|_| FAILURE)?;
        let capture: IAudioCaptureClient = client.GetService().map_err(|_| FAILURE)?;
        if selected_root(&source)? != root {
            return Err(CHANGED);
        }
        client.Start().map_err(|_| FAILURE)?;
        if ready.send(Ok(())).is_err() {
            let _ = client.Stop();
            return Err(FAILURE);
        }
        let result = (|| {
            while !stop.load(Ordering::Relaxed) {
                let mut any = false;
                while packet_available(&stop, || capture.GetNextPacketSize().map_err(|_| FAILURE))?
                {
                    any = true;
                    let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0, 0);
                    capture
                        .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                        .map_err(|_| FAILURE)?;
                    let ok = if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                        enqueue(&mut queues, &meters, &fault, None, frames as usize)
                    } else if data.is_null() || (data as usize) % std::mem::align_of::<f32>() != 0 {
                        fault.raise(2);
                        false
                    } else {
                        enqueue(
                            &mut queues,
                            &meters,
                            &fault,
                            Some(std::slice::from_raw_parts(
                                data.cast::<f32>(),
                                frames as usize * 2,
                            )),
                            frames as usize,
                        )
                    };
                    capture.ReleaseBuffer(frames).map_err(|_| FAILURE)?;
                    if !ok {
                        return Err(FAILURE);
                    }
                }
                if !any {
                    meters.callbacks.fetch_add(1, Ordering::Relaxed);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(())
        })();
        let stopped = client.Stop().map_err(|_| FAILURE);
        result.and(stopped)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deduplicates_roots_and_rejects_self_descendant() {
        let tree = [(2, 1), (3, 2), (4, 9)];
        assert_eq!(
            roots(&[1, 2, 3, 4], &tree, &HashSet::new()).unwrap(),
            vec![1, 4]
        );
        assert!(roots(&[1], &tree, &HashSet::from([3])).is_err());
    }
    #[test]
    fn pcm_silence_and_queue_faults() {
        let (network, mut n) = rtrb::RingBuffer::new(3);
        let (original, mut o) = rtrb::RingBuffer::new(3);
        let mut queues = CaptureQueues { network, original };
        let meters = AudioMeters::default();
        let fault = FaultSignal::default();
        assert!(enqueue(&mut queues, &meters, &fault, Some(&[1., -0.5]), 1));
        assert!(enqueue(&mut queues, &meters, &fault, None, 1));
        assert_eq!(n.pop().unwrap(), 0.25);
        assert!(o.pop().is_err());
        assert_eq!(n.pop().unwrap(), 0.);
        assert!(o.pop().is_err());
        assert_eq!(meters.callbacks.load(Ordering::Relaxed), 2);
        assert!(!enqueue(&mut queues, &meters, &fault, None, 4));
        assert_eq!(fault.get(), 1);
        fault.raise(2);
        assert_eq!(fault.get(), 1);
    }

    #[test]
    fn continuous_packets_stop_before_next_read() {
        let stop = AtomicBool::new(false);
        let mut reads = 0;
        assert!(packet_available(&stop, || {
            reads += 1;
            Ok(1)
        })
        .unwrap());
        stop.store(true, Ordering::Relaxed);
        assert!(!packet_available(&stop, || {
            reads += 1;
            Ok(1)
        })
        .unwrap());
        assert_eq!(reads, 1);
    }

    #[test]
    fn activation_payload_survives_timeout_until_late_handler_release() {
        let payload = Arc::new(ActivationPayload::new(42).unwrap());
        let weak = Arc::downgrade(&payload);
        unsafe {
            let value = &payload.variant.Anonymous.Anonymous;
            assert_eq!(value.vt, VT_BLOB);
            let blob = value.Anonymous.blob;
            assert_eq!(
                blob.cbSize as usize,
                size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>()
            );
            assert_eq!(
                (*blob.pBlobData.cast::<AUDIOCLIENT_ACTIVATION_PARAMS>())
                    .Anonymous
                    .ProcessLoopbackParams
                    .TargetProcessId,
                42
            );
        }
        let (tx, rx) = mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = Completion {
            signal: tx,
            _payload: payload.clone(),
        }
        .into();
        drop(payload);
        assert!(rx.recv_timeout(Duration::ZERO).is_err());
        assert!(weak.upgrade().is_some());
        drop(handler);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn activation_payload_drops_after_normal_completion() {
        let payload = Arc::new(ActivationPayload::new(7).unwrap());
        let weak = Arc::downgrade(&payload);
        let (tx, rx) = mpsc::channel();
        let handler = Completion {
            signal: tx,
            _payload: payload.clone(),
        };
        handler.signal.send(()).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(10)).is_ok());
        drop(handler);
        assert!(weak.upgrade().is_some());
        drop(payload);
        assert!(weak.upgrade().is_none());
    }
}
