use super::CaptureError;
use crate::{
    audio::{AudioMeters, CaptureQueues, FaultSignal},
    sources::{source_identity, unchanged, AudioSource, ProcessIdentity, ResolvedSource},
};
use objc2::{runtime::AnyObject, AnyThread};
use objc2_core_audio::*;
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSUUID};
use std::{
    collections::BTreeMap,
    ffi::{c_void, CStr},
    mem::{size_of, MaybeUninit},
    ptr::{self, NonNull},
    sync::{atomic::Ordering, Arc},
};

fn check(status: i32) -> Result<(), CaptureError> {
    if status == 0 {
        Ok(())
    } else {
        Err(CaptureError("Core Audio: не вдалося відкрити джерело. Перевірте дозвіл на запис системного аудіо в налаштуваннях macOS."))
    }
}
fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}
unsafe fn property<T: Copy>(object: u32, selector: u32) -> Result<T, CaptureError> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut size = size_of::<T>() as u32;
    check(AudioObjectGetPropertyData(
        object,
        NonNull::from(&address(selector)),
        0,
        ptr::null(),
        NonNull::from(&mut size),
        NonNull::new_unchecked(value.as_mut_ptr().cast()),
    ))?;
    if size as usize != size_of::<T>() {
        return Err(CaptureError("Некоректний формат властивості Core Audio."));
    }
    Ok(value.assume_init())
}
fn process_identity(object_id: u32) -> Option<ProcessIdentity> {
    unsafe {
        let pid: i32 = property(object_id, kAudioProcessPropertyPID).ok()?;
        if pid <= 0 || pid as u32 == std::process::id() {
            return None;
        }
        let mut path = [0i8; 4096];
        if libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) <= 0 {
            return None;
        }
        let executable = CStr::from_ptr(path.as_ptr()).to_string_lossy().into_owned();
        let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        if libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            info.as_mut_ptr().cast(),
            size_of::<libc::proc_bsdinfo>() as i32,
        ) != size_of::<libc::proc_bsdinfo>() as i32
        {
            return None;
        }
        let info = info.assume_init();
        Some(ProcessIdentity {
            object_id,
            pid,
            executable,
            started: (info.pbi_start_tvsec, info.pbi_start_tvusec),
            active: property::<u32>(object_id, kAudioProcessPropertyIsRunningOutput).unwrap_or(0)
                != 0,
        })
    }
}
fn processes() -> Result<Vec<ProcessIdentity>, CaptureError> {
    unsafe {
        let mut size = 0;
        let addr = address(kAudioHardwarePropertyProcessObjectList);
        check(AudioObjectGetPropertyDataSize(
            kAudioObjectSystemObject as u32,
            NonNull::from(&addr),
            0,
            ptr::null(),
            NonNull::from(&mut size),
        ))?;
        let mut ids = vec![0u32; size as usize / 4];
        check(AudioObjectGetPropertyData(
            kAudioObjectSystemObject as u32,
            NonNull::from(&addr),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::new_unchecked(ids.as_mut_ptr().cast()),
        ))?;
        ids.truncate(size as usize / 4);
        Ok(ids.into_iter().filter_map(process_identity).collect())
    }
}
pub fn list_sources() -> Result<Vec<AudioSource>, CaptureError> {
    let mut grouped = BTreeMap::<String, AudioSource>::new();
    for process in processes()? {
        let (id, name) = source_identity(&process.executable);
        if !id.starts_with("app:") && !process.active {
            continue;
        }
        let entry = grouped.entry(id.clone()).or_insert(AudioSource {
            id,
            name,
            active: false,
            detail: Some("Звук усієї програми".into()),
        });
        entry.active |= process.active;
    }
    let mut sources: Vec<_> = grouped.into_values().collect();
    sources.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(sources)
}
pub fn resolve_source(id: &str) -> Result<ResolvedSource, CaptureError> {
    let processes: Vec<_> = processes()?
        .into_iter()
        .filter(|p| source_identity(&p.executable).0 == id)
        .collect();
    if processes.is_empty() {
        return Err(CaptureError(
            "Програма більше недоступна. Оновіть список джерел.",
        ));
    }
    Ok(ResolvedSource {
        id: id.into(),
        processes,
    })
}
pub fn source_still_valid(source: &ResolvedSource) -> bool {
    let current: Vec<_> = source
        .processes
        .iter()
        .filter_map(|p| process_identity(p.object_id))
        .collect();
    unchanged(&source.processes, &current)
}
struct Context {
    queues: CaptureQueues,
    meters: Arc<AudioMeters>,
    fault: FaultSignal,
}
unsafe extern "C-unwind" fn callback(
    _: u32,
    _: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _: NonNull<AudioTimeStamp>,
    _: NonNull<AudioBufferList>,
    _: NonNull<AudioTimeStamp>,
    context: *mut c_void,
) -> i32 {
    let context = &mut *context.cast::<Context>();
    let list = input.as_ref();
    let buffers = std::slice::from_raw_parts(list.mBuffers.as_ptr(), list.mNumberBuffers as usize);
    let (left, right, stride) = match buffers {
        [mono]
            if mono.mNumberChannels == 1
                && !mono.mData.is_null()
                && mono.mDataByteSize % 4 == 0 =>
        {
            let data = std::slice::from_raw_parts(
                mono.mData.cast::<f32>(),
                mono.mDataByteSize as usize / 4,
            );
            (data, data, 1)
        }
        [stereo]
            if stereo.mNumberChannels == 2
                && !stereo.mData.is_null()
                && stereo.mDataByteSize % 8 == 0 =>
        {
            let data = std::slice::from_raw_parts(
                stereo.mData.cast::<f32>(),
                stereo.mDataByteSize as usize / 4,
            );
            if data.len() < 2 {
                return 0;
            }
            (data, &data[1..], 2)
        }
        [l, r]
            if l.mNumberChannels == 1
                && r.mNumberChannels == 1
                && !l.mData.is_null()
                && !r.mData.is_null()
                && l.mDataByteSize == r.mDataByteSize
                && l.mDataByteSize % 4 == 0 =>
        {
            (
                std::slice::from_raw_parts(l.mData.cast::<f32>(), l.mDataByteSize as usize / 4),
                std::slice::from_raw_parts(r.mData.cast::<f32>(), r.mDataByteSize as usize / 4),
                1,
            )
        }
        _ => {
            context.fault.raise(2);
            return 0;
        }
    };
    let mut peak = 0f32;
    for (l, r) in left
        .iter()
        .step_by(stride)
        .zip(right.iter().step_by(stride))
    {
        let sample = (l + r) * 0.5;
        if !sample.is_finite()
            || context.queues.network.push(sample).is_err()
            || context.queues.original.push(sample).is_err()
        {
            context.fault.raise(1);
            break;
        }
        peak = peak.max(sample.abs());
    }
    context
        .meters
        .peak
        .fetch_max(peak.to_bits(), Ordering::Relaxed);
    context.meters.callbacks.fetch_add(1, Ordering::Relaxed);
    0
}

pub struct CaptureHandle {
    tap: u32,
    device: u32,
    proc_id: AudioDeviceIOProcID,
    started: bool,
    rate: u32,
    source: ResolvedSource,
    context: Option<Box<Context>>,
}
impl CaptureHandle {
    pub fn prepare(source: ResolvedSource) -> Result<Self, CaptureError> {
        if !source_still_valid(&source) {
            return Err(CaptureError("Джерело змінилося. Оновіть список."));
        }
        unsafe {
            let ids: Vec<_> = source
                .processes
                .iter()
                .map(|p| NSNumber::numberWithUnsignedInt(p.object_id))
                .collect();
            let description = CATapDescription::initStereoMixdownOfProcesses(
                CATapDescription::alloc(),
                &NSArray::from_retained_slice(&ids),
            );
            description.setPrivate(true);
            description.setMuteBehavior(CATapMuteBehavior::MutedWhenTapped);
            let mut handle = Self {
                tap: 0,
                device: 0,
                proc_id: None,
                started: false,
                rate: 0,
                source,
                context: None,
            };
            check(AudioHardwareCreateProcessTap(
                Some(&description),
                &mut handle.tap,
            ))?;
            let format: AudioStreamBasicDescription =
                property(handle.tap, kAudioTapPropertyFormat)?;
            if format.mFormatID != u32::from_be_bytes(*b"lpcm")
                || format.mBitsPerChannel != 32
                || format.mFormatFlags & 1 == 0
                || ![44100., 48000.].contains(&format.mSampleRate)
                || !(1..=2).contains(&format.mChannelsPerFrame)
            {
                return Err(CaptureError("Джерело має непідтримуваний формат. Потрібен Float32 PCM 44.1/48 kHz mono/stereo."));
            }
            handle.rate = format.mSampleRate as u32;
            let uid = description.UUID().UUIDString();
            let yes = NSNumber::numberWithBool(true);
            let sub = dict(&["uid", "drift"], &[&uid, &yes]);
            let taps = NSArray::from_retained_slice(&[sub]);
            let aggregate_uid = NSUUID::new().UUIDString();
            let name = NSString::from_str("Live Translator audio");
            let aggregate = dict(
                &["name", "uid", "private", "taps", "tapautostart"],
                &[&name, &aggregate_uid, &yes, &taps, &yes],
            );
            let cf = &*(objc2::rc::Retained::as_ptr(&aggregate).cast::<CFDictionary>());
            check(AudioHardwareCreateAggregateDevice(
                cf,
                NonNull::from(&mut handle.device),
            ))?;
            Ok(handle)
        }
    }
    pub fn rate(&self) -> u32 {
        self.rate
    }
    pub fn start(
        &mut self,
        queues: CaptureQueues,
        meters: Arc<AudioMeters>,
        fault: FaultSignal,
    ) -> Result<(), CaptureError> {
        if self.started || !source_still_valid(&self.source) {
            return Err(CaptureError("Джерело змінилося або вже запущене."));
        }
        let mut context = Box::new(Context {
            queues,
            meters,
            fault,
        });
        unsafe {
            check(AudioDeviceCreateIOProcID(
                self.device,
                Some(callback),
                (&mut *context as *mut Context).cast(),
                NonNull::from(&mut self.proc_id),
            ))?;
            self.context = Some(context);
            check(AudioDeviceStart(self.device, self.proc_id))?;
        }
        self.started = true;
        Ok(())
    }
    pub fn stop(&mut self) -> Result<(), CaptureError> {
        let mut error = false;
        let mut callback_stopped = !self.started;
        unsafe {
            if self.started {
                if AudioDeviceStop(self.device, self.proc_id) == 0 {
                    self.started = false;
                    callback_stopped = true;
                } else {
                    error = true;
                }
            }
            if self.proc_id.is_some() {
                if AudioDeviceDestroyIOProcID(self.device, self.proc_id) == 0 {
                    self.proc_id = None;
                    callback_stopped = true;
                } else {
                    error = true;
                }
            }
            if self.device != 0 {
                if AudioHardwareDestroyAggregateDevice(self.device) == 0 {
                    self.device = 0;
                } else {
                    error = true;
                }
            }
            if self.tap != 0 {
                if AudioHardwareDestroyProcessTap(self.tap) == 0 {
                    self.tap = 0;
                } else {
                    error = true;
                }
            }
        }
        // Never free memory still potentially borrowed by a failed-to-stop native callback.
        if callback_stopped {
            self.context.take();
        }
        if error {
            Err(CaptureError("Не вдалося повністю очистити Core Audio. Перевірте повернення оригінального звуку."))
        } else {
            Ok(())
        }
    }
}
impl Drop for CaptureHandle {
    fn drop(&mut self) {
        let _ = self.stop();
        if let Some(context) = self.context.take() {
            std::mem::forget(context);
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
