pub mod buffer;
mod engine;
pub mod mixer;
pub mod resample;
pub use engine::AudioEngine;
pub use mixer::{LiveGains, Reader};
use rtrb::Producer;
use std::sync::{
    atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering},
    Arc,
};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct AudioError(pub &'static str);
#[derive(Default)]
pub struct AudioMeters {
    pub peak: AtomicU32,
    pub callbacks: AtomicU64,
    pub played: AtomicU64,
}
#[derive(Clone, Default)]
pub struct FaultSignal(Arc<AtomicU8>);
impl FaultSignal {
    pub fn raise(&self, code: u8) {
        let _ = self
            .0
            .compare_exchange(0, code, Ordering::Relaxed, Ordering::Relaxed);
    }
    pub fn get(&self) -> u8 {
        self.0.load(Ordering::Relaxed)
    }
}
pub struct CaptureQueues {
    pub network: Producer<f32>,
    pub original: Producer<f32>,
}
pub fn mix(original: f32, translation: f32, gains: (f32, f32)) -> f32 {
    (original * gains.0 + translation * gains.1).clamp(-1., 1.)
}
pub fn enqueue_pcm(pcm: &[i16], output: &mut Producer<f32>) -> Result<(), AudioError> {
    if pcm.len() > output.slots() {
        return Err(AudioError(
            "Черга перекладу перевищила 5 секунд. Переклад зупинено.",
        ));
    }
    for sample in pcm {
        output
            .push(*sample as f32 / 32768.)
            .map_err(|_| AudioError("Переповнення черги озвучення."))?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_and_gains_validate_without_dropping_speech() {
        let gains = LiveGains::new(0.15, 0.7).unwrap();
        assert!(gains.set(f32::NAN, 0.).is_err());
        assert_eq!(gains.get(), (0.15, 0.7));
        gains.set(0., 1.).unwrap();
        assert_eq!(gains.get(), (0., 1.));
        let (mut p, mut c) = rtrb::RingBuffer::new(2);
        enqueue_pcm(&[-32768, 32767], &mut p).unwrap();
        assert!(enqueue_pcm(&[0], &mut p).is_err());
        assert_eq!(c.pop().unwrap(), -1.);
        assert!(c.pop().unwrap() > 0.99);
        let fault = FaultSignal::default();
        fault.raise(1);
        fault.raise(2);
        assert_eq!(fault.get(), 1);
    }
    #[test]
    fn independent_mix_and_peak_safety() {
        assert_eq!(mix(1., 0., (0., 0.7)), 0.);
        assert!((mix(0.8, 0., (0.15, 0.7)) - 0.12).abs() < 0.00001);
        assert!((mix(0., 1., (0.15, 0.7)) - 0.7).abs() < 0.00001);
        assert_eq!(mix(1., 1., (1., 1.)), 1.);
        assert_eq!(mix(-1., -1., (1., 1.)), -1.);
    }
}
