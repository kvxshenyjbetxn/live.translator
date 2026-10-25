use super::AudioError;
use rtrb::Consumer;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct LiveGains(AtomicU64);
impl LiveGains {
    pub fn new(original: f32, translation: f32) -> Result<Self, AudioError> {
        let gains = Self(AtomicU64::new(0));
        gains.set(original, translation)?;
        Ok(gains)
    }
    pub fn set(&self, original: f32, translation: f32) -> Result<(), AudioError> {
        if !crate::settings::valid_gain(original) || !crate::settings::valid_gain(translation) {
            return Err(AudioError("Гучність має бути від 0 до 100%."));
        }
        self.0.store(
            ((original.to_bits() as u64) << 32) | translation.to_bits() as u64,
            Ordering::Relaxed,
        );
        Ok(())
    }
    pub fn get(&self) -> (f32, f32) {
        let bits = self.0.load(Ordering::Relaxed);
        (
            f32::from_bits((bits >> 32) as u32),
            f32::from_bits(bits as u32),
        )
    }
}
pub struct Reader {
    input: Consumer<f32>,
    ratio: f64,
    base_ratio: f64,
    prebuffer: usize,
    phase: f64,
    current: f32,
    next: f32,
    ready: bool,
    pub consumed: u64,
}
impl Reader {
    pub fn new(input: Consumer<f32>, rate: f64, output_rate: f64, prebuffer: usize) -> Self {
        Self {
            input,
            ratio: rate / output_rate,
            base_ratio: rate / output_rate,
            prebuffer,
            phase: 0.,
            current: 0.,
            next: 0.,
            ready: false,
            consumed: 0,
        }
    }
    pub fn set_speed(&mut self, speed: f32) {
        self.ratio = self.base_ratio * speed.clamp(1., 1.12) as f64;
    }
    pub fn sample(&mut self) -> f32 {
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catchup_accelerates_only_translation_without_compounding() {
        let (mut p1, c1) = rtrb::RingBuffer::new(512);
        let (mut p2, c2) = rtrb::RingBuffer::new(512);
        for _ in 0..500 {
            p1.push(0.5).unwrap();
            p2.push(0.5).unwrap();
        }
        let mut normal = Reader::new(c1, 48000., 48000., 4);
        let mut fast = Reader::new(c2, 48000., 48000., 4);
        fast.set_speed(1.12);
        fast.set_speed(1.12);
        for _ in 0..100 {
            normal.sample();
            fast.sample();
        }
        assert_eq!(normal.consumed, 102);
        assert!((113..=115).contains(&fast.consumed));
    }
    #[test]
    fn original_reader_runs_before_translation() {
        let (mut producer, consumer) = rtrb::RingBuffer::new(32);
        let mut reader = Reader::new(consumer, 48000., 48000., 4);
        assert_eq!(reader.sample(), 0.);
        for _ in 0..16 {
            producer.push(0.8).unwrap();
        }
        assert_eq!(reader.sample(), 0.8);
        assert!(reader.consumed > 0);
    }
}
