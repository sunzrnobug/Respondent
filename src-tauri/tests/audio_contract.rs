use respondent_lib::audio::convert::{
    downmix_to_mono, to_pcm16, FrameChunker, LinearResampler, TARGET_BITS_PER_SAMPLE,
    TARGET_CHANNELS, TARGET_FRAME_SAMPLES, TARGET_RATE,
};
use respondent_lib::audio::devices::{list_output_devices, OutputDevice};
use respondent_lib::audio::frame::{AudioFrame, PcmFormat};

#[test]
fn computes_frame_duration_for_16khz_mono_pcm() {
    // 320 i16 samples at 16 kHz mono = 320 / 16000 s = 20 ms.
    let frame = AudioFrame {
        format: PcmFormat {
            sample_rate: 16_000,
            channels: 1,
            bits_per_sample: 16,
        },
        samples: vec![0; 320],
        captured_at_ms: 100,
    };

    assert_eq!(frame.duration_ms(), 20);
}

#[test]
fn target_capture_format_is_16khz_mono_20ms() {
    assert_eq!(TARGET_RATE, 16_000);
    assert_eq!(TARGET_CHANNELS, 1);
    assert_eq!(TARGET_BITS_PER_SAMPLE, 16);
    assert_eq!(TARGET_FRAME_SAMPLES, 320);
}

#[test]
fn downmixes_stereo_to_mono_by_averaging_channels() {
    let mono = downmix_to_mono(&[1.0, -1.0, 0.5, 0.25], 2);
    assert_eq!(mono, vec![0.0, 0.375]);
}

#[test]
fn downmix_ignores_incomplete_interleaved_tail() {
    let mono = downmix_to_mono(&[1.0, 0.0, 99.0], 2);
    assert_eq!(mono, vec![0.5]);
}

#[test]
fn downmix_mono_returns_input_samples() {
    let mono = downmix_to_mono(&[0.25, -0.5], 1);
    assert_eq!(mono, vec![0.25, -0.5]);
}

#[test]
fn downmix_zero_channels_returns_empty_output() {
    assert!(downmix_to_mono(&[1.0, 2.0], 0).is_empty());
}

#[test]
fn quantizes_float_samples_to_pcm16_with_clamp() {
    assert_eq!(
        to_pcm16(&[1.5, -1.5, 0.0, 0.5]),
        vec![32767, -32767, 0, 16384]
    );
}

#[test]
fn resampler_passes_through_when_rates_match() {
    let mut resampler = LinearResampler::new(16_000, 16_000);
    assert_eq!(resampler.process(&[0.0, 0.5, 1.0]), vec![0.0, 0.5, 1.0]);
}

#[test]
fn resampler_downsamples_48khz_to_16khz_at_integer_ratio() {
    let mut resampler = LinearResampler::new(48_000, 16_000);
    assert_eq!(
        resampler.process(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        vec![0.0, 3.0]
    );
}

#[test]
fn resampler_keeps_fractional_ratio_output_monotonic() {
    let mut resampler = LinearResampler::new(44_100, 16_000);
    let output = resampler.process(&(0..100).map(|value| value as f32).collect::<Vec<_>>());
    assert!((35..=37).contains(&output.len()));
    assert!(output.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn resampler_is_continuous_across_chunks() {
    let input = (0..128).map(|value| value as f32).collect::<Vec<_>>();
    let mut one_pass = LinearResampler::new(48_000, 16_000);
    let expected = one_pass.process(&input);

    let mut chunked = LinearResampler::new(48_000, 16_000);
    let mut actual = chunked.process(&input[..64]);
    actual.extend(chunked.process(&input[64..]));

    assert_eq!(actual, expected);
}

#[test]
fn frame_chunker_emits_full_320_sample_frames_and_retains_remainder() {
    let mut chunker = FrameChunker::new();
    let first = chunker.push(&vec![1; 800]);
    assert_eq!(first.len(), 2);
    assert!(first.iter().all(|frame| frame.len() == 320));
    assert_eq!(chunker.pending_len(), 160);

    let second = chunker.push(&vec![2; 160]);
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].len(), 320);
    assert_eq!(chunker.pending_len(), 0);
}

#[test]
fn output_device_serializes_expected_fields() {
    let device = OutputDevice {
        id: "device-1".into(),
        name: "Headphones".into(),
        is_default: true,
    };

    let json = serde_json::to_value(device).expect("serialize device");
    assert_eq!(json["id"], "device-1");
    assert_eq!(json["name"], "Headphones");
    assert_eq!(json["is_default"], true);
}

#[test]
fn lists_at_least_one_output_device_with_a_default() {
    // On Windows this exercises the COM enumeration; on failure or other
    // platforms it falls back to the default endpoint. Either way the
    // contract is: never empty, and a default is present.
    let devices = list_output_devices();
    assert!(!devices.is_empty());
    assert!(devices.iter().any(|device| device.is_default));
}
