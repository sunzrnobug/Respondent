use std::time::{Duration, Instant};

use respondent_lib::audio::capture::LoopbackCapture;

#[test]
#[ignore = "requires audible system output on Windows"]
fn loopback_capture_receives_non_silent_16khz_frames() {
    let capture = LoopbackCapture::start("default-output").expect("start loopback capture");
    let receiver = capture.receiver();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut saw_non_silent = false;

    while Instant::now() < deadline {
        if let Ok(frame) = receiver.recv_timeout(Duration::from_millis(250)) {
            assert_eq!(frame.format.sample_rate, 16_000);
            assert_eq!(frame.format.channels, 1);
            assert_eq!(frame.format.bits_per_sample, 16);
            assert_eq!(frame.samples.len(), 320);
            saw_non_silent |= frame.samples.iter().any(|sample| *sample != 0);
            if saw_non_silent {
                break;
            }
        }
    }

    capture.stop().expect("stop capture");
    assert!(
        saw_non_silent,
        "play system audio while running this ignored test"
    );
}
