use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use respondent_lib::asr::client::{AsrError, StreamingAsrClient};
use respondent_lib::asr::openai_realtime::{
    OpenAiRealtimeAsrClient, OpenAiRealtimeConfig, RealtimeTransport, TranscriptionDelay,
};
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};
use serde_json::{json, Value};

#[derive(Clone, Default)]
struct RecordingHandle {
    sent: Arc<Mutex<Vec<Value>>>,
}

struct RecordingTransport {
    sent: Arc<Mutex<Vec<Value>>>,
    recv: VecDeque<Value>,
}

impl RecordingTransport {
    fn new() -> (RecordingHandle, Self) {
        let handle = RecordingHandle::default();
        (
            handle.clone(),
            Self {
                sent: handle.sent,
                recv: VecDeque::new(),
            },
        )
    }
}

impl RecordingHandle {
    fn sent(&self) -> Vec<Value> {
        self.sent.lock().expect("sent lock").clone()
    }
}

impl RealtimeTransport for RecordingTransport {
    fn send_json(&mut self, value: Value) -> Result<(), AsrError> {
        self.sent.lock().expect("sent lock").push(value);
        Ok(())
    }

    fn try_recv_json(&mut self) -> Result<Option<Value>, AsrError> {
        Ok(self.recv.pop_front())
    }

    fn close(&mut self) -> Result<(), AsrError> {
        Ok(())
    }
}

fn config() -> OpenAiRealtimeConfig {
    OpenAiRealtimeConfig {
        api_key: "test-key".to_string(),
        model: "gpt-realtime-whisper".to_string(),
        language: Some("en".to_string()),
        transcription_delay: TranscriptionDelay::Minimal,
    }
}

fn mono_16k_frame(samples: Vec<i16>, captured_at_ms: u64) -> AudioFrame {
    AudioFrame {
        format: PcmFormat {
            sample_rate: 16_000,
            channels: 1,
            bits_per_sample: 16,
        },
        samples,
        captured_at_ms,
    }
}

#[test]
fn new_sends_transcription_session_update() {
    let (handle, transport) = RecordingTransport::new();

    let client =
        OpenAiRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");

    assert_eq!(client.name(), "openai-realtime-asr");

    let sent = handle.sent();
    assert_eq!(sent.len(), 1);

    let update = &sent[0];
    assert_eq!(update["type"], "session.update");
    assert_eq!(update["session"]["type"], "transcription");
    assert_eq!(
        update["session"]["audio"]["input"]["format"],
        json!({"type": "audio/pcm", "rate": 24000})
    );
    assert_eq!(
        update["session"]["audio"]["input"]["transcription"]["model"],
        "gpt-realtime-whisper"
    );
    assert_eq!(
        update["session"]["audio"]["input"]["transcription"]["language"],
        "en"
    );
    assert_eq!(
        update["session"]["audio"]["input"]["transcription"]["delay"],
        "minimal"
    );
    assert!(update["session"]["audio"]["input"]["turn_detection"].is_null());
}

fn append_messages(sent: &[Value]) -> Vec<&Value> {
    sent.iter()
        .filter(|message| message["type"] == "input_audio_buffer.append")
        .collect()
}

#[test]
fn two_frames_append_one_continuous_24khz_pcm_chunk() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        OpenAiRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");

    client
        .push_frame(&mono_16k_frame((0..320).collect(), 100))
        .expect("push first frame");
    assert!(
        append_messages(&handle.sent()).is_empty(),
        "first frame should not be padded into a synthetic 24 kHz chunk"
    );

    client
        .push_frame(&mono_16k_frame((320..640).collect(), 120))
        .expect("push second frame");

    let sent = handle.sent();
    let appends = append_messages(&sent);
    assert_eq!(appends.len(), 1);
    let append = appends[0];
    let audio = append["audio"].as_str().expect("audio base64");
    let bytes = STANDARD.decode(audio).expect("valid base64");

    assert_eq!(bytes.len(), 960);
    assert_eq!(i16::from_le_bytes([bytes[0], bytes[1]]), 0);
    assert_eq!(i16::from_le_bytes([bytes[2], bytes[3]]), 1);
}

#[test]
fn wrong_frame_format_is_rejected_without_append() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        OpenAiRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");
    let frame = AudioFrame {
        format: PcmFormat {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        },
        samples: vec![0; 960],
        captured_at_ms: 100,
    };

    let err = client
        .push_frame(&frame)
        .expect_err("wrong format should be rejected");

    assert!(err.to_string().contains("expects 16 kHz mono i16 frames"));
    assert!(!handle
        .sent()
        .iter()
        .any(|message| message["type"] == "input_audio_buffer.append"));
}

#[test]
fn non_20ms_frame_is_rejected_without_append() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        OpenAiRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");

    let err = client
        .push_frame(&mono_16k_frame(vec![0; 319], 100))
        .expect_err("short frames should be rejected");

    assert!(err.to_string().contains("20 ms frames"));
    assert!(append_messages(&handle.sent()).is_empty());
}

#[test]
fn default_config_uses_low_latency_model_and_delay() {
    let config = OpenAiRealtimeConfig::from_api_key("k");

    assert_eq!(config.model, "gpt-realtime-whisper");
    assert_eq!(config.language, None);
    assert_eq!(config.transcription_delay, TranscriptionDelay::Minimal);
}

#[test]
fn whitespace_api_key_is_rejected() {
    let (_, transport) = RecordingTransport::new();

    let result = OpenAiRealtimeAsrClient::with_transport(
        "s1".to_string(),
        OpenAiRealtimeConfig::from_api_key("   "),
        Box::new(transport),
    );

    match result {
        Err(AsrError::Provider(message)) => assert_eq!(message, "missing OPENAI_API_KEY"),
        Err(other) => panic!("expected provider error, got {other:?}"),
        Ok(_) => panic!("blank keys should be rejected"),
    }
}
