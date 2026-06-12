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
    assert_eq!(endpointer.observe(&frame(8000, 0)), Some(EndpointSignal::StartOfSpeech));
    assert_eq!(endpointer.observe(&frame(8000, 20)), None);
}

#[test]
fn endpointer_emits_end_of_speech_after_silence_window() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    endpointer.observe(&frame(8000, 0)); // start of speech
    assert_eq!(endpointer.observe(&frame(0, 20)), None); // 20ms silence
    assert_eq!(endpointer.observe(&frame(0, 40)), None); // 40ms silence
    assert_eq!(endpointer.observe(&frame(0, 60)), Some(EndpointSignal::EndOfSpeech)); // 60ms
    assert_eq!(endpointer.observe(&frame(0, 80)), None); // already idle
}

#[test]
fn endpointer_rearms_for_a_new_utterance() {
    let mut endpointer = EnergyEndpointer::new(0.01, 60);
    endpointer.observe(&frame(8000, 0));
    endpointer.observe(&frame(0, 20));
    endpointer.observe(&frame(0, 40));
    endpointer.observe(&frame(0, 60)); // end of speech
    assert_eq!(endpointer.observe(&frame(8000, 80)), Some(EndpointSignal::StartOfSpeech));
}

#[test]
fn endpointer_exposes_its_silence_window() {
    let endpointer = EnergyEndpointer::new(0.01, 300);
    assert_eq!(endpointer.silence_window_ms(), 300);
}
