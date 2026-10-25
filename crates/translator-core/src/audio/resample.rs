/// Rational-rate streaming FIR; no packet-boundary reset. Runs off realtime callbacks.
pub struct Resampler {
    taps: [f64; 95],
    history: [f64; 95],
    cursor: usize,
    input_rate: u32,
    phase: u32,
    previous: f64,
}
impl Resampler {
    pub fn new(input_rate: u32) -> Result<Self, &'static str> {
        if ![44100, 48000].contains(&input_rate) {
            return Err("Unsupported capture sample rate");
        }
        let mut taps = [0.; 95];
        let cutoff = 7000. / input_rate as f64;
        for (i, tap) in taps.iter_mut().enumerate() {
            let x = i as f64 - 47.;
            let sinc = if x == 0. {
                2. * cutoff
            } else {
                (2. * std::f64::consts::PI * cutoff * x).sin() / (std::f64::consts::PI * x)
            };
            *tap = sinc * (0.54 - 0.46 * (2. * std::f64::consts::PI * i as f64 / 94.).cos());
        }
        let sum: f64 = taps.iter().sum();
        for tap in &mut taps {
            *tap /= sum;
        }
        Ok(Self {
            taps,
            history: [0.; 95],
            cursor: 0,
            input_rate,
            phase: 0,
            previous: 0.,
        })
    }
    pub fn push(&mut self, sample: f32) -> Option<i16> {
        self.history[self.cursor] = if sample.is_finite() {
            sample as f64
        } else {
            0.
        };
        self.cursor = (self.cursor + 1) % 95;
        let filtered: f64 = self
            .taps
            .iter()
            .enumerate()
            .map(|(i, tap)| tap * self.history[(self.cursor + 94 - i) % 95])
            .sum();
        self.phase += 16000;
        let result = if self.phase >= self.input_rate {
            self.phase -= self.input_rate;
            let fraction = 1. - self.phase as f64 / 16000.;
            Some(
                ((self.previous * (1. - fraction) + filtered * fraction).clamp(-1., 1.) * 32767.)
                    .round() as i16,
            )
        } else {
            None
        };
        self.previous = filtered;
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rates_and_packet_continuity() {
        for rate in [44100, 48000] {
            let mut r = Resampler::new(rate).unwrap();
            let samples: Vec<_> = (0..rate).filter_map(|_| r.push(0.5)).collect();
            assert_eq!(samples.len(), 16000);
            assert!((samples[15999] as i32 - 16384).abs() < 3);
        }
        assert!(Resampler::new(0).is_err());
        assert!(Resampler::new(96000).is_err());
    }
    #[test]
    fn rejects_out_of_band_tone() {
        let mut r = Resampler::new(48000).unwrap();
        let out: Vec<_> = (0..48000)
            .filter_map(|i| r.push((2. * std::f32::consts::PI * 12000. * i as f32 / 48000.).sin()))
            .skip(100)
            .collect();
        let rms = (out
            .iter()
            .map(|v| (*v as f64 / 32768.).powi(2))
            .sum::<f64>()
            / out.len() as f64)
            .sqrt();
        assert!(rms < 0.02, "alias rms {rms}");
    }
}
