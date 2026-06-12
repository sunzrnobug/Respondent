use respondent_lib::audio::devices::{list_output_devices, OutputDevice};
use respondent_lib::audio::convert::{
    downmix_to_mono, to_pcm16, TARGET_BITS_PER_SAMPLE, TARGET_CHANNELS, TARGET_FRAME_SAMPLES,
    TARGET_RATE,
};
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
    assert_eq!(to_pcm16(&[1.5, -1.5, 0.0, 0.5]), vec![32767, -32767, 0, 16384]);
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
