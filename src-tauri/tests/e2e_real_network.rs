use std::thread;
use std::time::{Duration, Instant};

use respondent_lib::asr::bailian_realtime::{BailianRealtimeAsrClient, BailianRealtimeConfig};
use respondent_lib::asr::client::{AsrEvent, StreamingAsrClient};
use respondent_lib::asr::openai_realtime::{OpenAiRealtimeAsrClient, OpenAiRealtimeConfig};
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};
use respondent_lib::llm::client::{ReplyEvent, ReplyPoll, ReplyRequest, StreamingReplyClient};
use respondent_lib::llm::openai_compatible::{OpenAiCompatibleReplyClient, ProviderConfig};
use respondent_lib::llm::openai_responses::OpenAiReplyClient;

const PARTIAL_TRANSCRIPT_TARGET_MS: u128 = 2_000;
const FIRST_REPLY_TOKEN_TARGET_MS: u128 = 3_000;

fn report_latency(label: &str, elapsed_ms: u128, target_ms: u128) {
    let status = if elapsed_ms <= target_ms { "PASS" } else { "SLOW" };
    eprintln!("[acceptance] {label}: {elapsed_ms}ms (target <{target_ms}ms) [{status}]");
}

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

    let started = Instant::now();
    let mut generation = client.start(ReplyRequest {
        session_id: "sf-e2e".into(),
        generation_id: "gen-sf".into(),
        transcript: "Could you summarize the timeline and next steps?".into(),
        context: vec!["Could you summarize the timeline and next steps?".into()],
        document_context: None,
        reply_style: None,
    });

    let deadline = Instant::now() + Duration::from_secs(40);
    let mut token_count = 0usize;
    let mut final_text: Option<String> = None;
    let mut first_token_at: Option<Instant> = None;
    while Instant::now() < deadline {
        match generation.poll() {
            ReplyPoll::Event(ReplyEvent::Started { .. }) => eprintln!("[siliconflow] started"),
            ReplyPoll::Event(ReplyEvent::Token { token, .. }) => {
                if first_token_at.is_none() {
                    first_token_at = Some(Instant::now());
                    report_latency(
                        "siliconflow first reply token",
                        started.elapsed().as_millis(),
                        FIRST_REPLY_TOKEN_TARGET_MS,
                    );
                }
                token_count += 1;
                eprint!("{token}");
            }
            ReplyPoll::Event(ReplyEvent::Final { text, .. }) => final_text = Some(text),
            ReplyPoll::Event(ReplyEvent::Cancelled { .. }) => break,
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
        !final_text.contains("回复生成失败"),
        "SiliconFlow request failed (generic failure final returned)"
    );
    assert!(
        token_count > 0,
        "expected streamed tokens, not just a final"
    );
    assert!(first_token_at.is_some(), "expected first reply token latency");
}

#[test]
#[ignore = "uses real DashScope/Bailian network calls and billable API usage"]
fn real_dashscope_llm_smoke_when_api_key_is_present() {
    let Some(api_key) = std::env::var("DASHSCOPE_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!("skipping real DashScope E2E smoke: DASHSCOPE_API_KEY is not set");
        return;
    };
    let base_url = std::env::var("DASHSCOPE_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string());
    let model = std::env::var("DASHSCOPE_LLM_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "qwen-plus".to_string());
    eprintln!("[dashscope] model = {model}");

    let client = OpenAiCompatibleReplyClient::connect(ProviderConfig {
        base_url,
        api_key,
        model,
    })
    .expect("connect DashScope compatible LLM");

    let started = Instant::now();
    let mut generation = client.start(ReplyRequest {
        session_id: "dashscope-e2e".into(),
        generation_id: "gen-dashscope".into(),
        transcript: "请用一句话概括会议时间线和下一步。".into(),
        context: vec!["请用一句话概括会议时间线和下一步。".into()],
        document_context: None,
        reply_style: None,
    });

    let deadline = Instant::now() + Duration::from_secs(40);
    let mut token_count = 0usize;
    let mut final_text: Option<String> = None;
    let mut first_token_at: Option<Instant> = None;
    while Instant::now() < deadline {
        match generation.poll() {
            ReplyPoll::Event(ReplyEvent::Started { .. }) => eprintln!("[dashscope] started"),
            ReplyPoll::Event(ReplyEvent::Token { token, .. }) => {
                if first_token_at.is_none() {
                    first_token_at = Some(Instant::now());
                    report_latency(
                        "dashscope first reply token",
                        started.elapsed().as_millis(),
                        FIRST_REPLY_TOKEN_TARGET_MS,
                    );
                }
                token_count += 1;
                eprint!("{token}");
            }
            ReplyPoll::Event(ReplyEvent::Final { text, .. }) => final_text = Some(text),
            ReplyPoll::Event(ReplyEvent::Cancelled { .. }) => break,
            ReplyPoll::Pending => thread::sleep(Duration::from_millis(20)),
            ReplyPoll::Done => break,
        }
    }

    let final_text = final_text.expect("DashScope final reply");
    eprintln!(
        "\n[dashscope] tokens={token_count} final_len={}",
        final_text.len()
    );
    assert!(!final_text.trim().is_empty(), "DashScope smoke must produce text");
    assert!(token_count > 0, "expected streamed tokens");
    assert!(first_token_at.is_some(), "expected first reply token latency");
}

#[test]
#[ignore = "uses real Bailian realtime ASR and DashScope LLM network calls"]
fn real_bailian_asr_and_llm_smoke_when_api_key_is_present() {
    let Some(api_key) = std::env::var("DASHSCOPE_API_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
    else {
        eprintln!("skipping real Bailian E2E smoke: DASHSCOPE_API_KEY is not set");
        return;
    };

    let session_id = "e2e-bailian-network".to_string();
    let mut asr = BailianRealtimeAsrClient::connect(
        session_id.clone(),
        BailianRealtimeConfig::from_api_key(api_key.clone()),
    )
    .expect("connect real Bailian realtime ASR");

    let asr_started = Instant::now();
    for frame in smoke_frames() {
        asr.push_frame(&frame).expect("push real Bailian ASR frame");
    }
    if let Err(err) = asr.finalize() {
        eprintln!("bailian finalize on synthetic audio: {err:?}");
    }
    let (transcript, partial_ms, final_ms) =
        wait_for_bailian_asr_timings(&asr, asr_started).unwrap_or_else(|| {
            eprintln!(
                "real Bailian ASR connected and finalized, but produced no final transcript from synthetic audio"
            );
            (
                "请建议一句关于时间线的简洁会议回复。".to_string(),
                None,
                None,
            )
        });
    if let Some(ms) = partial_ms {
        report_latency("bailian first partial transcript", ms, PARTIAL_TRANSCRIPT_TARGET_MS);
    }
    if let Some(ms) = final_ms {
        eprintln!("[acceptance] bailian final transcript: {ms}ms");
    }

    let base_url = std::env::var("DASHSCOPE_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string());
    let model = std::env::var("DASHSCOPE_LLM_MODEL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "qwen-plus".to_string());
    let llm = OpenAiCompatibleReplyClient::connect(ProviderConfig {
        base_url,
        api_key,
        model,
    })
    .expect("connect real DashScope compatible LLM");
    let reply_started = Instant::now();
    let mut generation = llm.start(ReplyRequest {
        session_id,
        generation_id: "gen-bailian-network".into(),
        transcript: transcript.clone(),
        context: vec![transcript],
        document_context: None,
        reply_style: None,
    });
    let (reply, first_token_ms) =
        wait_for_reply_final_with_timing(&mut generation, reply_started).expect("real LLM final reply");
    if let Some(ms) = first_token_ms {
        report_latency(
            "dashscope first reply token after final transcript",
            ms,
            FIRST_REPLY_TOKEN_TARGET_MS,
        );
    }

    assert!(
        !reply.trim().is_empty(),
        "real DashScope LLM smoke must produce non-empty final text"
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

    let asr_started = Instant::now();
    for frame in smoke_frames() {
        asr.push_frame(&frame).expect("push real ASR frame");
    }
    asr.finalize().expect("finalize real ASR");
    let (transcript, partial_ms, final_ms) = wait_for_asr_timings(&asr, asr_started).unwrap_or_else(|| {
        eprintln!("real ASR smoke connected and finalized, but produced no final transcript from synthetic audio");
        (
            "Please suggest a concise meeting reply for asking about timeline.".to_string(),
            None,
            None,
        )
    });
    if let Some(ms) = partial_ms {
        report_latency("openai first partial transcript", ms, PARTIAL_TRANSCRIPT_TARGET_MS);
    }
    if let Some(ms) = final_ms {
        eprintln!("[acceptance] openai final transcript: {ms}ms");
    }

    let llm = OpenAiReplyClient::from_env().expect("connect real OpenAI responses LLM");
    let reply_started = Instant::now();
    let mut generation = llm.start(ReplyRequest {
        session_id,
        generation_id: "gen-real-network".into(),
        transcript: transcript.clone(),
        context: vec![transcript],
        document_context: None,
        reply_style: None,
    });
    let (reply, first_token_ms) =
        wait_for_reply_final_with_timing(&mut generation, reply_started).expect("real LLM final reply");
    if let Some(ms) = first_token_ms {
        report_latency(
            "openai first reply token after final transcript",
            ms,
            FIRST_REPLY_TOKEN_TARGET_MS,
        );
    }

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

    let capture_started = Instant::now();
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
    report_latency(
        "siliconflow file transcription after capture",
        capture_started.elapsed().as_millis(),
        PARTIAL_TRANSCRIPT_TARGET_MS,
    );
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

fn wait_for_asr_timings(
    asr: &OpenAiRealtimeAsrClient,
    started: Instant,
) -> Option<(String, Option<u128>, Option<u128>)> {
    let events = asr.events();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut partial_ms = None;
    let mut final_ms = None;
    let mut final_text = None;
    while Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(100)) {
            Ok(AsrEvent::Partial { text, .. }) if !text.trim().is_empty() && partial_ms.is_none() => {
                partial_ms = Some(started.elapsed().as_millis());
                eprintln!("[acceptance] openai first partial: {text:?}");
            }
            Ok(AsrEvent::Final { text, .. }) if !text.trim().is_empty() => {
                final_ms = Some(started.elapsed().as_millis());
                final_text = Some(text);
                break;
            }
            Ok(_) => {}
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
    final_text.map(|text| (text, partial_ms, final_ms))
}

fn wait_for_bailian_asr_timings(
    asr: &BailianRealtimeAsrClient,
    started: Instant,
) -> Option<(String, Option<u128>, Option<u128>)> {
    let events = asr.events();
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut partial_ms = None;
    let mut final_ms = None;
    let mut final_text = None;
    while Instant::now() < deadline {
        match events.recv_timeout(Duration::from_millis(100)) {
            Ok(AsrEvent::Partial { text, .. }) if !text.trim().is_empty() && partial_ms.is_none() => {
                partial_ms = Some(started.elapsed().as_millis());
                eprintln!("[acceptance] bailian first partial: {text:?}");
            }
            Ok(AsrEvent::Final { text, .. }) if !text.trim().is_empty() => {
                final_ms = Some(started.elapsed().as_millis());
                final_text = Some(text);
                break;
            }
            Ok(_) => {}
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
    final_text.map(|text| (text, partial_ms, final_ms))
}

fn wait_for_reply_final_with_timing(
    generation: &mut Box<dyn respondent_lib::llm::client::ReplyGeneration>,
    started: Instant,
) -> Option<(String, Option<u128>)> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut first_token_ms = None;
    let mut final_text = None;
    while Instant::now() < deadline {
        match generation.poll() {
            ReplyPoll::Event(ReplyEvent::Token { .. }) if first_token_ms.is_none() => {
                first_token_ms = Some(started.elapsed().as_millis());
            }
            ReplyPoll::Event(ReplyEvent::Final { text, .. }) => {
                final_text = Some(text);
                break;
            }
            ReplyPoll::Event(_) => {}
            ReplyPoll::Pending => thread::sleep(Duration::from_millis(20)),
            ReplyPoll::Done => break,
        }
    }
    final_text.map(|text| (text, first_token_ms))
}
