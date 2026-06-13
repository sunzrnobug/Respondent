use respondent_lib::asr::siliconflow_file::{encode_wav_pcm16_mono, join_transcriptions_url};

#[test]
fn wav_header_and_length_are_correct() {
    let samples = [0x0102i16, -1];
    let wav = encode_wav_pcm16_mono(&samples, 16_000);
    assert_eq!(wav.len(), 44 + samples.len() * 2);
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[12..16], b"fmt ");
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]), 16_000);
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
    assert_eq!(join_transcriptions_url("https://x/v1"), "https://x/v1/audio/transcriptions");
    assert_eq!(join_transcriptions_url("https://x/v1/"), "https://x/v1/audio/transcriptions");
}
