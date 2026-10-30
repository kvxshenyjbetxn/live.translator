use super::{
    enqueue_pcm, mix, AudioError, AudioMeters, CaptureQueues, FaultSignal, LiveGains, Reader,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer};
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn output_unchanged<T: PartialEq>(old: &T, new: &T, old_rate: u32, new_rate: u32) -> bool {
    old == new && old_rate == new_rate
}

pub struct AudioEngine {
    // This handle and all capture handles remain on their owning audio worker thread.
    _stream: cpal::Stream,
    pub input: Consumer<f32>,
    pub output: Producer<f32>,
    pub queues: Option<CaptureQueues>,
    device: cpal::Device,
    device_rate: u32,
    speed: Arc<AtomicU32>,
}
impl AudioEngine {
    pub fn prepare(
        capture_rate: u32,
        gains: Arc<LiveGains>,
        meters: Arc<AudioMeters>,
        fault: FaultSignal,
    ) -> Result<Self, AudioError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or(AudioError("Немає аудіовиходу."))?;
        let config = device
            .default_output_config()
            .map_err(|_| AudioError("Не вдалося налаштувати аудіовихід."))?;
        let device_rate = config.sample_rate().0;
        let (network, input) = rtrb::RingBuffer::new(capture_rate as usize);
        let (original, original_rx) = rtrb::RingBuffer::new(capture_rate as usize);
        let (output, translated_rx) = rtrb::RingBuffer::new(120000);
        let translated = Reader::new(translated_rx, 24000., device_rate as f64, 2400);
        let original_reader = Reader::new(
            original_rx,
            capture_rate as f64,
            device_rate as f64,
            capture_rate as usize / 20,
        );
        let format = config.sample_format();
        let configuration: cpal::StreamConfig = config.into();
        let speed = Arc::new(AtomicU32::new(1f32.to_bits()));
        let builder = Output {
            speed: speed.clone(),
            original: original_reader,
            translated,
            gains,
            meters,
            fault,
        };
        let stream = match format {
            cpal::SampleFormat::F32 => builder.build::<f32>(&device, &configuration),
            cpal::SampleFormat::F64 => builder.build::<f64>(&device, &configuration),
            cpal::SampleFormat::I16 => builder.build::<i16>(&device, &configuration),
            cpal::SampleFormat::U16 => builder.build::<u16>(&device, &configuration),
            cpal::SampleFormat::I32 => builder.build::<i32>(&device, &configuration),
            _ => return Err(AudioError("Непідтримуваний формат аудіовиходу.")),
        }
        .map_err(|_| AudioError("Не вдалося відкрити аудіовихід."))?;
        stream
            .play()
            .map_err(|_| AudioError("Не вдалося почати відтворення."))?;
        Ok(Self {
            _stream: stream,
            input,
            output,
            queues: Some(CaptureQueues { network, original }),
            device,
            device_rate,
            speed,
        })
    }
    pub fn enqueue(&mut self, samples: &[i16]) -> Result<(), AudioError> {
        enqueue_pcm(samples, &mut self.output)
    }
    pub fn backlog(&self) -> f64 {
        (120000 - self.output.slots()) as f64 / 24000.
    }
    pub fn set_backlog(&self, backlog: f64) -> f32 {
        let current = f32::from_bits(self.speed.load(Ordering::Relaxed));
        let target = super::buffer::adaptive_speed(backlog);
        let speed = (current + (target - current).clamp(-0.002, 0.002)).clamp(1., 1.12);
        self.speed.store(speed.to_bits(), Ordering::Relaxed);
        speed
    }
    pub fn output_is_current(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            cpal::default_host()
                .default_output_device()
                .is_some_and(|device| {
                    let (
                        cpal::platform::DeviceInner::CoreAudio(old),
                        cpal::platform::DeviceInner::CoreAudio(new),
                    ) = (self.device.as_inner(), device.as_inner());
                    // CPAL CoreAudio Device equality includes the system AudioDeviceID, not display name.
                    device.default_output_config().is_ok_and(|config| {
                        output_unchanged(old, new, self.device_rate, config.sample_rate().0)
                    })
                })
        }
        #[cfg(target_os = "windows")]
        {
            cpal::default_host()
                .default_output_device()
                .is_some_and(|device| {
                    let (
                        cpal::platform::DeviceInner::Wasapi(old),
                        cpal::platform::DeviceInner::Wasapi(new),
                    ) = (self.device.as_inner(), device.as_inner());
                    device.default_output_config().is_ok_and(|config| {
                        output_unchanged(old, new, self.device_rate, config.sample_rate().0)
                    })
                })
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            let _ = (&self.device, self.device_rate);
            false
        }
    }
}
struct Output {
    speed: Arc<AtomicU32>,
    original: Reader,
    translated: Reader,
    gains: Arc<LiveGains>,
    meters: Arc<AudioMeters>,
    fault: FaultSignal,
}
impl Output {
    fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(
        self,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
        let Self {
            speed,
            mut original,
            mut translated,
            gains,
            meters,
            fault,
        } = self;
        #[cfg(target_os = "windows")]
        let _ = &mut original;
        let channels = config.channels as usize;
        device.build_output_stream(
            config,
            move |output: &mut [T], _| {
                let gains = gains.get();
                translated.set_speed(f32::from_bits(speed.load(Ordering::Relaxed)));
                for frame in output.chunks_mut(channels) {
                    #[cfg(not(target_os = "windows"))]
                    let original_sample = original.sample();
                    #[cfg(target_os = "windows")]
                    let original_sample = 0.;
                    frame.fill(T::from_sample(mix(
                        original_sample,
                        translated.sample(),
                        gains,
                    )));
                }
                meters.played.store(translated.consumed, Ordering::Relaxed);
            },
            move |_| fault.raise(3),
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn output_identity_and_rate_must_both_match() {
        assert!(super::output_unchanged(
            &"endpoint-id",
            &"endpoint-id",
            48000,
            48000
        ));
        assert!(!super::output_unchanged(
            &"endpoint-id",
            &"other-id",
            48000,
            48000
        ));
        assert!(!super::output_unchanged(
            &"endpoint-id",
            &"endpoint-id",
            48000,
            44100
        ));
    }
    #[test]
    fn different_outputs_with_identical_names_and_rates_are_not_current() {
        #[derive(PartialEq)]
        struct Device {
            system_id: u32,
            name: &'static str,
        }
        let old = Device {
            system_id: 10,
            name: "Speakers",
        };
        let new = Device {
            system_id: 20,
            name: "Speakers",
        };
        assert!(!super::output_unchanged(&old, &new, 48000, 48000));
        assert!(super::output_unchanged(&old, &old, 48000, 48000));
        assert!(!super::output_unchanged(&old, &old, 48000, 44100));
    }
}
