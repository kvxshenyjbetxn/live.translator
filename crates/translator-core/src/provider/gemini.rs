use super::{
    audio_message, parse_events, setup, ConnectionReport, NetworkStage, ProviderError,
    TranslationConfig, TranslationEvent, TransportFailure,
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use std::{
    collections::VecDeque,
    future::Future,
    time::{Duration, Instant},
};
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Error, Message},
    MaybeTlsStream, WebSocketStream,
};
use tokio_util::sync::CancellationToken;
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct GeminiClient {
    socket: Socket,
    cancel: CancellationToken,
}
pub struct GeminiSender {
    sink: SplitSink<Socket, Message>,
    cancel: CancellationToken,
}
pub struct GeminiReceiver {
    stream: SplitStream<Socket>,
    pending: VecDeque<TranslationEvent>,
    cancel: CancellationToken,
}

async fn setup_guard<T>(
    future: impl Future<Output = Result<T, ProviderError>>,
    cancel: &CancellationToken,
) -> Result<T, ProviderError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(ProviderError::Cancelled),
        result = tokio::time::timeout(Duration::from_secs(15), future) => result.map_err(|_| ProviderError::Timeout)?,
    }
}
impl GeminiClient {
    pub async fn connect(
        config: TranslationConfig,
        cancel: CancellationToken,
    ) -> Result<Self, ProviderError> {
        if cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        if config.api_key.is_empty()
            || config.api_key.len() > 4096
            || !config
                .api_key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            || !crate::settings::LANGUAGES
                .iter()
                .any(|language| language.code == config.target_language)
        {
            return Err(ProviderError::Credentials);
        }
        let url = format!("wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent?key={}", config.api_key);
        Self::connect_at(config, cancel, &url).await
    }
    async fn connect_at(
        config: TranslationConfig,
        cancel: CancellationToken,
        url: &str,
    ) -> Result<Self, ProviderError> {
        let socket = setup_guard(
            async {
                let websocket_config = WebSocketConfig::default()
                    .max_message_size(Some(1048576))
                    .max_frame_size(Some(1048576));
                let (mut socket, _) = connect_async_with_config(url, Some(websocket_config), true)
                    .await
                    .map_err(classify_connect)?;
                socket
                    .send(Message::Text(setup(&config).to_string().into()))
                    .await
                    .map_err(|error| transport_error(error, NetworkStage::SetupSend))?;
                loop {
                    let message = socket
                        .next()
                        .await
                        .ok_or(ProviderError::Closed)?
                        .map_err(|error| transport_error(error, NetworkStage::SetupReceive))?;
                    match message {
                        message @ (Message::Text(_) | Message::Binary(_)) => {
                            let events = parse_events(&message.into_data())?;
                            if events.contains(&TranslationEvent::Ready) {
                                return Ok(socket);
                            }
                        }
                        Message::Close(frame) => {
                            return Err(classify_close(frame.map(|frame| u16::from(frame.code))))
                        }
                        _ => {
                            socket
                                .flush()
                                .await
                                .map_err(|error| transport_error(error, NetworkStage::SetupSend))?;
                        }
                    }
                }
            },
            &cancel,
        )
        .await?;
        Ok(Self { socket, cancel })
    }
    pub async fn check_connection(
        config: TranslationConfig,
        cancel: CancellationToken,
    ) -> Result<ConnectionReport, ProviderError> {
        let started = Instant::now();
        Self::connect(config, cancel)
            .await?
            .finish_check(started)
            .await
    }
    async fn finish_check(mut self, started: Instant) -> Result<ConnectionReport, ProviderError> {
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(ProviderError::Cancelled),
            result = tokio::time::timeout(Duration::from_secs(2), self.socket.close(None)) => {
                result.map_err(|_| ProviderError::Transport { stage: NetworkStage::Close, reason: TransportFailure::Timeout })?
                    .map_err(|error| transport_error(error, NetworkStage::Close))?;
            }
        }
        Ok(ConnectionReport {
            model: "gemini-3.5-live-translate-preview",
            elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        })
    }
    pub fn split(self) -> (GeminiSender, GeminiReceiver) {
        let (sink, stream) = self.socket.split();
        (
            GeminiSender {
                sink,
                cancel: self.cancel.clone(),
            },
            GeminiReceiver {
                stream,
                pending: VecDeque::new(),
                cancel: self.cancel,
            },
        )
    }
}
fn classify_connect(error: Error) -> ProviderError {
    match error {
        Error::Http(response) => match response.status().as_u16() {
            400 | 401 | 403 => ProviderError::Credentials,
            429 => ProviderError::Quota,
            code if code >= 500 => ProviderError::ServiceUnavailable { code },
            _ => ProviderError::Network,
        },
        error => transport_error(error, NetworkStage::Connect),
    }
}
fn classify_close(code: Option<u16>) -> ProviderError {
    match code {
        Some(1007) => ProviderError::Protocol,
        Some(1008) => ProviderError::Credentials,
        Some(code @ (1011 | 1013)) => ProviderError::ServiceUnavailable { code },
        _ => ProviderError::Closed,
    }
}
fn transport_error(error: Error, stage: NetworkStage) -> ProviderError {
    let reason = match error {
        Error::Io(error) => match error.kind() {
            std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::BrokenPipe => TransportFailure::Reset,
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                TransportFailure::Timeout
            }
            _ => TransportFailure::Unavailable,
        },
        Error::Tls(_) => TransportFailure::Tls,
        Error::Protocol(
            tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
        ) => TransportFailure::Reset,
        Error::ConnectionClosed | Error::AlreadyClosed => TransportFailure::Ended,
        _ => TransportFailure::Protocol,
    };
    ProviderError::Transport { stage, reason }
}
impl GeminiSender {
    pub async fn send_audio(&mut self, pcm: &[i16]) -> Result<(), ProviderError> {
        if pcm.len() != 1600 {
            return Err(ProviderError::Protocol);
        }
        tokio::select! {
            biased;
            _ = self.cancel.cancelled() => Err(ProviderError::Cancelled),
            result = tokio::time::timeout(Duration::from_secs(2), self.sink.send(Message::Text(audio_message(pcm).to_string().into()))) => result.map_err(|_| ProviderError::Transport { stage: NetworkStage::Send, reason: TransportFailure::Timeout })?.map_err(|error| transport_error(error, NetworkStage::Send)),
        }
    }
}
impl GeminiReceiver {
    pub async fn next_event(&mut self) -> Result<TranslationEvent, ProviderError> {
        loop {
            if self.cancel.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            if let Some(event) = self.pending.pop_front() {
                return Ok(event);
            }
            let message = tokio::select! {
                biased;
                _ = self.cancel.cancelled() => return Err(ProviderError::Cancelled),
                message = self.stream.next() => message.ok_or(ProviderError::Closed)?.map_err(|error| transport_error(error, NetworkStage::Receive))?,
            };
            match message {
                message @ (Message::Text(_) | Message::Binary(_)) => {
                    self.pending.extend(parse_events(&message.into_data())?)
                }
                Message::Close(frame) => {
                    return Err(classify_close(frame.map(|frame| u16::from(frame.code))))
                }
                _ => {} // Tungstenite queues control responses, flushed by the periodic sender.
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn service_busy_is_not_misreported_as_exhausted_quota() {
        assert_eq!(
            classify_close(Some(1013)),
            ProviderError::ServiceUnavailable { code: 1013 }
        );
        assert_eq!(
            classify_close(Some(1011)),
            ProviderError::ServiceUnavailable { code: 1011 }
        );
        let response = tokio_tungstenite::tungstenite::http::Response::builder()
            .status(503)
            .body(None)
            .unwrap();
        assert_eq!(
            classify_connect(Error::Http(Box::new(response))),
            ProviderError::ServiceUnavailable { code: 503 }
        );
    }
    #[tokio::test]
    async fn connection_check_closes_session_without_sending_audio() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let setup = socket.next().await.unwrap().unwrap();
            let value: serde_json::Value = serde_json::from_slice(&setup.into_data()).unwrap();
            assert!(value.get("setup").is_some());
            socket
                .send(Message::Binary(br#"{"setupComplete":{}}"#.to_vec().into()))
                .await
                .unwrap();
            assert!(matches!(socket.next().await, Some(Ok(Message::Close(_)))));
        });
        let client = GeminiClient::connect_at(
            TranslationConfig {
                api_key: "test".into(),
                target_language: "uk".into(),
            },
            CancellationToken::new(),
            &url,
        )
        .await
        .unwrap();
        let report = client.finish_check(Instant::now()).await.unwrap();
        assert_eq!(report.model, "gemini-3.5-live-translate-preview");
        server.await.unwrap();
    }
    #[tokio::test]
    async fn cancelled_before_connect_makes_no_request() {
        let token = CancellationToken::new();
        token.cancel();
        let result = GeminiClient::connect(
            TranslationConfig {
                api_key: "test".into(),
                target_language: "uk".into(),
            },
            token,
        )
        .await;
        assert!(matches!(result, Err(ProviderError::Cancelled)));
    }
    #[tokio::test]
    async fn setup_timeout_and_cancellation() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(matches!(
            setup_guard(std::future::pending::<Result<(), ProviderError>>(), &cancel).await,
            Err(ProviderError::Cancelled)
        ));
        tokio::time::pause();
        let result = setup_guard(
            std::future::pending::<Result<(), ProviderError>>(),
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(result, Err(ProviderError::Timeout));
    }
    #[tokio::test]
    async fn real_local_websocket_accepts_binary_and_cancelled_read() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let message = socket.next().await.unwrap().unwrap();
            assert!(String::from_utf8_lossy(&message.into_data()).contains("targetLanguageCode"));
            socket
                .send(Message::Binary(br#"{"setupComplete":{}}"#.to_vec().into()))
                .await
                .unwrap();
            let audio = socket.next().await.unwrap().unwrap();
            let value: serde_json::Value = serde_json::from_slice(&audio.into_data()).unwrap();
            assert_eq!(
                value["realtimeInput"]["audio"]["mimeType"],
                "audio/pcm;rate=16000"
            );
            socket.send(Message::Binary(br#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"AAA="}}]}}}"#.to_vec().into())).await.unwrap();
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let cancel = CancellationToken::new();
        let client = GeminiClient::connect_at(
            TranslationConfig {
                api_key: "test".into(),
                target_language: "uk".into(),
            },
            cancel.clone(),
            &url,
        )
        .await
        .unwrap();
        let (mut sender, mut receiver) = client.split();
        sender.send_audio(&[0; 1600]).await.unwrap();
        assert_eq!(
            receiver.next_event().await.unwrap(),
            TranslationEvent::Audio(vec![0])
        );
        cancel.cancel();
        assert_eq!(receiver.next_event().await, Err(ProviderError::Cancelled));
        server.abort();
    }
}
