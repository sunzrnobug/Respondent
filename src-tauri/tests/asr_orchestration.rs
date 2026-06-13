use respondent_lib::asr::endpointer::{EndpointSignal, EnergyEndpointer};
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};

fn frame(amplitude: i16, captured_at_ms: u64) -> AudioFrame {
    AudioFrame {
        format: PcmFormat {
            sample_rate: 16_000,
            channels: 1,
            bits_per_sample: 16,
        },
        samples: vec![amplitude; 320],
        captured_at_ms,
    }
}

#[test]
fn endpointer_ignores_pure_silence() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    assert_eq!(endpointer.observe(&frame(0, 0)), None);
    assert_eq!(endpointer.observe(&frame(0, 20)), None);
}

#[test]
fn endpointer_emits_start_of_speech_once() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    assert_eq!(
        endpointer.observe(&frame(8000, 0)),
        Some(EndpointSignal::StartOfSpeech)
    );
    assert_eq!(endpointer.observe(&frame(8000, 20)), None);
}

#[test]
fn endpointer_emits_end_of_speech_after_silence_window() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    endpointer.observe(&frame(8000, 0)); // start of speech
    assert_eq!(endpointer.observe(&frame(0, 20)), None); // 20ms silence
    assert_eq!(endpointer.observe(&frame(0, 40)), None); // 40ms silence
    assert_eq!(
        endpointer.observe(&frame(0, 60)),
        Some(EndpointSignal::EndOfSpeech)
    ); // 60ms
    assert_eq!(endpointer.observe(&frame(0, 80)), None); // already idle
}

#[test]
fn endpointer_rearms_for_a_new_utterance() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    endpointer.observe(&frame(8000, 0));
    endpointer.observe(&frame(0, 20));
    endpointer.observe(&frame(0, 40));
    endpointer.observe(&frame(0, 60)); // end of speech
    assert_eq!(
        endpointer.observe(&frame(8000, 80)),
        Some(EndpointSignal::StartOfSpeech)
    );
}

#[test]
fn endpointer_exposes_its_silence_window() {
    let endpointer = EnergyEndpointer::new(0.01, 300);
    assert_eq!(endpointer.silence_window_ms(), 300);
}

#[test]
fn endpointer_speech_burst_resets_silence_counter() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    endpointer.observe(&frame(8000, 0)); // start of speech
    endpointer.observe(&frame(0, 20)); // 20ms silence
    endpointer.observe(&frame(0, 40)); // 40ms silence (not yet 60)
    endpointer.observe(&frame(8000, 60)); // speech resumes -> counter resets
    assert_eq!(endpointer.observe(&frame(0, 80)), None); // only 20ms since speech
    assert_eq!(endpointer.observe(&frame(0, 100)), None); // 40ms
    assert_eq!(
        endpointer.observe(&frame(0, 120)),
        Some(EndpointSignal::EndOfSpeech)
    ); // 60ms
}

use respondent_lib::asr::client::{AsrEvent, StreamingAsrClient};
use respondent_lib::asr::mock::MockAsrClient;

#[test]
fn mock_emits_partials_while_frames_arrive() {
    let mut client = MockAsrClient::new("s1");
    let events = client.events();
    for index in 0..25 {
        client.push_frame(&frame(8000, index * 20)).unwrap();
    }

    match events.try_recv().expect("a partial after 25 frames") {
        AsrEvent::Partial {
            session_id, text, ..
        } => {
            assert_eq!(session_id, "s1");
            assert_eq!(text, "could");
        }
        other => panic!("expected partial, got {other:?}"),
    }
}

#[test]
fn mock_emits_full_phrase_on_finalize() {
    let mut client = MockAsrClient::new("s1");
    let events = client.events();
    for index in 0..10 {
        client.push_frame(&frame(8000, index * 20)).unwrap();
    }
    client.finalize().unwrap();

    let mut last_final = None;
    while let Ok(event) = events.try_recv() {
        if let AsrEvent::Final { text, .. } = event {
            last_final = Some(text);
        }
    }
    assert_eq!(
        last_final.as_deref(),
        Some("could you summarize the timeline")
    );
}

#[test]
fn mock_advances_to_next_phrase_after_finalize() {
    let mut client = MockAsrClient::new("s1");
    let events = client.events();

    client.push_frame(&frame(8000, 0)).unwrap();
    client.finalize().unwrap();
    client.push_frame(&frame(8000, 20)).unwrap();
    client.finalize().unwrap();

    let finals: Vec<String> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            AsrEvent::Final { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(
        finals,
        vec![
            "could you summarize the timeline".to_string(),
            "what are the main risks".to_string(),
        ]
    );
}

#[test]
fn mock_finalize_without_frames_is_a_noop() {
    let mut client = MockAsrClient::new("s1");
    let events = client.events();
    client.finalize().unwrap();
    assert!(events.try_recv().is_err());
}

use std::time::Duration;

use crossbeam_channel::unbounded as unbounded_frames;
use respondent_lib::asr::session::TranscriptionSession;

#[test]
fn session_emits_partial_then_endpoint_then_final_for_one_utterance() {
    let (frame_tx, frame_rx) = unbounded_frames();
    let session = TranscriptionSession::start(
        "s1".to_string(),
        frame_rx,
        Box::new(MockAsrClient::new("s1")),
        EnergyEndpointer::new(0.01, 60),
    );
    let events = session.events();

    let mut at_ms = 0u64;
    for _ in 0..30 {
        frame_tx.send(frame(8000, at_ms)).unwrap();
        at_ms += 20;
    }
    for _ in 0..5 {
        frame_tx.send(frame(0, at_ms)).unwrap();
        at_ms += 20;
    }
    drop(frame_tx); // closing the capture stream ends the session

    let mut collected = Vec::new();
    while let Ok(event) = events.recv_timeout(Duration::from_secs(2)) {
        collected.push(event);
    }
    session.stop().unwrap();

    assert!(
        matches!(collected.first(), Some(AsrEvent::Partial { .. })),
        "first event should be a partial, got {collected:?}"
    );
    let endpoint_pos = collected
        .iter()
        .position(|event| matches!(event, AsrEvent::Endpoint { .. }))
        .expect("an endpoint event");
    let final_pos = collected
        .iter()
        .position(|event| matches!(event, AsrEvent::Final { .. }))
        .expect("a final event");
    assert!(
        endpoint_pos < final_pos,
        "endpoint must precede final: {collected:?}"
    );
}
