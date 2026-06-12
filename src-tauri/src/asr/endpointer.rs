use crate::audio::frame::AudioFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointSignal {
    StartOfSpeech,
    EndOfSpeech,
}

/// Provider-independent turn detection from audio energy. A frame whose RMS
/// energy is at or above `speech_threshold` counts as speech; once speech has
/// started, `silence_window_ms` of continuous sub-threshold audio ends the turn.
pub struct EnergyEndpointer {
    speech_threshold: f32,
    silence_window_ms: u32,
    in_speech: bool,
    silence_accum_ms: u32,
}

impl EnergyEndpointer {
    pub fn new(speech_threshold: f32, silence_window_ms: u32) -> Self {
        Self {
            speech_threshold,
            silence_window_ms,
            in_speech: false,
            silence_accum_ms: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(0.01, 300)
    }

    pub fn silence_window_ms(&self) -> u32 {
        self.silence_window_ms
    }

    pub fn observe(&mut self, frame: &AudioFrame) -> Option<EndpointSignal> {
        let rms = frame_rms(&frame.samples);
        if rms >= self.speech_threshold {
            self.silence_accum_ms = 0;
            if !self.in_speech {
                self.in_speech = true;
                return Some(EndpointSignal::StartOfSpeech);
            }
            return None;
        }

        if self.in_speech {
            self.silence_accum_ms = self.silence_accum_ms.saturating_add(frame.duration_ms());
            if self.silence_accum_ms >= self.silence_window_ms {
                self.in_speech = false;
                self.silence_accum_ms = 0;
                return Some(EndpointSignal::EndOfSpeech);
            }
        }
        None
    }
}

fn frame_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&sample| {
            let normalized = sample as f64 / 32768.0_f64;
            normalized * normalized
        })
        .sum();
    ((sum_sq / samples.len() as f64).sqrt()) as f32
}
