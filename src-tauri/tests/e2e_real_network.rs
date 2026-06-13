use std::thread;
use std::time::{Duration, Instant};

use respondent_lib::asr::client::{AsrEvent, StreamingAsrClient};
use respondent_lib::asr::openai_realtime::{OpenAiRealtimeAsrClient, OpenAiRealtimeConfig};
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};
use respondent_lib::llm::client::{ReplyEvent, ReplyPoll, ReplyRequest, StreamingReplyClient};
use respondent_lib::llm::openai_compatible::{OpenAiCompatibleReplyClient, ProviderConfig};
use respondent_lib::llm::openai_responses::OpenAiReplyClient;

#[test]
#[ignore = "uses real SiliconFlow network calls and billable API usage"]
fn real_siliconflow_llm_smoke_when_api_key_is_present() {
    let Some(api_key) = std::env::var("SILICONFLOW_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!("skipping real SiliconFlow E2E smoke: SILICONFLOW_API_KEY is not set");
        return;
    };
    let model = std::env::var("SILICONFLOW_LLM_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Qwen/Qwen2.5-7B-Instruct".to_string());
    eprintln!("[siliconflow] model = {model}");

    let client = OpenAiCompatibleReplyClient::connect(ProviderConfig {
        base_url: "https://api.siliconflow.cn/v1".into(),
        api_key,
        model,
    })
    .expect("connect SiliconFlow compatible LLM");

    let mut generation = client.start(ReplyRequest {
        session_id: "sf-e2e".into(),
        generation_id: "gen-sf".into(),
        transcript: "Could you summarize the timeline and next steps?".into(),
        context: vec!["Could you summarize the timeline and next steps?".into()],
    });

    let deadline = Instant::now() + Duration::from_secs(40);
    let mut token_count = 0usize;
    let mut final_text: Option<String> = None;
    while Instant::now() < deadline {
        match generation.poll() {
            ReplyPoll::Event(ReplyEvent::Started { .. }) => eprintln!("[siliconflow] started"),
            ReplyPoll::Event(ReplyEvent::Token { token, .. }) => {
                token_count += 1;
                eprint!("{token}");
            }
            ReplyPoll::Event(ReplyEvent::Final { text, .. }) => final_text = Some(text),
            ReplyPoll::Pending => thread::sleep(Duration::from_millis(20)),
            ReplyPoll::Done => break,
        }
    }

    let final_text = final_text.expect("SiliconFlow final reply");
    eprintln!(
        "\n[siliconflow] tokens={token_count} final_len={}",
        final_text.len()
    );
    assert!(
        !final_text.trim().is_empty(),
        "SiliconFlow smoke must produce non-empty final text"
    );
    assert!(
        !final_text.contains("Reply generation failed"),
        "SiliconFlow request failed (generic failure final returned)"
    );
    assert!(
        token_count > 0,
        "expected streamed tokens, not just a final"
    );
}

#[test]
#[ignore = "uses real OpenAI network calls and billable API usage"]
fn real_openai_asr_and_llm_smoke_when_api_key_is_present() {
    if std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .is_none()
    {
        eprintln!("skipping real OpenAI E2E smoke: OPENAI_API_KEY is not set");
        return;
    }

    let session_id = "e2e-real-network".to_string();
    let api_key = std::env::var("OPENAI_API_KEY").expect("checked key");
    let mut asr = OpenAiRealtimeAsrClient::connect(
        session_id.clone(),
        OpenAiRealtimeConfig::from_api_key(api_key),
    )
    .expect("connect real OpenAI realtime ASR");

    for frame in smoke_frames() {
        asr.push_frame(&frame).expect("push real ASR frame");
    }
    asr.finalize().expect("finalize real ASR");
    let transcript = wait_for_asr_final(&asr).unwrap_or_else(|| {
        eprintln!("real ASR smoke connected and finalized, but produced no final transcript from synthetic audio");
        "Please suggest a concise meeting reply for asking about timeline.".to_string()
    });

    let llm = OpenAiReplyClient::from_env().expect("connect real OpenAI responses LLM");
    let mut generation = llm.start(ReplyRequest {
        session_id,
        generation_id: "gen-real-network".into(),
        transcript: transcript.clone(),
        context: vec![transcript],
    });
    let reply = wait_for_reply_final(&mut generation).expect("real LLM final reply");

    assert!(
        !reply.trim().is_empty(),
        "real LLM smoke must produce non-empty final text"
    );
}

#[test]
#[ignore = "captures real system audio and uses real SiliconFlow file transcription"]
fn real_capture_to_siliconflow_transcription() {
    use respondent_lib::asr::siliconflow_file::{
        encode_wav_pcm16_mono, ReqwestTranscriptionTransport, SiliconFlowFileConfig,
        TranscriptionTransport,
    };
    use respondent_lib::audio::capture::LoopbackCapture;

    let Some(api_key) = std::env::var("SILICONFLOW_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!("skipping: SILICONFLOW_API_KEY not set");
        return;
    };

    let capture = LoopbackCapture::start("default-output").expect("start loopback capture");
    let receiver = capture.receiver();
    let mut buffer: Vec<i16> = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        if let Ok(frame) = receiver.recv_timeout(Duration::from_millis(200)) {
            buffer.extend_from_slice(&frame.samples);
        }
    }
    let _ = capture.stop();

    let nonzero = buffer.iter().filter(|sample| **sample != 0).count();
    eprintln!(
        "[capture] {} samples (~{} ms), non-zero={nonzero}",
        buffer.len(),
        buffer.len() / 16
    );
    assert!(!buffer.is_empty(), "no audio frames captured");

    let wav = encode_wav_pcm16_mono(&buffer, 16_000);
    let model = std::env::var("SILICONFLOW_ASR_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "FunAudioLLM/SenseVoiceSmall".to_string());
    eprintln!(
        "[siliconflow-asr] model = {model}, wav_bytes = {}",
        wav.len()
    );
    let transport = ReqwestTranscriptionTransport::default();
    let config = SiliconFlowFileConfig {
        base_url: "https://api.siliconflow.cn/v1".into(),
        api_key,
        model,
    };
    let text = transport
        .transcribe(&config, &wav)
        .expect("real SiliconFlow transcription round-trip");
    eprintln!("[siliconflow-asr] TRANSCRIPT: {text:?}");
}

fn smoke_frames() -> Vec<AudioFrame> {
    let mut frames = Vec::new();
    for frame_index in 0..25 {
        let samples = (0..320)
            .map(|sample_index| {
                let phase = (frame_index * 320 + sample_index) as f32 / 16_000.0;
                (phase * 440.0 * std::f32::consts::TAU).sin() * 4000.0
            })
            .map(|sample| sample as i16)
            .collect();
        frames.push(AudioFrame {
            format: PcmFormat {
                sample_rate: 16_000,
                channels: 1,
                bits_per_sample: 16,
            },
            samples,
            captured_at_ms: (frame_index * 20) as u64,
        });
    }
    frames
}

fn wait_for_asr_final(asr: &OpenAiRealtimeAsrClient) -> Option<String> {
    let events = asr.events();
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(100)) {
            Ok(AsrEvent::Final { text, .. }) if !text.trim().is_empty() => return Some(text),
            Ok(_) => {}
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return None,
        }
    }
    None
}

fn wait_for_reply_final(
    generation: &mut Box<dyn respondent_lib::llm::client::ReplyGeneration>,
) -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        match generation.poll() {
            ReplyPoll::Event(ReplyEvent::Final { text, .. }) => return Some(text),
            ReplyPoll::Event(_) => {}
            ReplyPoll::Pending => thread::sleep(Duration::from_millis(20)),
            ReplyPoll::Done => return None,
        }
    }
    None
}
