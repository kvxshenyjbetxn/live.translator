use anyhow::{ensure, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

/// Windowed-sinc low-pass, then 3:1 decimation. Keeps state across capture packets.
pub struct Downsample {
    taps: [f32; 63],
    history: [f32; 63],
    cursor: usize,
    phase: usize,
}
impl Downsample {
    pub fn new() -> Self {
        let mut taps = [0.; 63];
        for (i, tap) in taps.iter_mut().enumerate() {
            let x = i as f64 - 31.;
            let cutoff = 7000. / 48000.;
            let sinc = if x == 0. {
                2. * cutoff
            } else {
                (2. * std::f64::consts::PI * cutoff * x).sin() / (std::f64::consts::PI * x)
            };
            let window = 0.54 - 0.46 * (2. * std::f64::consts::PI * i as f64 / 62.).cos();
            *tap = (sinc * window) as f32;
        }
        let sum: f32 = taps.iter().sum();
        for tap in &mut taps {
            *tap /= sum;
        }
        Self {
            taps,
            history: [0.; 63],
            cursor: 0,
            phase: 0,
        }
    }
    pub fn push(&mut self, sample: f32) -> Option<i16> {
        self.history[self.cursor] = sample;
        self.cursor = (self.cursor + 1) % 63;
        self.phase += 1;
        if self.phase != 3 {
            return None;
        }
        self.phase = 0;
        let filtered: f32 = self
            .taps
            .iter()
            .enumerate()
            .map(|(i, tap)| tap * self.history[(self.cursor + 62 - i) % 63])
            .sum();
        Some((filtered.clamp(-1., 1.) * 32767.).round() as i16)
    }
}

struct Reader {
    input: Consumer<f32>,
    ratio: f64,
    prebuffer: usize,
    phase: f64,
    current: f32,
    next: f32,
    ready: bool,
    consumed: u64,
}
impl Reader {
    fn new(input: Consumer<f32>, rate: f64, output_rate: f64, prebuffer: usize) -> Self {
        Self {
            input,
            ratio: rate / output_rate,
            prebuffer,
            phase: 0.,
            current: 0.,
            next: 0.,
            ready: false,
            consumed: 0,
        }
    }
    fn sample(&mut self) -> f32 {
        if !self.ready {
            if self.input.slots() < self.prebuffer.max(2) {
                return 0.;
            }
            self.current = self.input.pop().unwrap_or(0.);
            self.next = self.input.pop().unwrap_or(0.);
            self.consumed += 2;
            self.phase = 0.;
            self.ready = true;
        }
        let sample =
            (self.current as f64 * (1. - self.phase) + self.next as f64 * self.phase) as f32;
        self.phase += self.ratio;
        while self.phase >= 1. {
            self.phase -= 1.;
            self.current = self.next;
            match self.input.pop() {
                Ok(value) => {
                    self.next = value;
                    self.consumed += 1;
                }
                Err(_) => {
                    self.ready = false;
                    self.current = 0.;
                    self.next = 0.;
                    self.phase = 0.;
                    break;
                }
            }
        }
        sample
    }
}

fn mix(translated: f32, original: f32, original_gain: f32) -> f32 {
    (translated * 0.7 + original * original_gain).clamp(-1., 1.)
}

pub fn playback(
    input: Consumer<f32>,
    original: Consumer<f32>,
    original_gain: f32,
    failed: Arc<AtomicBool>,
    played: Arc<AtomicU64>,
) -> Result<cpal::Stream> {
    let device = cpal::default_host()
        .default_output_device()
        .context("No audio output device")?;
    let config = device.default_output_config()?;
    ensure!(
        config.sample_format() == cpal::SampleFormat::F32,
        "Spike supports only Float32 output"
    );
    let channels = config.channels() as usize;
    let rate = config.sample_rate().0 as f64;
    let mut translated = Reader::new(input, 24000., rate, 2400);
    let mut original = Reader::new(original, 48000., rate, 2048);
    let stream = device.build_output_stream(
        &config.into(),
        move |output: &mut [f32], _| {
            for frame in output.chunks_mut(channels) {
                frame.fill(mix(translated.sample(), original.sample(), original_gain));
            }
            played.store(translated.consumed, Ordering::Relaxed);
        },
        move |_| {
            failed.store(true, Ordering::Relaxed);
        },
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

/// Decode a whole chunk first; fail rather than silently discard queued speech.
pub fn enqueue_pcm(bytes: &[u8], output: &mut Producer<f32>) -> Result<usize> {
    ensure!(bytes.len().is_multiple_of(2), "Invalid PCM16 chunk length");
    let count = bytes.len() / 2;
    ensure!(
        output.slots() >= count,
        "Playback backlog exceeded 5 seconds; stopping instead of dropping speech"
    );
    for sample in bytes.chunks_exact(2) {
        let value = i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.;
        output
            .push(value)
            .map_err(|_| anyhow::anyhow!("Playback queue overflow"))?;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimator_count_and_dc() {
        let mut filter = Downsample::new();
        let samples: Vec<_> = (0..4800).filter_map(|_| filter.push(0.5)).collect();
        assert_eq!(samples.len(), 1600);
        assert!((samples[1599] as i32 - 16384).abs() <= 1);
    }
    #[test]
    fn original_mix_works_without_translation() {
        assert!((mix(0., 0.8, 0.15) - 0.12).abs() < 0.00001);
        assert_eq!(mix(0., 0.8, 0.), 0.);
        assert_eq!(mix(1., 1., 1.), 1.);
        let (mut producer, consumer) = rtrb::RingBuffer::new(16);
        let mut reader = Reader::new(consumer, 48000., 48000., 4);
        assert_eq!(reader.sample(), 0.);
        for _ in 0..8 {
            producer.push(0.8).unwrap();
        }
        assert!((mix(0., reader.sample(), 0.15) - 0.12).abs() < 0.00001);
        assert!(reader.consumed > 0);
    }
    #[test]
    fn rejects_partial_pcm_and_overflow() {
        let (mut p, mut c) = rtrb::RingBuffer::new(2);
        assert!(enqueue_pcm(&[1], &mut p).is_err());
        assert_eq!(enqueue_pcm(&[0, 128, 255, 127], &mut p).unwrap(), 2);
        assert!(enqueue_pcm(&[0, 0], &mut p).is_err());
        assert_eq!(c.pop().unwrap(), -1.);
        assert!(c.pop().unwrap() > 0.99);
    }
}
