//! Disposable macOS -> Gemini -> speaker test, with a hard 20-second capture limit.
#[cfg(not(target_os = "macos"))]
compile_error!(
    "This spike is macOS-only. The product remains Rust + Tauri on macOS/Windows/Linux."
);
mod audio;
mod capture;
use anyhow::{bail, ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tungstenite::{connect, stream::MaybeTlsStream, Error, Message, WebSocket};

fn tcp(socket: &WebSocket<MaybeTlsStream<TcpStream>>) -> &TcpStream {
    match socket.get_ref() {
        MaybeTlsStream::Plain(stream) => stream,
        MaybeTlsStream::Rustls(stream) => stream.get_ref(),
        _ => unreachable!("only Rustls and plain TCP enabled"),
    }
}
fn parse(message: Message) -> Result<Option<Value>> {
    match message {
        message @ (Message::Text(_) | Message::Binary(_)) => {
            Ok(Some(serde_json::from_slice(&message.into_data())?))
        }
        Message::Close(frame) => bail!(
            "Server closed WebSocket (code {:?})",
            frame.map(|f| u16::from(f.code))
        ),
        _ => Ok(None),
    }
}

fn run(key: &str, pid: i32, original_gain: f32, stop: Arc<AtomicBool>) -> Result<()> {
    let url = format!("wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={key}");
    let (mut socket, _) = connect(url.as_str())
        .map_err(|_| anyhow::anyhow!("WebSocket connection failed; URL suppressed"))?;
    tcp(&socket).set_read_timeout(Some(Duration::from_secs(10)))?;
    tcp(&socket).set_write_timeout(Some(Duration::from_secs(2)))?;
    socket.send(Message::Text(
        json!({"setup": {
            "model": "models/gemini-3.5-live-translate-preview",
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "translationConfig": {"targetLanguageCode": "uk", "echoTargetLanguage": false}
            }
        }})
        .to_string()
        .into(),
    ))?;
    let setup_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        ensure!(
            Instant::now() < setup_deadline && !stop.load(Ordering::Relaxed),
            "Setup timed out or cancelled"
        );
        if let Some(value) = parse(socket.read()?)? {
            if value.get("error").is_some() {
                bail!("Server rejected setup; use connection probe for diagnostics");
            }
            if value.get("setupComplete").is_some() || value.get("setup_complete").is_some() {
                break;
            }
        }
    }
    println!("Session ready. Starting 20-second test: original {:.0}%, translation 70%. Native original muted, captured original mixed locally. Ctrl+C stops.", original_gain * 100.);
    tcp(&socket).set_read_timeout(Some(Duration::from_millis(10)))?;
    let failed = Arc::new(AtomicBool::new(false));
    let played = Arc::new(AtomicU64::new(0));
    let (capture_tx, mut capture_rx) = rtrb::RingBuffer::new(48000);
    let (original_tx, original_rx) = rtrb::RingBuffer::new(48000);
    let (mut playback_tx, playback_rx) = rtrb::RingBuffer::new(24000 * 5);
    // Output must work before source is muted. RAII restores the original on all returns.
    let player = audio::playback(
        playback_rx,
        original_rx,
        original_gain,
        failed.clone(),
        played.clone(),
    )?;
    let capture = capture::Capture::start(pid, capture_tx, original_tx, failed.clone())?;
    let started = Instant::now();
    let mut filter = audio::Downsample::new();
    let mut chunk = Vec::with_capacity(3200);
    let mut sent_samples = 0usize;
    let mut received_samples = 0usize;
    let mut nonzero_input = 0usize;
    let mut first_audio = false;
    let mut last_report = 0;
    while started.elapsed() < Duration::from_secs(20) && !stop.load(Ordering::Relaxed) {
        ensure!(
            !failed.load(Ordering::Relaxed),
            "Audio callback error, unexpected format, or capture queue overflow; stopping"
        );
        // Limit work per iteration so playback/receive cannot starve behind input.
        for _ in 0..4800 {
            let Ok(sample) = capture_rx.pop() else {
                break;
            };
            if sample.abs() > 0.00001 {
                nonzero_input += 1;
            }
            if let Some(sample) = filter.push(sample) {
                chunk.extend_from_slice(&sample.to_le_bytes());
                if chunk.len() == 3200 {
                    socket.send(Message::Text(
                        json!({"realtimeInput": {"audio": {
                            "data": STANDARD.encode(&chunk), "mimeType": "audio/pcm;rate=16000"
                        }}})
                        .to_string()
                        .into(),
                    ))?;
                    sent_samples += 1600;
                    chunk.clear();
                }
            }
        }
        let message = match socket.read() {
            Ok(message) => Some(message),
            Err(Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                None
            }
            Err(_) => bail!("WebSocket read failed; stopping"),
        };
        if let Some(message) = message {
            if let Some(value) = parse(message)? {
                if value.get("error").is_some() {
                    bail!("Server error during translation; raw response suppressed");
                }
                if let Some(content) = value.get("serverContent") {
                    if content.get("interrupted").and_then(Value::as_bool) == Some(true) {
                        bail!("Server interrupted translation; stopping rather than playing stale audio");
                    }
                    if let Some(parts) = content
                        .pointer("/modelTurn/parts")
                        .and_then(Value::as_array)
                    {
                        for part in parts {
                            if let Some(inline) = part.get("inlineData") {
                                let mime =
                                    inline.get("mimeType").and_then(Value::as_str).unwrap_or("");
                                ensure!(
                                    mime.starts_with("audio/pcm") && mime.contains("rate=24000"),
                                    "Unexpected output audio format: {mime}"
                                );
                                let data = inline
                                    .get("data")
                                    .and_then(Value::as_str)
                                    .context("Missing audio payload")?;
                                let bytes = STANDARD.decode(data)?;
                                if !bytes.is_empty() && !first_audio {
                                    println!("First translated audio received at {:.2}s from capture start (not a semantic latency measurement).", started.elapsed().as_secs_f64());
                                    first_audio = true;
                                }
                                received_samples += audio::enqueue_pcm(&bytes, &mut playback_tx)?;
                            }
                        }
                    }
                }
            }
        }
        let elapsed = started.elapsed().as_secs();
        if elapsed >= last_report + 2 {
            println!(
                "{elapsed}s: sent {:.1}s, received {:.1}s, played ~{:.1}s, playback queue {:.2}s",
                sent_samples as f64 / 16000.,
                received_samples as f64 / 24000.,
                played.load(Ordering::Relaxed) as f64 / 24000.,
                (120000 - playback_tx.slots()) as f64 / 24000.
            );
            last_report = elapsed;
        }
    }
    // Hard stop: do not keep source muted or leave queued translation playing.
    drop(player);
    drop(capture);
    let _ = socket.close(None);
    println!("Stopped. Original audio restored by tap removal. Sent {:.1}s, received {:.1}s; audio not saved. Any queued tail was intentionally cut at the test limit.", sent_samples as f64 / 16000., received_samples as f64 / 24000.);
    ensure!(nonzero_input > 0, "Captured only silence");
    ensure!(
        received_samples > 0,
        "No translated audio received during this short test"
    );
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(args.len() == 1 || (args.len() == 3 && args[1] == "--original-volume"), "Usage: live-translate-spike AUDIO_PID [--original-volume 0..1] (transmits audio to Google for 20 seconds)");
    let pid: i32 = args[0].parse()?;
    ensure!(pid > 0, "PID must be positive");
    let original_gain: f32 = if args.len() == 3 {
        args[2].parse()?
    } else {
        0.
    };
    ensure!(
        original_gain.is_finite() && (0.0..=1.0).contains(&original_gain),
        "Original volume must be 0..1"
    );
    let key = match std::env::var("GEMINI_API_KEY") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => rpassword::prompt_password("Gemini API key (hidden, not saved): ")?,
    };
    let key = key.trim();
    ensure!(
        !key.is_empty()
            && key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
        "Invalid key characters"
    );
    let stop = Arc::new(AtomicBool::new(false));
    let signal_stop = stop.clone();
    ctrlc::set_handler(move || signal_stop.store(true, Ordering::Relaxed))?;
    // All returned diagnostics scrub the literal key, including third-party errors.
    match run(key, pid, original_gain, stop) {
        Ok(()) => Ok(()),
        Err(error) => bail!("{}", error.to_string().replace(key, "[REDACTED]")),
    }
}
