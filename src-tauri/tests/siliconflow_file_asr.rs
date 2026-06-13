use respondent_lib::asr::siliconflow_file::{encode_wav_pcm16_mono, join_transcriptions_url};
use std::sync::{Arc, Mutex};

use respondent_lib::asr::client::{AsrError, AsrEvent, StreamingAsrClient};
use respondent_lib::asr::siliconflow_file::{
    SiliconFlowFileAsrClient, SiliconFlowFileConfig, TranscriptionTransport,
};
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};

fn config() -> SiliconFlowFileConfig {
    SiliconFlowFileConfig {
        base_url: "https://example.test/v1".into(),
        api_key: "secret-key".into(),
        model: "FunAudioLLM/SenseVoiceSmall".into(),
    }
}

fn frame(amplitude: i16, at_ms: u64) -> AudioFrame {
    AudioFrame {
        format: PcmFormat {
            sample_rate: 16_000,
            channels: 1,
            bits_per_sample: 16,
        },
        samples: vec![amplitude; 320],
        captured_at_ms: at_ms,
    }
}

struct FakeTransport {
    result: Mutex<Vec<Result<String, AsrError>>>,
    calls: Mutex<usize>,
}
impl FakeTransport {
    fn new(results: Vec<Result<String, AsrError>>) -> Self {
        Self {
            result: Mutex::new(results),
            calls: Mutex::new(0),
        }
    }
}
impl TranscriptionTransport for FakeTransport {
    fn transcribe(&self, _c: &SiliconFlowFileConfig, _wav: &[u8]) -> Result<String, AsrError> {
        *self.calls.lock().unwrap() += 1;
        let mut r = self.result.lock().unwrap();
        if r.is_empty() {
            Ok(String::new())
        } else {
            r.remove(0)
        }
    }
}

fn drain(events: &crossbeam_channel::Receiver<AsrEvent>) -> Vec<AsrEvent> {
    let mut out = Vec::new();
    while let Ok(e) = events.try_recv() {
        out.push(e);
    }
    out
}

#[test]
fn finalize_uploads_buffer_and_emits_final() {
    let mut client = SiliconFlowFileAsrClient::with_transport(
        "s1".into(),
        config(),
        Arc::new(FakeTransport::new(vec![Ok("hello world".into())])),
    )
    .expect("client");
    let events = client.events();
    client.push_frame(&frame(1000, 0)).unwrap();
    client.push_frame(&frame(1000, 20)).unwrap();
    client.finalize().unwrap();
    let drained = drain(&events);
    match drained.as_slice() {
        [AsrEvent::Final {
            session_id, text, ..
        }] => {
            assert_eq!(session_id, "s1");
            assert_eq!(text, "hello world");
        }
        other => panic!("expected one final, got {other:?}"),
    }
}

#[test]
fn finalize_without_frames_is_noop() {
    let mut client = SiliconFlowFileAsrClient::with_transport(
        "s1".into(),
        config(),
        Arc::new(FakeTransport::new(vec![Ok("x".into())])),
    )
    .unwrap();
    let events = client.events();
    client.finalize().unwrap();
    assert!(drain(&events).is_empty());
}

#[test]
fn empty_transcript_emits_no_final() {
    let mut client = SiliconFlowFileAsrClient::with_transport(
        "s1".into(),
        config(),
        Arc::new(FakeTransport::new(vec![Ok("".into())])),
    )
    .unwrap();
    let events = client.events();
    client.push_frame(&frame(1000, 0)).unwrap();
    client.finalize().unwrap();
    assert!(drain(&events).is_empty());
}

#[test]
fn transcription_error_does_not_end_session() {
    let mut client = SiliconFlowFileAsrClient::with_transport(
        "s1".into(),
        config(),
        Arc::new(FakeTransport::new(vec![
            Err(AsrError::Provider("boom".into())),
            Ok("second".into()),
        ])),
    )
    .unwrap();
    let events = client.events();
    client.push_frame(&frame(1000, 0)).unwrap();
    client.finalize().unwrap(); // error -> Ok, no final
    assert!(drain(&events).is_empty());
    // buffer cleared; next utterance works
    client.push_frame(&frame(1000, 40)).unwrap();
    client.finalize().unwrap();
    let drained = drain(&events);
    assert!(matches!(drained.as_slice(), [AsrEvent::Final { text, .. }] if text == "second"));
}

#[test]
fn rejects_empty_api_key() {
    let mut cfg = config();
    cfg.api_key = "".into();
    assert!(SiliconFlowFileAsrClient::with_transport(
        "s1".into(),
        cfg,
        Arc::new(FakeTransport::new(vec![]))
    )
    .is_err());
}

#[test]
fn wav_header_and_length_are_correct() {
    let samples = [0x0102i16, -1];
    let wav = encode_wav_pcm16_mono(&samples, 16_000);
    assert_eq!(wav.len(), 44 + samples.len() * 2);
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[12..16], b"fmt ");
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(
        u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
        16_000
    );
    assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1); // channels
    assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16); // bits
    assert_eq!(&wav[44..46], &[0x02, 0x01]); // first sample little-endian
}

#[test]
fn wav_empty_samples_is_header_only() {
    let wav = encode_wav_pcm16_mono(&[], 16_000);
    assert_eq!(wav.len(), 44);
    assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 0); // data len
}

#[test]
fn join_transcriptions_url_handles_trailing_slash() {
    assert_eq!(
        join_transcriptions_url("https://x/v1"),
        "https://x/v1/audio/transcriptions"
    );
    assert_eq!(
        join_transcriptions_url("https://x/v1/"),
        "https://x/v1/audio/transcriptions"
    );
}
