use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use respondent_lib::asr::client::{AsrError, AsrEvent, StreamingAsrClient};
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

    fn queue(&mut self, value: Value) {
        self.recv.push_back(value);
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
fn finalize_flushes_pending_audio_before_commit() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        OpenAiRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");

    client
        .push_frame(&mono_16k_frame((0..320).collect(), 100))
        .expect("push frame");
    client.finalize().expect("finalize");

    let sent = handle.sent();
    let event_types = sent
        .iter()
        .map(|message| message["type"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();

    assert_eq!(
        event_types,
        vec![
            "session.update",
            "input_audio_buffer.append",
            "input_audio_buffer.commit",
        ]
    );

    let audio = sent[1]["audio"].as_str().expect("tail audio base64");
    let bytes = STANDARD.decode(audio).expect("valid base64");
    assert!(!bytes.is_empty());
    assert!(bytes.len() < 960);
}

#[test]
fn finalize_commits_without_requiring_a_final() {
    let (handle, transport) = RecordingTransport::new();
    let mut client =
        OpenAiRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");

    client.finalize().expect("commit");

    assert!(handle
        .sent()
        .iter()
        .any(|message| message["type"] == "input_audio_buffer.commit"));
}

#[test]
fn delta_accumulates_into_partial() {
    let (_, mut transport) = RecordingTransport::new();
    transport.queue(json!({
        "type": "conversation.item.input_audio_transcription.delta",
        "item_id": "item_1",
        "delta": "Hello",
    }));
    transport.queue(json!({
        "type": "conversation.item.input_audio_transcription.delta",
        "item_id": "item_1",
        "delta": ", world",
    }));
    let mut client =
        OpenAiRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");
    let events = client.events();

    client
        .push_frame(&mono_16k_frame(vec![0; 320], 100))
        .expect("push frame");

    match events.try_recv().expect("first partial") {
        AsrEvent::Partial { text, .. } => assert_eq!(text, "Hello"),
        other => panic!("expected partial, got {other:?}"),
    }
    match events.try_recv().expect("second partial") {
        AsrEvent::Partial { text, .. } => assert_eq!(text, "Hello, world"),
        other => panic!("expected partial, got {other:?}"),
    }
}

#[test]
fn completed_emits_final_and_clears_buffer() {
    let (_, mut transport) = RecordingTransport::new();
    transport.queue(json!({
        "type": "conversation.item.input_audio_transcription.delta",
        "item_id": "item_1",
        "delta": "draft",
    }));
    transport.queue(json!({
        "type": "conversation.item.input_audio_transcription.completed",
        "item_id": "item_1",
        "transcript": "final text",
    }));
    transport.queue(json!({
        "type": "conversation.item.input_audio_transcription.delta",
        "item_id": "item_1",
        "delta": "new",
    }));
    let mut client =
        OpenAiRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");
    let events = client.events();

    client
        .push_frame(&mono_16k_frame(vec![0; 320], 100))
        .expect("push frame");

    let collected = events.try_iter().collect::<Vec<_>>();
    assert!(collected
        .iter()
        .any(|event| matches!(event, AsrEvent::Final { text, .. } if text == "final text")));
    assert!(collected
        .iter()
        .any(|event| matches!(event, AsrEvent::Partial { text, .. } if text == "new")));
    assert!(!collected
        .iter()
        .any(|event| matches!(event, AsrEvent::Partial { text, .. } if text.contains("draftnew"))));
}

#[test]
fn provider_error_event_returns_provider_error_without_secret() {
    let (_, mut transport) = RecordingTransport::new();
    transport.queue(json!({
        "type": "error",
        "error": {
            "message": "bad audio",
        },
    }));
    let mut client =
        OpenAiRealtimeAsrClient::with_transport("s1".to_string(), config(), Box::new(transport))
            .expect("client");

    let err = client
        .push_frame(&mono_16k_frame(vec![0; 320], 100))
        .expect_err("provider error");

    let message = err.to_string();
    assert!(message.contains("bad audio"));
    assert!(!message.contains("test-key"));
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
