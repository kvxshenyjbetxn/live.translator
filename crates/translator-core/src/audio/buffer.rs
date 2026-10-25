use super::AudioError;
use std::{collections::VecDeque, time::Duration};
#[derive(Default)]
pub struct PlaybackBuffer {
    pending: VecDeque<i16>,
    lag_since: Option<Duration>,
}
impl PlaybackBuffer {
    pub fn receive(&mut self, pcm: Vec<i16>, queued: usize) -> Result<(), AudioError> {
        if pcm
            .len()
            .saturating_add(self.pending.len())
            .saturating_add(queued)
            > 24000 * 15
        {
            return Err(AudioError("Переклад відстає понад 15 секунд. Зупинено, щоб не накопичувати відставання. Спробуйте менш швидке джерело."));
        }
        self.pending.extend(pcm);
        Ok(())
    }
    pub fn pump(&mut self, output: &mut rtrb::Producer<f32>) {
        let queued = 120000 - output.slots();
        let available = 48000usize.saturating_sub(queued).min(output.slots());
        for _ in 0..available.min(self.pending.len()) {
            if let Some(sample) = self.pending.pop_front() {
                // Only this worker owns the producer; consumer can only increase available space.
                if output.push(sample as f32 / 32768.).is_err() {
                    self.pending.push_front(sample);
                    break;
                }
            }
        }
    }
    pub fn waiting(&self) -> usize {
        self.pending.len()
    }
    pub fn observe(&mut self, backlog: f64, elapsed: Duration) -> Result<(), AudioError> {
        if backlog <= 5. {
            self.lag_since = None;
            return Ok(());
        }
        let since = *self.lag_since.get_or_insert(elapsed);
        if elapsed.saturating_sub(since) >= Duration::from_secs(20) {
            return Err(AudioError("Переклад стійко відстає понад 5 секунд і не встигає наздогнати. Зупинено без пропуску мовлення."));
        }
        Ok(())
    }
}
pub fn adaptive_speed(backlog: f64) -> f32 {
    (1. + ((backlog - 0.8).max(0.) * 0.04).min(0.12)) as f32
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn six_second_server_burst_is_played_in_order_without_drops() {
        let mut buffer = PlaybackBuffer::default();
        let (mut output, mut input) = rtrb::RingBuffer::new(120000);
        buffer.receive(vec![1234; 144000], 0).unwrap();
        let mut played = 0;
        while buffer.waiting() > 0 {
            buffer.pump(&mut output);
            assert!(input.slots() <= 48000);
            while let Ok(sample) = input.pop() {
                assert_eq!(sample, 1234. / 32768.);
                played += 1;
            }
        }
        assert_eq!(played, 144000);
    }
    #[test]
    fn hard_limit_and_sustained_lag_are_bounded_but_brief_bursts_are_tolerated() {
        let mut buffer = PlaybackBuffer::default();
        assert!(buffer.receive(vec![0; 360001], 0).is_err());
        assert!(buffer.observe(6., Duration::from_secs(1)).is_ok());
        assert!(buffer.observe(6., Duration::from_secs(19)).is_ok());
        assert!(buffer.observe(6., Duration::from_secs(21)).is_err());
        assert!(buffer.observe(1., Duration::from_secs(22)).is_ok());
        assert!(buffer.observe(6., Duration::from_secs(23)).is_ok());
        assert_eq!(adaptive_speed(0.3), 1.);
        assert!(adaptive_speed(2.) > 1.);
        assert!(adaptive_speed(15.) <= 1.12);
    }
}
