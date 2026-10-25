//! Disposable copy of the validated Core Audio tap setup, with a bounded mono queue.
use anyhow::{ensure, Result};
use objc2::{runtime::AnyObject, AnyThread};
use objc2_core_audio::*;
use objc2_core_audio_types::{AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp};
use objc2_core_foundation::CFDictionary;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSUUID};
use rtrb::Producer;
use std::{
    ffi::c_void,
    mem::{size_of, MaybeUninit},
    ptr::{self, NonNull},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

fn check(status: i32, operation: &str) -> Result<()> {
    ensure!(status == 0, "{operation}: Core Audio OSStatus {status}");
    Ok(())
}
fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}
unsafe fn property<T: Copy>(id: u32, selector: u32) -> Result<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut size = size_of::<T>() as u32;
    check(
        AudioObjectGetPropertyData(
            id,
            NonNull::from(&address(selector)),
            0,
            ptr::null(),
            NonNull::from(&mut size),
            NonNull::new_unchecked(value.as_mut_ptr().cast()),
        ),
        "read property",
    )?;
    ensure!(size as usize == size_of::<T>(), "Unexpected property size");
    Ok(value.assume_init())
}
fn process_ids(pid: i32) -> Result<Vec<objc2::rc::Retained<NSNumber>>> {
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
            "list processes",
        )?;
        let mut ids = vec![0u32; size as usize / 4];
        check(
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject as u32,
                NonNull::from(&addr),
                0,
                ptr::null(),
                NonNull::from(&mut size),
                NonNull::new_unchecked(ids.as_mut_ptr().cast()),
            ),
            "get processes",
        )?;
        ids.truncate(size as usize / 4);
        Ok(ids
            .into_iter()
            .filter(|id| property::<i32>(*id, kAudioProcessPropertyPID).ok() == Some(pid))
            .map(NSNumber::numberWithUnsignedInt)
            .collect())
    }
}
struct Context {
    producer: Producer<f32>,
    original: Producer<f32>,
    failed: Arc<AtomicBool>,
}
// Single Core Audio callback owns this producer. No allocation/lock/IO in callback.
unsafe extern "C-unwind" fn callback(
    _device: u32,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    _input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    context: *mut c_void,
) -> i32 {
    let context = &mut *context.cast::<Context>();
    let list = input.as_ref();
    let buffers = std::slice::from_raw_parts(list.mBuffers.as_ptr(), list.mNumberBuffers as usize);
    // Stereo mixdown is either one interleaved stereo buffer or two mono buffers.
    let (left, right, stride) = match buffers {
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
            context.failed.store(true, Ordering::Relaxed);
            return 0;
        }
    };
    for (l, r) in left
        .iter()
        .step_by(stride)
        .zip(right.iter().step_by(stride))
    {
        let sample = (l + r) * 0.5;
        if !sample.is_finite()
            || context.producer.push(sample).is_err()
            || context.original.push(sample).is_err()
        {
            context.failed.store(true, Ordering::Relaxed);
            break;
        }
    }
    0
}

pub struct Capture {
    tap: u32,
    device: u32,
    proc_id: AudioDeviceIOProcID,
    started: bool,
    context: Box<Context>,
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
impl Capture {
    pub fn start(
        pid: i32,
        producer: Producer<f32>,
        original: Producer<f32>,
        failed: Arc<AtomicBool>,
    ) -> Result<Self> {
        ensure!(
            pid > 0 && pid as u32 != std::process::id(),
            "Invalid target PID"
        );
        let targets = process_ids(pid)?;
        ensure!(!targets.is_empty(), "No audio process for PID {pid}");
        unsafe {
            let description = CATapDescription::initStereoMixdownOfProcesses(
                CATapDescription::alloc(),
                &NSArray::from_retained_slice(&targets),
            );
            description.setPrivate(true);
            description.setMuteBehavior(CATapMuteBehavior::MutedWhenTapped);
            let mut capture = Self {
                tap: 0,
                device: 0,
                proc_id: None,
                started: false,
                context: Box::new(Context {
                    producer,
                    original,
                    failed,
                }),
            };
            check(
                AudioHardwareCreateProcessTap(Some(&description), &mut capture.tap),
                "create tap",
            )?;
            let format: AudioStreamBasicDescription =
                property(capture.tap, kAudioTapPropertyFormat)?;
            ensure!(
                format.mFormatID == u32::from_be_bytes(*b"lpcm")
                    && format.mBitsPerChannel == 32
                    && format.mFormatFlags & 1 != 0
                    && format.mSampleRate == 48000.0
                    && format.mChannelsPerFrame == 2,
                "This spike requires Float32 stereo 48 kHz: {format:?}"
            );
            let tap_uid = description.UUID().UUIDString();
            let yes = NSNumber::numberWithBool(true);
            let sub_tap = dict(&["uid", "drift"], &[&tap_uid, &yes]);
            let taps = NSArray::from_retained_slice(&[sub_tap]);
            let uid = NSUUID::new().UUIDString();
            let name = NSString::from_str("Live Translator temporary translation spike");
            let aggregate = dict(
                &["name", "uid", "private", "taps", "tapautostart"],
                &[&name, &uid, &yes, &taps, &yes],
            );
            let cf = &*(objc2::rc::Retained::as_ptr(&aggregate).cast::<CFDictionary>());
            check(
                AudioHardwareCreateAggregateDevice(cf, NonNull::from(&mut capture.device)),
                "create aggregate",
            )?;
            let context = (&mut *capture.context as *mut Context).cast();
            check(
                AudioDeviceCreateIOProcID(
                    capture.device,
                    Some(callback),
                    context,
                    NonNull::from(&mut capture.proc_id),
                ),
                "register capture callback",
            )?;
            check(
                AudioDeviceStart(capture.device, capture.proc_id),
                "start capture",
            )?;
            capture.started = true;
            Ok(capture)
        }
    }
}
