//! Throwaway feasibility spike. macOS 14.2+, no microphone/network/audio files.
#[cfg(not(target_os = "macos"))]
compile_error!("This experimental capture backend is macOS-only, not the product.");

use anyhow::{bail, ensure, Context, Result};
use objc2::{runtime::AnyObject, AnyThread};
use objc2_core_audio::*;
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSUUID};
use std::{
    ffi::c_void,
    mem::{size_of, MaybeUninit},
    ptr::{self, NonNull},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

fn check(status: i32, operation: &str) -> Result<()> {
    ensure!(status == 0, "{operation}: OSStatus {status} (0x{:08x}). Check System Settings > Privacy & Security > Screen & System Audio Recording if capture is denied.", status as u32);
    Ok(())
}

fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

// Only called with plain Core Audio integer/ASBD property types.
unsafe fn property<T: Copy>(object: u32, selector: u32) -> Result<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut size = size_of::<T>() as u32;
    check(
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address(selector)),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::new_unchecked(value.as_mut_ptr().cast()),
        ),
        "read property",
    )?;
    ensure!(size as usize == size_of::<T>(), "unexpected property size");
    Ok(value.assume_init())
}

fn processes() -> Result<Vec<(u32, i32, bool)>> {
    unsafe {
        let addr = address(kAudioHardwarePropertyProcessObjectList);
        let mut size = 0;
        check(
            AudioObjectGetPropertyDataSize(
                kAudioObjectSystemObject as u32,
                NonNull::from(&addr),
                0,
                ptr::null(),
                NonNull::from(&mut size),
            ),
            "list audio processes",
        )?;
        let mut ids = vec![0u32; size as usize / size_of::<u32>()];
        check(
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject as u32,
                NonNull::from(&addr),
                0,
                ptr::null(),
                NonNull::from(&mut size),
                NonNull::new_unchecked(ids.as_mut_ptr().cast()),
            ),
            "read audio processes",
        )?;
        ids.truncate(size as usize / size_of::<u32>());
        Ok(ids
            .into_iter()
            .filter_map(|id| {
                let pid = property::<i32>(id, kAudioProcessPropertyPID).ok()?;
                let active =
                    property::<u32>(id, kAudioProcessPropertyIsRunningOutput).unwrap_or(0) != 0;
                Some((id, pid, active))
            })
            .collect())
    }
}

#[derive(Default)]
struct Meter {
    callbacks: AtomicU64,
    samples: AtomicU64,
    nonzero: AtomicU64,
    peak: AtomicU64,
}

// No allocations, locks, file/network IO or printing on the realtime callback.
unsafe extern "C-unwind" fn audio_callback(
    _device: u32,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    context: *mut c_void,
) -> i32 {
    let meter = &*context.cast::<Meter>();
    let list = input.as_ref();
    let buffers = std::slice::from_raw_parts(list.mBuffers.as_ptr(), list.mNumberBuffers as usize);
    let mut count = 0u64;
    let mut nonzero = 0u64;
    let mut peak = 0f32;
    for buffer in buffers {
        if buffer.mData.is_null() {
            continue;
        }
        let samples = std::slice::from_raw_parts(
            buffer.mData.cast::<f32>(),
            buffer.mDataByteSize as usize / size_of::<f32>(),
        );
        for &sample in samples {
            if sample.is_finite() {
                count += 1;
                if sample.abs() > 0.00001 {
                    nonzero += 1;
                }
                peak = peak.max(sample.abs());
            }
        }
    }
    meter.callbacks.fetch_add(1, Ordering::Relaxed);
    meter.samples.fetch_add(count, Ordering::Relaxed);
    meter.nonzero.fetch_add(nonzero, Ordering::Relaxed);
    meter
        .peak
        .fetch_max(peak.to_bits() as u64, Ordering::Relaxed);
    0
}

struct Capture {
    tap: u32,
    device: u32,
    proc_id: AudioDeviceIOProcID,
    started: bool,
    // Stable heap allocation must outlive the IO proc, including during shutdown.
    meter: Box<Meter>,
}
impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            if self.started {
                let _ = AudioDeviceStop(self.device, self.proc_id);
            }
            if self.proc_id.is_some() {
                let _ = AudioDeviceDestroyIOProcID(self.device, self.proc_id);
            }
            if self.device != 0 {
                let _ = AudioHardwareDestroyAggregateDevice(self.device);
            }
            if self.tap != 0 {
                let _ = AudioHardwareDestroyProcessTap(self.tap);
            }
        }
    }
}

fn dict(
    keys: &[&str],
    values: &[&AnyObject],
) -> objc2::rc::Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<_> = keys.iter().map(|key| NSString::from_str(key)).collect();
    NSDictionary::from_slices(&keys.iter().map(|key| &**key).collect::<Vec<_>>(), values)
}

fn capture(pid: i32, seconds: u64, muted: bool) -> Result<()> {
    let targets: Vec<_> = processes()?
        .into_iter()
        .filter(|(_, p, _)| *p == pid)
        .map(|(id, _, _)| NSNumber::numberWithUnsignedInt(id))
        .collect();
    ensure!(
        !targets.is_empty(),
        "PID {pid} has no audio process. Start audio playback and run list again."
    );
    ensure!(
        pid as u32 != std::process::id(),
        "refusing to capture ourselves"
    );
    unsafe {
        let description = CATapDescription::initStereoMixdownOfProcesses(
            CATapDescription::alloc(),
            &NSArray::from_retained_slice(&targets),
        );
        description.setPrivate(true);
        description.setMuteBehavior(if muted {
            CATapMuteBehavior::MutedWhenTapped
        } else {
            CATapMuteBehavior::Unmuted
        });
        let mut capture = Capture {
            tap: 0,
            device: 0,
            proc_id: None,
            started: false,
            meter: Box::default(),
        };
        check(
            AudioHardwareCreateProcessTap(Some(&description), &mut capture.tap),
            "create process tap",
        )?;
        let format: AudioStreamBasicDescription = property(capture.tap, kAudioTapPropertyFormat)?;
        ensure!(
            format.mFormatID == u32::from_be_bytes(*b"lpcm")
                && format.mBitsPerChannel == 32
                && format.mFormatFlags & 1 != 0,
            "spike only supports Float32 PCM: {format:?}"
        );
        println!(
            "Tap format: {} Hz, {} channels; mute-original={muted}",
            format.mSampleRate, format.mChannelsPerFrame
        );
        let tap_uid = description.UUID().UUIDString();
        let yes = NSNumber::numberWithBool(true);
        let sub_tap = dict(&["uid", "drift"], &[&tap_uid, &yes]);
        let taps = NSArray::from_retained_slice(&[sub_tap]);
        let uid = NSUUID::new().UUIDString();
        let name = NSString::from_str("Live Translator temporary audio spike");
        let aggregate = dict(
            &["name", "uid", "private", "taps", "tapautostart"],
            &[&name, &uid, &yes, &taps, &yes],
        );
        // NSDictionary and CFDictionary are toll-free bridged; borrowed only for this call.
        let cf = &*(objc2::rc::Retained::as_ptr(&aggregate).cast::<CFDictionary>());
        check(
            AudioHardwareCreateAggregateDevice(cf, NonNull::from(&mut capture.device)),
            "create private aggregate",
        )?;
        let context = (&mut *capture.meter as *mut Meter).cast();
        check(
            AudioDeviceCreateIOProcID(
                capture.device,
                Some(audio_callback),
                context,
                NonNull::from(&mut capture.proc_id),
            ),
            "register callback",
        )?;
        check(
            AudioDeviceStart(capture.device, capture.proc_id),
            "start capture",
        )?;
        capture.started = true;
        for second in 1..=seconds {
            std::thread::sleep(Duration::from_secs(1));
            println!(
                "{second}s callbacks={} samples={} nonzero={} peak={:.5}",
                capture.meter.callbacks.load(Ordering::Relaxed),
                capture.meter.samples.load(Ordering::Relaxed),
                capture.meter.nonzero.load(Ordering::Relaxed),
                f32::from_bits(capture.meter.peak.load(Ordering::Relaxed) as u32)
            );
        }
        let nonzero = capture.meter.nonzero.load(Ordering::Relaxed);
        drop(capture); // Stop and destroy the tap, restoring original playback before returning.
        ensure!(nonzero > 0, "No non-silent audio captured. Permission may be denied, source silent, or wrong PID selected.");
    }
    println!("Captured non-silent samples. Tap removed. This does NOT prove original output was muted; verify separately.");
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") if args.len() == 1 => {
            println!("AUDIO_ID\tPID\tPLAYING\tCOMMAND");
            for (id, pid, active) in processes()? {
                let output = std::process::Command::new("ps").args(["-p", &pid.to_string(), "-o", "comm="]).output()?;
                println!("{id}\t{pid}\t{active}\t{}", String::from_utf8_lossy(&output.stdout).trim());
            }
            Ok(())
        }
        Some("capture") if args.len() == 3 || (args.len() == 4 && args[3] == "--mute-original") => {
            let pid = args[1].parse().context("PID must be an integer")?;
            ensure!(pid > 0, "PID must be positive");
            let seconds = args[2].parse().context("duration must be an integer")?;
            ensure!((1..=30).contains(&seconds), "duration must be 1–30 seconds");
            capture(pid, seconds, args.len() == 4)
        }
        _ => bail!("Usage: macos-process-audio-spike list\n       macos-process-audio-spike capture PID SECONDS [--mute-original]\nNo audio is saved or transmitted. Select only a source you intend to capture."),
    }
}
