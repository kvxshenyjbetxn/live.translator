use serde::Serialize;
use std::{collections::VecDeque, time::Duration};

const WINDOW: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticSample {
    pub elapsed_seconds: f64,
    pub sending_queue_seconds: f64,
    pub playback_queue_seconds: f64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSample {
    pub elapsed_seconds: f64,
    pub rtt_ms: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticIssue {
    NetworkUnstable,
    SendingQueue,
    PlaybackQueue,
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSnapshot {
    pub elapsed_seconds: f64,
    pub samples: Vec<DiagnosticSample>,
    pub network_samples: Vec<NetworkSample>,
    pub network_rtt_ms: Option<f64>,
    pub send_duration_ms: Option<f64>,
    pub sending_queue_seconds: f64,
    pub playback_queue_seconds: f64,
    pub issues: Vec<DiagnosticIssue>,
}
#[derive(Default)]
pub struct Diagnostics {
    samples: VecDeque<DiagnosticSample>,
    network: VecDeque<NetworkSample>,
    sending_since: Option<Duration>,
    playback_since: Option<Duration>,
    last_send: Option<(Duration, Duration)>,
}
impl Diagnostics {
    pub fn observe(&mut self, now: Duration, sending: f64, playback: f64) {
        if self
            .samples
            .back()
            .is_some_and(|last| now.as_secs_f64() - last.elapsed_seconds < 0.0999)
        {
            return;
        }
        // A sampling gap is not evidence that a queue remained high throughout it.
        if self
            .samples
            .back()
            .is_some_and(|last| now.as_secs_f64() - last.elapsed_seconds > 1.5)
        {
            self.sending_since = None;
            self.playback_since = None;
        }
        Self::track(&mut self.sending_since, now, sending >= 0.5);
        Self::track(&mut self.playback_since, now, playback >= 2.);
        self.samples.push_back(DiagnosticSample {
            elapsed_seconds: now.as_secs_f64(),
            sending_queue_seconds: sending,
            playback_queue_seconds: playback,
        });
        while self.samples.len() > 101 {
            self.samples.pop_front();
        }
        self.prune(now);
    }
    fn track(since: &mut Option<Duration>, now: Duration, high: bool) {
        if high {
            since.get_or_insert(now);
        } else {
            *since = None;
        }
    }
    pub fn record_rtt(&mut self, now: Duration, rtt: Duration) {
        self.network.push_back(NetworkSample {
            elapsed_seconds: now.as_secs_f64(),
            rtt_ms: rtt.as_secs_f64() * 1000.,
        });
        while self.network.len() > 10 {
            self.network.pop_front();
        }
        self.prune(now);
    }
    pub fn record_send(&mut self, now: Duration, duration: Duration) {
        self.last_send = Some((now, duration));
    }
    fn prune(&mut self, now: Duration) {
        let cutoff = now.as_secs_f64() - WINDOW.as_secs_f64();
        while self
            .samples
            .front()
            .is_some_and(|s| s.elapsed_seconds < cutoff)
        {
            self.samples.pop_front();
        }
        if self.samples.is_empty() {
            // Once observations expire, they cannot establish sustained queue growth.
            self.sending_since = None;
            self.playback_since = None;
        }
    }
    pub fn snapshot(&mut self, now: Duration) -> DiagnosticsSnapshot {
        self.prune(now);
        let mut issues = Vec::new();
        // These thresholds are cautious heuristics, not provider guarantees.
        let fresh_network: Vec<_> = self
            .network
            .iter()
            .filter(|s| now.as_secs_f64() - s.elapsed_seconds <= WINDOW.as_secs_f64())
            .collect();
        if fresh_network.len() >= 3 {
            let min = fresh_network
                .iter()
                .map(|s| s.rtt_ms)
                .fold(f64::INFINITY, f64::min);
            let max = fresh_network.iter().map(|s| s.rtt_ms).fold(0., f64::max);
            let mean =
                fresh_network.iter().map(|s| s.rtt_ms).sum::<f64>() / fresh_network.len() as f64;
            if mean >= 500. || max - min >= 250. {
                issues.push(DiagnosticIssue::NetworkUnstable);
            }
        }
        let latest = self.samples.back();
        let fresh = latest.is_some_and(|s| now.as_secs_f64() - s.elapsed_seconds <= 1.5);
        if fresh {
            if self
                .sending_since
                .is_some_and(|s| now.saturating_sub(s) >= Duration::from_secs(2))
            {
                issues.push(DiagnosticIssue::SendingQueue);
            }
            if self
                .playback_since
                .is_some_and(|s| now.saturating_sub(s) >= Duration::from_secs(2))
            {
                issues.push(DiagnosticIssue::PlaybackQueue);
            }
        }
        DiagnosticsSnapshot {
            elapsed_seconds: now.as_secs_f64(),
            samples: self.samples.iter().cloned().collect(),
            network_samples: self.network.iter().cloned().collect(),
            network_rtt_ms: fresh_network.last().map(|s| s.rtt_ms),
            send_duration_ms: self
                .last_send
                .filter(|(time, _)| now.saturating_sub(*time) <= WINDOW)
                .map(|(_, d)| d.as_secs_f64() * 1000.),
            sending_queue_seconds: latest.map_or(0., |s| s.sending_queue_seconds),
            playback_queue_seconds: latest.map_or(0., |s| s.playback_queue_seconds),
            issues,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(seconds: f64) -> Duration {
        Duration::from_secs_f64(seconds)
    }

    #[test]
    fn bounded_history_expires_without_fabricating_network_samples() {
        let mut d = Diagnostics::default();
        assert_eq!(d.snapshot(at(0.)).network_rtt_ms, None);
        d.record_rtt(at(0.), at(0.08));
        for i in 0..2001 {
            d.observe(at(i as f64 / 100.), 0.05, 0.1);
        }
        let snapshot = d.snapshot(at(20.));
        assert!(snapshot.samples.len() <= 101);
        assert!(snapshot.samples.iter().all(|s| s.elapsed_seconds >= 10.));
        assert_eq!(
            snapshot.network_samples.len(),
            1,
            "ping bars persist until a new sample replaces them"
        );
        assert_eq!(snapshot.network_rtt_ms, None);
        assert!(snapshot.issues.is_empty(), "silence is not an issue");
        d.record_rtt(at(20.), at(0.1));
        d.observe(at(20.1), 0., 0.);
        assert_eq!(d.snapshot(at(20.1)).network_samples.len(), 2);
        assert!(Diagnostics::default().snapshot(at(0.)).samples.is_empty());
    }

    #[test]
    fn transient_queues_do_not_warn_but_sustained_simultaneous_queues_do() {
        let mut d = Diagnostics::default();
        d.observe(at(0.), 0.6, 2.5);
        assert!(d.snapshot(at(0.)).issues.is_empty());
        d.observe(at(1.), 0., 0.);
        d.observe(at(2.), 0.6, 2.5);
        d.observe(at(3.), 0.6, 2.5);
        d.observe(at(4.), 0.6, 2.5);
        let issues = d.snapshot(at(4.)).issues;
        assert!(issues.contains(&DiagnosticIssue::SendingQueue));
        assert!(issues.contains(&DiagnosticIssue::PlaybackQueue));
        d.observe(at(4.2), 0., 0.);
        assert!(d.snapshot(at(4.2)).issues.is_empty());
    }

    #[test]
    fn network_warning_needs_three_fresh_replies_and_expires() {
        let mut d = Diagnostics::default();
        d.record_rtt(at(0.), at(0.1));
        d.record_rtt(at(5.), at(0.9));
        assert!(d.snapshot(at(5.)).issues.is_empty());
        d.record_rtt(at(10.), at(0.2));
        assert!(d
            .snapshot(at(10.))
            .issues
            .contains(&DiagnosticIssue::NetworkUnstable));
        assert!(!d
            .snapshot(at(10.1))
            .issues
            .contains(&DiagnosticIssue::NetworkUnstable));
        assert_eq!(d.snapshot(at(21.)).network_rtt_ms, None);
    }

    #[test]
    fn pruned_history_cannot_establish_continuity_after_an_unobserved_gap() {
        for prune_via_rtt in [false, true] {
            let mut d = Diagnostics::default();
            d.observe(at(0.), 0.8, 3.);
            if prune_via_rtt {
                d.record_rtt(at(11.), at(0.08));
            } else {
                assert!(d.snapshot(at(11.)).samples.is_empty());
            }
            d.observe(at(11.1), 0.8, 3.);
            assert!(
                d.snapshot(at(11.1)).issues.is_empty(),
                "old warnings survived pruning (RTT={prune_via_rtt})"
            );
            d.observe(at(12.1), 0.8, 3.);
            d.observe(at(13.1), 0.8, 3.);
            assert!(d
                .snapshot(at(13.1))
                .issues
                .contains(&DiagnosticIssue::SendingQueue));
            assert!(d
                .snapshot(at(13.1))
                .issues
                .contains(&DiagnosticIssue::PlaybackQueue));
        }
    }

    #[test]
    fn ping_history_retains_ten_real_measurements_without_keeping_stale_current_values() {
        let mut d = Diagnostics::default();
        for i in 0..15 {
            d.record_rtt(at(i as f64 * 4.), at(0.1));
        }
        let s = d.snapshot(at(100.));
        assert_eq!(s.network_samples.len(), 10);
        assert_eq!(s.network_samples[0].elapsed_seconds, 20.);
        assert_eq!(s.network_samples[9].elapsed_seconds, 56.);
        assert_eq!(s.network_rtt_ms, None);
        assert!(s.issues.is_empty());
    }

    #[test]
    fn send_duration_is_optional_and_expires() {
        let mut d = Diagnostics::default();
        d.record_send(at(1.), at(0.03));
        assert_eq!(d.snapshot(at(1.)).send_duration_ms, Some(30.));
        assert_eq!(d.snapshot(at(12.)).send_duration_ms, None);
    }
}
