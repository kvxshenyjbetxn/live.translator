pub mod gemini;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};

pub struct TranslationConfig {
    pub api_key: String,
    pub target_language: String,
}
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NetworkStage {
    #[error("підключення")]
    Connect,
    #[error("надсилання налаштувань")]
    SetupSend,
    #[error("підтвердження сесії")]
    SetupReceive,
    #[error("надсилання аудіо")]
    Send,
    #[error("отримання перекладу")]
    Receive,
    #[error("закриття перевірки")]
    Close,
}
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TransportFailure {
    #[error("TLS/сертифікат")]
    Tls,
    #[error("з'єднання скинуто сервером або мережею")]
    Reset,
    #[error("час очікування вичерпано")]
    Timeout,
    #[error("мережа недоступна")]
    Unavailable,
    #[error("порушення WebSocket-протоколу")]
    Protocol,
    #[error("канал закрито")]
    Ended,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionReport {
    pub model: &'static str,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProviderError {
    #[error("Google тимчасово не може обробити запит (код {code}). Це не обов'язково ліміт; повторіть пізніше.")]
    ServiceUnavailable { code: u16 },
    #[error("Збій з'єднання з Google: {stage} — {reason}. Спробуйте запустити знову.")]
    Transport {
        stage: NetworkStage,
        reason: TransportFailure,
    },
    #[error("Перевірте API-ключ та доступ проєкту до Gemini Live Translation.")]
    Credentials,
    #[error("Вичерпано квоту Gemini. Перевірте ліміти та запустіть знову пізніше.")]
    Quota,
    #[error("Сесію Gemini завершено. Натисніть «Почати», щоб відкрити нову.")]
    Closed,
    #[error("Некоректна або непідтримувана відповідь Gemini.")]
    Protocol,
    #[error("З'єднання з Google втрачено. Перевірте мережу й запустіть переклад знову.")]
    Network,
    #[error("Запуск скасовано.")]
    Cancelled,
    #[error("Google не відповів за 15 секунд. Спробуйте ще раз.")]
    Timeout,
}
#[derive(Debug, PartialEq)]
pub enum TranslationEvent {
    Ready,
    Audio(Vec<i16>),
}
pub fn setup(config: &TranslationConfig) -> Value {
    json!({"setup": {"model": "models/gemini-3.5-live-translate-preview", "generationConfig": {
        "responseModalities": ["AUDIO"], "translationConfig": {"targetLanguageCode": config.target_language, "echoTargetLanguage": false}
    }}})
}
pub fn parse_events(bytes: &[u8]) -> Result<Vec<TranslationEvent>, ProviderError> {
    if bytes.len() > 1048576 {
        return Err(ProviderError::Protocol);
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ProviderError::Protocol)?;
    if !value.is_object() {
        return Err(ProviderError::Protocol);
    }
    for field in [
        "error",
        "setupComplete",
        "setup_complete",
        "serverContent",
        "goAway",
    ] {
        if value
            .get(field)
            .is_some_and(|container| !container.is_object())
        {
            return Err(ProviderError::Protocol);
        }
    }
    if let Some(turn) = value.pointer("/serverContent/modelTurn") {
        if !turn.is_object() || !turn.get("parts").is_some_and(Value::is_array) {
            return Err(ProviderError::Protocol);
        }
    }
    if let Some(error) = value.get("error") {
        return Err(match error.get("code").and_then(Value::as_u64) {
            Some(429) => ProviderError::Quota,
            Some(400 | 401 | 403) => ProviderError::Credentials,
            Some(code @ 500..=599) => ProviderError::ServiceUnavailable { code: code as u16 },
            _ => ProviderError::Network,
        });
    }
    if value.get("goAway").is_some()
        || value
            .pointer("/serverContent/interrupted")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return Err(ProviderError::Closed);
    }
    let mut events = Vec::new();
    if value
        .get("setupComplete")
        .or_else(|| value.get("setup_complete"))
        .is_some_and(Value::is_object)
    {
        events.push(TranslationEvent::Ready);
    }
    if let Some(parts) = value.pointer("/serverContent/modelTurn/parts") {
        let parts = parts.as_array().ok_or(ProviderError::Protocol)?;
        for part in parts {
            if !part.is_object() {
                return Err(ProviderError::Protocol);
            }
            if let Some(inline) = part.get("inlineData") {
                let mime = inline
                    .get("mimeType")
                    .and_then(Value::as_str)
                    .ok_or(ProviderError::Protocol)?;
                let mut fields = mime.split(';').map(str::trim);
                if fields.next() != Some("audio/pcm") || !fields.any(|field| field == "rate=24000")
                {
                    return Err(ProviderError::Protocol);
                }
                let data = inline
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or(ProviderError::Protocol)?;
                let bytes = STANDARD.decode(data).map_err(|_| ProviderError::Protocol)?;
                if !bytes.len().is_multiple_of(2) {
                    return Err(ProviderError::Protocol);
                }
                if !bytes.is_empty() {
                    events.push(TranslationEvent::Audio(
                        bytes
                            .chunks_exact(2)
                            .map(|b| i16::from_le_bytes([b[0], b[1]]))
                            .collect(),
                    ));
                }
            }
        }
    }
    Ok(events)
}
pub fn audio_message(pcm: &[i16]) -> Value {
    let bytes: Vec<_> = pcm.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    json!({"realtimeInput": {"audio": {"data": STANDARD.encode(bytes), "mimeType": "audio/pcm;rate=16000"}}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_and_pcm_framing() {
        let config = TranslationConfig {
            api_key: "test-secret".into(),
            target_language: "uk".into(),
        };
        let setup = setup(&config);
        assert_eq!(
            setup["setup"]["model"],
            "models/gemini-3.5-live-translate-preview"
        );
        assert_eq!(
            setup["setup"]["generationConfig"]["translationConfig"]["targetLanguageCode"],
            "uk"
        );
        assert_eq!(
            setup["setup"]["generationConfig"]["translationConfig"]["echoTargetLanguage"],
            false
        );
        assert!(!setup.to_string().contains("test-secret"));
        assert_eq!(
            audio_message(&[0, -32768])["realtimeInput"]["audio"]["data"],
            "AAAAgA=="
        );
    }
    #[test]
    fn setup_and_binary_json_audio() {
        assert_eq!(
            parse_events(br#"{"setupComplete":{}}"#).unwrap(),
            vec![TranslationEvent::Ready]
        );
        let data = br#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"AAAAgA=="}}]}}}"#;
        assert_eq!(
            parse_events(data).unwrap(),
            vec![TranslationEvent::Audio(vec![0, -32768])]
        );
    }
    #[test]
    fn malformed_payloads_and_limits_rejected() {
        for bytes in [b"bad".as_slice(), br#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=240000","data":"AAA="}}]}}}"#.as_slice(), br#"{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"AA=="}}]}}}"#.as_slice()] {
            assert!(parse_events(bytes).is_err());
        }
        assert!(parse_events(&vec![b' '; 1048577]).is_err());
    }
    #[test]
    fn malformed_known_containers_do_not_keep_upload_alive() {
        for bytes in [
            br#"{"serverContent":{"modelTurn":{"parts":[null]}}}"#.as_slice(),
            br#"{"serverContent":{"modelTurn":5}}"#.as_slice(),
            br#"{"serverContent":false}"#.as_slice(),
            br#"{"setupComplete":null}"#.as_slice(),
        ] {
            assert_eq!(parse_events(bytes), Err(ProviderError::Protocol));
        }
        assert!(parse_events(br#"{"serverContent":{"turnComplete":true}}"#)
            .unwrap()
            .is_empty());
    }
    #[test]
    fn errors_expiry_and_no_secret_diagnostics() {
        assert_eq!(
            parse_events(br#"{"error":{"code":429,"message":"test-secret"}}"#),
            Err(ProviderError::Quota)
        );
        assert_eq!(
            parse_events(br#"{"goAway":{"timeLeft":"1s"}}"#),
            Err(ProviderError::Closed)
        );
        assert_eq!(
            parse_events(br#"{"serverContent":{"interrupted":true}}"#),
            Err(ProviderError::Closed)
        );
        assert!(
            !parse_events(br#"{"error":{"code":403,"message":"test-secret"}}"#)
                .unwrap_err()
                .to_string()
                .contains("test-secret")
        );
    }
}
