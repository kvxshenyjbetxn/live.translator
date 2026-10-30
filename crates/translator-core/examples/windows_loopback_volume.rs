#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
}

#[cfg(windows)]
mod diagnostic {
    use std::{
        error::Error,
        io::{BufRead, BufReader, Write},
        mem::{size_of, ManuallyDrop},
        os::windows::io::AsRawHandle,
        process::{Child, Command, Stdio},
        sync::{mpsc, Arc},
        thread,
        time::{Duration, Instant},
    };
    use windows::{
        core::{implement, IUnknown, Interface, Ref, GUID, HRESULT},
        Win32::{
            Foundation::{CloseHandle, FILETIME, HANDLE},
            Media::Audio::*,
            System::{
                Com::{
                    CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, CoTaskMemFree,
                    CoUninitialize,
                    StructuredStorage::{
                        PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
                    },
                    BLOB, CLSCTX_ALL, COINIT_MULTITHREADED,
                },
                Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
                Variant::VT_BLOB,
            },
        },
    };
    type Result<T> = std::result::Result<T, Box<dyn Error>>;
    struct Com;
    impl Com {
        fn new() -> Result<Self> {
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            }
            Ok(Self)
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe { CoUninitialize() }
        }
    }
    fn creation(h: HANDLE) -> Result<u64> {
        unsafe {
            let (mut a, mut b, mut c, mut d) = (
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
                FILETIME::default(),
            );
            GetProcessTimes(h, &mut a, &mut b, &mut c, &mut d)?;
            Ok(((a.dwHighDateTime as u64) << 32) | a.dwLowDateTime as u64)
        }
    }
    unsafe fn string_and_free(p: windows::core::PWSTR) -> Result<String> {
        let s = p.to_string()?;
        CoTaskMemFree(Some(p.0.cast()));
        Ok(s)
    }
    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    struct Saved {
        volume: ISimpleAudioVolume,
        level: f32,
        mute: bool,
    }
    impl Drop for Saved {
        fn drop(&mut self) {
            unsafe {
                let a = self.volume.SetMasterVolume(self.level, std::ptr::null());
                let b = self.volume.SetMute(self.mute, std::ptr::null());
                eprintln!("restore volume={a:?} mute={b:?}");
            }
        }
    }
    struct Payload {
        variant: PROPVARIANT,
    }
    unsafe impl Send for Payload {}
    unsafe impl Sync for Payload {}
    impl Payload {
        fn new(pid: u32) -> Result<Self> {
            unsafe {
                let p = CoTaskMemAlloc(size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>())
                    .cast::<AUDIOCLIENT_ACTIVATION_PARAMS>();
                if p.is_null() {
                    return Err("allocation failed".into());
                }
                p.write(AUDIOCLIENT_ACTIVATION_PARAMS {
                    ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
                    Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                        ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                            TargetProcessId: pid,
                            ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                        },
                    },
                });
                Ok(Self {
                    variant: PROPVARIANT {
                        Anonymous: PROPVARIANT_0 {
                            Anonymous: ManuallyDrop::new(PROPVARIANT_0_0 {
                                vt: VT_BLOB,
                                wReserved1: 0,
                                wReserved2: 0,
                                wReserved3: 0,
                                Anonymous: PROPVARIANT_0_0_0 {
                                    blob: BLOB {
                                        cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                                        pBlobData: p.cast(),
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
        _payload: Arc<Payload>,
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
    fn format() -> WAVEFORMATEX {
        WAVEFORMATEX {
            wFormatTag: 3,
            nChannels: 2,
            nSamplesPerSec: 48000,
            nAvgBytesPerSec: 384000,
            nBlockAlign: 8,
            wBitsPerSample: 32,
            cbSize: 0,
        }
    }
    unsafe fn renderer() -> Result<()> {
        let _com = Com::new()?;
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        // Unique session GUID for this child instance; never target a shared default session.
        let guid = GUID::from_u128(
            (0x71a97c52430d42d0u128 << 64)
                | ((std::process::id() as u128) << 32)
                | (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .subsec_nanos() as u128),
        );
        client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 0, 0, &format(), Some(&guid))?;
        let control: IAudioSessionControl2 = client.GetService::<IAudioSessionControl>()?.cast()?;
        let id = string_and_free(control.GetSessionInstanceIdentifier()?)?;
        let render: IAudioRenderClient = client.GetService()?;
        let size = client.GetBufferSize()?;
        client.Start()?;
        println!("READY {id}");
        std::io::stdout().flush()?;
        let deadline = Instant::now() + Duration::from_secs(30);
        let mut written = 0u64;
        let mut report = Instant::now();
        while Instant::now() < deadline {
            let free = size.saturating_sub(client.GetCurrentPadding()?);
            if free > 0 {
                let p = render.GetBuffer(free)?.cast::<f32>();
                for n in 0..free as usize {
                    let x = ((written + n as u64) as f64 * std::f64::consts::TAU * 440.0 / 48000.0)
                        .sin() as f32
                        * 0.08;
                    p.add(n * 2).write(x);
                    p.add(n * 2 + 1).write(x);
                }
                render.ReleaseBuffer(free, 0)?;
                written += free as u64;
            }
            if report.elapsed() >= Duration::from_millis(150) {
                println!("FRAMES {written}");
                std::io::stdout().flush()?;
                report = Instant::now();
            }
            thread::sleep(Duration::from_millis(5));
        }
        client.Stop()?;
        Ok(())
    }
    unsafe fn session(pid: u32, instance: &str) -> Result<Saved> {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devices = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        let mut found = Vec::new();
        for i in 0..devices.GetCount()? {
            let manager: IAudioSessionManager2 = devices.Item(i)?.Activate(CLSCTX_ALL, None)?;
            let sessions = manager.GetSessionEnumerator()?;
            for j in 0..sessions.GetCount()? {
                let c = sessions.GetSession(j)?;
                let c2: IAudioSessionControl2 = c.cast()?;
                let mut session_pid = 0u32;
                let hr =
                    (Interface::vtable(&c2).GetProcessId)(Interface::as_raw(&c2), &mut session_pid);
                if hr.0 != 0 || session_pid != pid {
                    continue;
                }
                let id = string_and_free(c2.GetSessionInstanceIdentifier()?)?;
                if id == instance {
                    found.push(c.cast::<ISimpleAudioVolume>()?);
                }
            }
        }
        if found.len() != 1 {
            return Err(format!(
                "expected exactly one child-owned, single-process matching session; found {}",
                found.len()
            )
            .into());
        }
        let volume = found.pop().unwrap();
        let level = volume.GetMasterVolume()?;
        let mute = volume.GetMute()?.as_bool();
        Ok(Saved {
            volume,
            level,
            mute,
        })
    }
    unsafe fn capture(pid: u32) -> Result<(IAudioClient, IAudioCaptureClient)> {
        let payload = Arc::new(Payload::new(pid)?);
        let (tx, rx) = mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = Completion {
            signal: tx,
            _payload: payload.clone(),
        }
        .into();
        let op = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&payload.variant),
            &handler,
        )?;
        rx.recv_timeout(Duration::from_secs(8))?;
        let mut hr = HRESULT(0);
        let mut object: Option<IUnknown> = None;
        op.GetActivateResult(&mut hr, &mut object)?;
        hr.ok()?;
        let client: IAudioClient = object.ok_or("no capture client")?.cast()?;
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            0,
            0,
            &format(),
            None,
        )?;
        let capture = client.GetService()?;
        client.Start()?;
        Ok((client, capture))
    }
    fn progress(rx: &mpsc::Receiver<String>, last: &mut u64) {
        while let Ok(s) = rx.try_recv() {
            if let Some(n) = s.strip_prefix("FRAMES ").and_then(|x| x.parse().ok()) {
                *last = n;
            }
        }
    }
    unsafe fn measure(
        cap: &IAudioCaptureClient,
        rx: &mpsc::Receiver<String>,
        child: &mut Child,
        previous: &mut u64,
    ) -> Result<(u64, f64, f32, u64)> {
        let mut count = 0u64;
        let mut squares = 0f64;
        let mut peak = 0f32;
        let mut frames = *previous;
        let until = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < until {
            if child.try_wait()?.is_some() {
                return Err("renderer exited during stage".into());
            }
            progress(rx, &mut frames);
            while cap.GetNextPacketSize()? > 0 {
                let (mut p, mut n, mut flags) = (std::ptr::null_mut(), 0, 0);
                cap.GetBuffer(&mut p, &mut n, &mut flags, None, None)?;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 == 0 {
                    if p.is_null() {
                        cap.ReleaseBuffer(n)?;
                        return Err("null non-silent capture packet".into());
                    }
                    for &x in std::slice::from_raw_parts(p.cast::<f32>(), n as usize * 2) {
                        if !x.is_finite() {
                            cap.ReleaseBuffer(n)?;
                            return Err("nonfinite PCM".into());
                        }
                        squares += (x as f64) * (x as f64);
                        peak = peak.max(x.abs());
                    }
                }
                count += n as u64 * 2;
                cap.ReleaseBuffer(n)?;
            }
            thread::sleep(Duration::from_millis(5));
        }
        if frames <= *previous || count < 20000 {
            return Err(format!("insufficient renderer progress or capture samples: renderer_delta={} samples={count}",frames.saturating_sub(*previous)).into());
        }
        let delta = frames - *previous;
        *previous = frames;
        Ok((count, (squares / count as f64).sqrt(), peak, delta))
    }
    unsafe fn parent() -> Result<()> {
        let _com = Com::new()?;
        let mut child = ChildGuard(
            Command::new(std::env::current_exe()?)
                .arg("--renderer")
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()?,
        );
        let pid = child.0.id();
        let from_child = creation(HANDLE(child.0.as_raw_handle()))?;
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
        let opened = creation(h);
        let _ = CloseHandle(h);
        if opened? != from_child {
            return Err("child process identity changed".into());
        }
        let out = child.0.stdout.take().ok_or("missing child pipe")?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(out)
                .lines()
                .map_while(std::result::Result::ok)
            {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let ready = rx.recv_timeout(Duration::from_secs(8))?;
        let id = ready
            .strip_prefix("READY ")
            .ok_or("renderer did not report session identity")?;
        let saved = session(pid, id)?;
        println!("validated own child PID and exact unique single-process session; initial level={:.3} mute={}",saved.level,saved.mute);
        let (client, cap) = capture(pid)?;
        let mut previous = 0;
        progress(&rx, &mut previous);
        for (name, level, mute) in [
            ("baseline", 1.0, false),
            ("half", 0.5, false),
            ("zero", 0.0, false),
            ("muted", 1.0, true),
            ("unmuted", 1.0, false),
        ] {
            saved.volume.SetMute(mute, std::ptr::null())?;
            saved.volume.SetMasterVolume(level, std::ptr::null())?;
            thread::sleep(Duration::from_millis(350));
            while cap.GetNextPacketSize()? > 0 {
                let (mut data, mut n, mut flags) = (std::ptr::null_mut(), 0, 0);
                cap.GetBuffer(&mut data, &mut n, &mut flags, None, None)?;
                cap.ReleaseBuffer(n)?;
            }
            progress(&rx, &mut previous);
            let applied = saved.volume.GetMasterVolume()?;
            let applied_mute = saved.volume.GetMute()?.as_bool();
            if (applied - level).abs() > 0.001 || applied_mute != mute {
                return Err("session applied state mismatch".into());
            }
            let (samples, rms, peak, rendered) = measure(&cap, &rx, &mut child.0, &mut previous)?;
            println!("{name}: applied={applied:.3} mute={applied_mute} samples={samples} rms={rms:.6} peak={peak:.6} rendered_frames_delta={rendered}");
            if name == "baseline" && rms < 0.001 {
                return Err("baseline synthetic tone is not present in capture".into());
            }
        }
        client.Stop()?;
        println!("Capture PCM only; audible output attenuation is not independently measured.");
        Ok(())
    }
    pub fn main() {
        let renderer_mode = std::env::args().nth(1).as_deref() == Some("--renderer");
        let result = unsafe {
            if renderer_mode {
                renderer()
            } else {
                parent()
            }
        };
        if let Err(e) = result {
            eprintln!("diagnostic error: {e:?}");
            std::process::exit(1);
        }
    }
}
#[cfg(windows)]
fn main() {
    diagnostic::main()
}
