use std::sync::Arc;

use crossbeam_channel::{unbounded, Receiver, Sender};
use serde_json::Value;

use crate::audio::frame::AudioFrame;

use super::client::{AsrError, AsrEvent, StreamingAsrClient};

const TARGET_RATE: u32 = 16_000;

#[derive(Clone)]
pub struct SiliconFlowFileConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub trait TranscriptionTransport: Send + Sync {
    fn transcribe(&self, config: &SiliconFlowFileConfig, wav: &[u8]) -> Result<String, AsrError>;
}

fn truncate(text: &str) -> String {
    let t = text.trim();
    if t.len() <= 240 {
        return t.to_string();
    }
    let b = t
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|i| *i <= 240)
        .last()
        .unwrap_or(0);
    format!("{}...", &t[..b])
}

pub struct ReqwestTranscriptionTransport {
    client: reqwest::blocking::Client,
}
/// Bound the connect phase and the whole upload+transcription request. Without
/// these a hung upload would block the ASR thread forever and freeze session
/// teardown (`end_session` joins that thread).
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);
const TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

impl Default for ReqwestTranscriptionTransport {
    fn default() -> Self {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(TOTAL_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        Self { client }
    }
}
impl TranscriptionTransport for ReqwestTranscriptionTransport {
    fn transcribe(&self, config: &SiliconFlowFileConfig, wav: &[u8]) -> Result<String, AsrError> {
        let part = reqwest::blocking::multipart::Part::bytes(wav.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| AsrError::Provider(format!("transcription mime: {e}")))?;
        let form = reqwest::blocking::multipart::Form::new()
            .text("model", config.model.clone())
            .part("file", part);
        let response = self
            .client
            .post(join_transcriptions_url(&config.base_url))
            .bearer_auth(&config.api_key)
            .multipart(form)
            .send()
            .map_err(|e| AsrError::Provider(format!("transcription request: {e}")))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            return Err(AsrError::Provider(format!(
                "transcription http {status}: {}",
                truncate(&body)
            )));
        }
        let value: Value = response
            .json()
            .map_err(|e| AsrError::Provider(format!("transcription json: {e}")))?;
        Ok(value["text"].as_str().unwrap_or("").to_string())
    }
}

pub struct SiliconFlowFileAsrClient {
    session_id: String,
    config: SiliconFlowFileConfig,
    transport: Arc<dyn TranscriptionTransport>,
    sender: Sender<AsrEvent>,
    receiver: Receiver<AsrEvent>,
    buffer: Vec<i16>,
    started_at_ms: Option<i64>,
    last_ended_at_ms: i64,
}

impl SiliconFlowFileAsrClient {
    pub fn connect(session_id: String, config: SiliconFlowFileConfig) -> Result<Self, AsrError> {
        Self::with_transport(
            session_id,
            config,
            Arc::new(ReqwestTranscriptionTransport::default()),
        )
    }

    pub fn with_transport(
        session_id: String,
        config: SiliconFlowFileConfig,
        transport: Arc<dyn TranscriptionTransport>,
    ) -> Result<Self, AsrError> {
        if config.api_key.trim().is_empty() {
            return Err(AsrError::Provider("missing SiliconFlow API key".into()));
        }
        if config.base_url.trim().is_empty() {
            return Err(AsrError::Provider("missing SiliconFlow base_url".into()));
        }
        if config.model.trim().is_empty() {
            return Err(AsrError::Provider("missing SiliconFlow ASR model".into()));
        }
        let (sender, receiver) = unbounded();
        Ok(Self {
            session_id,
            config,
            transport,
            sender,
            receiver,
            buffer: Vec::new(),
            started_at_ms: None,
            last_ended_at_ms: 0,
        })
    }
}

impl StreamingAsrClient for SiliconFlowFileAsrClient {
    fn name(&self) -> &'static str {
        "siliconflow-file-asr"
    }

    fn push_frame(&mut self, frame: &AudioFrame) -> Result<(), AsrError> {
        if self.started_at_ms.is_none() {
            self.started_at_ms = Some(frame.captured_at_ms as i64);
        }
        self.last_ended_at_ms = frame.captured_at_ms as i64 + frame.duration_ms() as i64;
        self.buffer.extend_from_slice(&frame.samples);
        Ok(())
    }

    fn events(&self) -> Receiver<AsrEvent> {
        self.receiver.clone()
    }

    fn finalize(&mut self) -> Result<(), AsrError> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let started_at_ms = self.started_at_ms.unwrap_or(0);
        let ended_at_ms = self.last_ended_at_ms;
        eprintln!("[sf-asr] finalize: {} samples", self.buffer.len());
        let wav = encode_wav_pcm16_mono(&self.buffer, TARGET_RATE);
        self.buffer.clear();
        self.started_at_ms = None;

        match self.transport.transcribe(&self.config, &wav) {
            Ok(text) if !text.trim().is_empty() => {
                eprintln!("[sf-asr] transcript: {text:?}");
                let _ = self.sender.send(AsrEvent::Final {
                    session_id: self.session_id.clone(),
                    text,
                    started_at_ms,
                    ended_at_ms,
                    received_at_ms: ended_at_ms,
                });
            }
            Ok(_) => {} // empty transcript -> silent segment
            Err(error) => {
                // One failed segment must not end the session.
                eprintln!("siliconflow transcription failed: {error}");
            }
        }
        Ok(())
    }
}

/// Encode 16-bit mono PCM samples as an in-memory canonical WAV (44-byte
/// header + little-endian i16 data).
pub fn encode_wav_pcm16_mono(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

pub fn join_transcriptions_url(base_url: &str) -> String {
    format!("{}/audio/transcriptions", base_url.trim_end_matches('/'))
}
