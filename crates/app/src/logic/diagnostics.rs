//! Latency diagnostics projection (port of `src/lib/diagnostics.ts` and the row derivation of
//! `Diagnostics.svelte`).

use super::i18n::{t, Locale};
use translator_core::session::diagnostics::{DiagnosticIssue, DiagnosticsSnapshot};

#[derive(Clone, Copy, Debug)]
pub struct Measurement {
    pub time: f64,
    pub value: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub points: Vec<Point>,
    /// Polylines of consecutive samples; a gap in sampling starts a new one.
    pub paths: Vec<Vec<Point>>,
}

/// Coordinates in a 100×30 view, using only actual observations in the last 10 s.
pub fn project_history(samples: &[Measurement], now: f64, maximum: f64, dots_only: bool) -> Graph {
    let mut graph = Graph::default();
    if !now.is_finite() || !maximum.is_finite() || maximum <= 0.0 {
        return graph;
    }
    let mut segment: Vec<Point> = Vec::new();
    let mut previous_time: Option<f64> = None;
    let flush = |segment: &mut Vec<Point>, previous: &mut Option<f64>, paths: &mut Vec<Vec<Point>>| {
        if !segment.is_empty() {
            paths.push(std::mem::take(segment));
        }
        *previous = None;
    };
    for sample in samples {
        let valid = sample.time.is_finite()
            && sample.time >= now - 10.0
            && sample.time <= now
            && sample.value.is_some_and(|value| value.is_finite() && value >= 0.0);
        let Some(value) = sample.value.filter(|_| valid) else {
            flush(&mut segment, &mut previous_time, &mut graph.paths);
            continue;
        };
        if previous_time.is_some_and(|previous| sample.time - previous > 0.4 || sample.time <= previous) {
            flush(&mut segment, &mut previous_time, &mut graph.paths);
        }
        let point = Point {
            x: (sample.time - now + 10.0) * 10.0,
            y: 30.0 * (1.0 - (value / maximum).min(1.0)),
        };
        graph.points.push(point);
        if !dots_only {
            segment.push(point);
        }
        previous_time = Some(sample.time);
    }
    flush(&mut segment, &mut previous_time, &mut graph.paths);
    graph
}

pub fn diagnostic_messages(diagnostics: &DiagnosticsSnapshot) -> Vec<&'static str> {
    let messages: Vec<&'static str> = diagnostics
        .issues
        .iter()
        .map(|issue| match issue {
            DiagnosticIssue::NetworkUnstable => "Відгук з’єднання з Google високий або нестабільний. Це може додавати затримку, але не доводить низьку швидкість інтернету.",
            DiagnosticIssue::SendingQueue => "Захоплене аудіо накопичується: надсилання не встигає за джерелом. Причиною може бути з’єднання або навантаження програми.",
            DiagnosticIssue::PlaybackQueue => "Отримана озвучка накопичується перед відтворенням. Програма плавно прискорює її, щоб зменшити чергу.",
        })
        .collect();
    if !messages.is_empty() {
        return messages;
    }
    let samples = &diagnostics.samples;
    let span = match (samples.first(), samples.last()) {
        (Some(first), Some(last)) => last.elapsed_seconds - first.elapsed_seconds,
        _ => 0.0,
    };
    if samples.len() < 2 || span < 2.0 {
        return vec!["Збираємо вимірювання. Пауза в озвучці сама по собі не означає несправність."];
    }
    vec!["Значного накопичення в локальних чергах не виявлено. Ці вимірювання не визначають час роботи моделі."]
}

/// `Number.toLocaleString` with `maximumFractionDigits`, for `uk-UA` and `en-US`.
pub fn format_number(value: f64, max_fraction_digits: usize, locale: Locale) -> String {
    let (group, decimal) = match locale {
        Locale::En => (",", "."),
        Locale::Uk => ("\u{a0}", ","),
    };
    let fixed = format!("{:.*}", max_fraction_digits, value.abs());
    let (whole, fraction) = fixed.split_once('.').unwrap_or((fixed.as_str(), ""));
    let fraction = fraction.trim_end_matches('0');
    let mut grouped = String::new();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push_str(group);
        }
        grouped.push(digit);
    }
    let negative = value < 0.0 && (grouped != "0" || !fraction.is_empty());
    let sign = if negative { "-" } else { "" };
    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}{decimal}{fraction}")
    }
}

pub fn format_measurement(value: f64, unit: &str, locale: Locale) -> String {
    let digits = if unit == "мс" { 0 } else { 2 };
    format!("{} {}", format_number(value, digits, locale), t(unit, locale))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RowColor {
    Fixed([u8; 3]),
    /// The theme focus colour.
    Focus,
}

#[derive(Clone, Debug)]
pub struct DiagnosticRow {
    pub label: &'static str,
    pub unit: &'static str,
    pub color: RowColor,
    pub is_network: bool,
    pub max: f64,
    pub graph: Graph,
    pub value: Option<f64>,
}

/// The three chart rows: Google ping, sending queue, playback queue.
pub fn diagnostic_rows(diagnostics: &DiagnosticsSnapshot) -> Vec<DiagnosticRow> {
    let now = diagnostics.elapsed_seconds;
    let fresh_queues = diagnostics
        .samples
        .last()
        .is_some_and(|last| now - last.elapsed_seconds <= 1.5);
    let queue_samples = |pick: fn(&translator_core::session::diagnostics::DiagnosticSample) -> f64| {
        diagnostics
            .samples
            .iter()
            .map(|s| Measurement {
                time: s.elapsed_seconds,
                value: Some(pick(s)),
            })
            .collect::<Vec<_>>()
    };
    let network_samples: Vec<Measurement> = diagnostics
        .network_samples
        .iter()
        .map(|s| Measurement {
            time: s.elapsed_seconds,
            value: Some(s.rtt_ms),
        })
        .collect();
    let configs = [
        ("Відгук Google", "мс", RowColor::Fixed([0x8e, 0xbc, 0xd2]), 1000.0, true, diagnostics.network_rtt_ms, network_samples),
        ("Черга надсилання", "с", RowColor::Focus, 1.0, false, fresh_queues.then_some(diagnostics.sending_queue_seconds), queue_samples(|s| s.sending_queue_seconds)),
        ("Черга озвучення", "с", RowColor::Fixed([0xe6, 0xbc, 0x80]), 3.0, false, fresh_queues.then_some(diagnostics.playback_queue_seconds), queue_samples(|s| s.playback_queue_seconds)),
    ];
    configs
        .into_iter()
        .map(|(label, unit, color, floor, is_network, value, samples)| {
            let visible: Vec<Measurement> = samples
                .iter()
                .copied()
                .filter(|s| {
                    s.value.is_some_and(f64::is_finite)
                        && (is_network || (s.time >= now - 10.0 && s.time <= now))
                })
                .collect();
            let visible = &visible[visible.len().saturating_sub(101)..];
            let retained = if is_network {
                &visible[visible.len().saturating_sub(10)..]
            } else {
                visible
            };
            let max = retained
                .iter()
                .filter_map(|s| s.value)
                .map(|v| (v / floor).ceil() * floor)
                .fold(floor, f64::max);
            let graph = if is_network {
                Graph {
                    paths: Vec::new(),
                    points: retained
                        .iter()
                        .enumerate()
                        .map(|(i, s)| Point {
                            x: (10.0 - retained.len() as f64 + i as f64 + 0.5) * 10.0,
                            y: 30.0 * (1.0 - (s.value.unwrap_or(0.0) / max).min(1.0)),
                        })
                        .collect(),
                }
            } else {
                project_history(&samples, now, max, false)
            };
            DiagnosticRow {
                label,
                unit,
                color,
                is_network,
                max,
                graph,
                value,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use translator_core::session::diagnostics::{DiagnosticSample, NetworkSample};

    fn at(time: f64, value: f64) -> Measurement {
        Measurement {
            time,
            value: Some(value),
        }
    }
    fn snapshot() -> DiagnosticsSnapshot {
        DiagnosticsSnapshot {
            elapsed_seconds: 12.0,
            ..DiagnosticsSnapshot::default()
        }
    }

    #[test]
    fn history_projects_real_timestamps_in_the_last_ten_seconds_and_clips_expired_values() {
        let samples = [at(1.0, 50.0), at(2.0, 20.0), at(12.0, 100.0), at(13.0, 10.0), at(8.0, f64::NAN)];
        let graph = project_history(&samples, 12.0, 100.0, true);
        assert_eq!(
            graph.points,
            [Point { x: 0.0, y: 24.0 }, Point { x: 100.0, y: 0.0 }]
        );
        assert!(graph.paths.is_empty(), "sparse RTT is dots only");
    }

    #[test]
    fn missing_measurements_and_sampling_gaps_split_paths_and_empty_graphs_stay_finite() {
        assert_eq!(project_history(&[], 10.0, 1.0, false), Graph::default());
        let samples = [
            at(9.0, 0.2),
            Measurement { time: 9.1, value: None },
            at(9.2, 0.3),
            at(10.0, 0.4),
        ];
        let graph = project_history(&samples, 10.0, 1.0, false);
        assert_eq!(graph.paths.len(), 3);
        assert_eq!(graph.points.len(), 3);
        assert!(graph.points.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    }

    #[test]
    fn multiple_causes_are_explained_without_treating_high_rtt_as_proven_slow_internet() {
        let mut diagnostics = snapshot();
        diagnostics.issues = vec![
            DiagnosticIssue::NetworkUnstable,
            DiagnosticIssue::SendingQueue,
            DiagnosticIssue::PlaybackQueue,
        ];
        let messages = diagnostic_messages(&diagnostics);
        assert_eq!(messages.len(), 3);
        let joined = messages.join(" ");
        assert!(joined.contains("може") && joined.contains("не встигає") && joined.contains("накопичується"));
        let calm = diagnostic_messages(&snapshot()).join(" ");
        assert!(!calm.contains("повільний інтернет") && !calm.contains("Google повільно"));
    }

    #[test]
    fn rows_report_unknown_network_and_scale_independently() {
        let rows = diagnostic_rows(&snapshot());
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.value.is_none() && row.graph.points.is_empty()));
        assert_eq!(rows.iter().map(|row| row.max).collect::<Vec<_>>(), [1000.0, 1.0, 3.0]);
    }

    #[test]
    fn each_real_ping_adds_a_packed_bar_and_bars_move_only_on_a_new_measurement() {
        let pings = |values: &[(f64, f64)]| -> Vec<NetworkSample> {
            values
                .iter()
                .map(|(elapsed_seconds, rtt_ms)| NetworkSample {
                    elapsed_seconds: *elapsed_seconds,
                    rtt_ms: *rtt_ms,
                })
                .collect()
        };
        let mut diagnostics = snapshot();
        diagnostics.network_rtt_ms = Some(200.0);
        diagnostics.network_samples = pings(&[(0.0, 900.0), (4.0, 100.0), (8.0, 300.0), (12.0, 200.0)]);
        diagnostics.samples = vec![DiagnosticSample {
            elapsed_seconds: 12.0,
            sending_queue_seconds: 0.2,
            playback_queue_seconds: 0.4,
        }];
        let before = diagnostic_rows(&diagnostics)[0].graph.points.clone();
        assert_eq!(before.len(), 4);
        assert_eq!(30.0 - before[1].y, 3.0);
        assert_eq!(before[1].x - before[0].x, 10.0);
        diagnostics.elapsed_seconds = 18.0;
        assert_eq!(diagnostic_rows(&diagnostics)[0].graph.points, before);
        diagnostics.elapsed_seconds = 20.0;
        diagnostics.network_samples = pings(&[(0.0, 900.0), (4.0, 100.0), (8.0, 300.0), (12.0, 200.0), (20.0, 100.0)]);
        let after = diagnostic_rows(&diagnostics)[0].graph.points.clone();
        assert_eq!(after.len(), 5);
        assert_eq!(after[0].x, before[0].x - 10.0);
    }

    #[test]
    fn measurements_use_locale_grouping_and_translated_units() {
        assert_eq!(format_measurement(1234.0, "мс", Locale::En), "1,234 ms");
        assert_eq!(format_measurement(1234.0, "мс", Locale::Uk), "1\u{a0}234 мс");
        assert_eq!(format_measurement(0.5, "с", Locale::Uk), "0,5 с");
        assert_eq!(format_measurement(3.14159, "с", Locale::En), "3.14 s");
    }
}
