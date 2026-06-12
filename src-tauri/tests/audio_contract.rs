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
