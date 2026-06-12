pub const TARGET_RATE: u32 = 16_000;
pub const TARGET_CHANNELS: u16 = 1;
pub const TARGET_BITS_PER_SAMPLE: u16 = 16;
pub const TARGET_FRAME_SAMPLES: usize = 320;

pub fn downmix_to_mono(interleaved: &[f32], channels: u16) -> Vec<f32> {
    if channels == 0 {
        return Vec::new();
    }
    if channels == 1 {
        return interleaved.to_vec();
    }

    let channels = channels as usize;
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

pub fn to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|sample| {
            let clamped = sample.clamp(-1.0, 1.0);
            (clamped * i16::MAX as f32).round() as i16
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct LinearResampler {
    src_rate: u32,
    dst_rate: u32,
    pos: f64,
    last: Option<f32>,
}

impl LinearResampler {
    pub fn new(src_rate: u32, dst_rate: u32) -> Self {
        Self {
            src_rate,
            dst_rate,
            pos: 0.0,
            last: None,
        }
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if input.is_empty() {
            return Vec::new();
        }
        if self.src_rate == self.dst_rate {
            self.last = input.last().copied();
            return input.to_vec();
        }
        if self.src_rate == 0 || self.dst_rate == 0 {
            self.last = input.last().copied();
            return Vec::new();
        }

        let mut extended = Vec::with_capacity(input.len() + usize::from(self.last.is_some()));
        if let Some(last) = self.last {
            extended.push(last);
        }
        extended.extend_from_slice(input);

        let offset = if self.last.is_some() { 1.0 } else { 0.0 };
        let step = self.src_rate as f64 / self.dst_rate as f64;
        let mut output = Vec::new();

        while self.pos + 1.0 < extended.len() as f64 {
            let left_index = self.pos.floor() as usize;
            let frac = (self.pos - left_index as f64) as f32;
            let left = extended[left_index];
            let right = extended[left_index + 1];
            output.push(left + (right - left) * frac);
            self.pos += step;
        }

        self.pos -= input.len() as f64;
        if self.last.is_some() {
            self.pos += offset;
        }
        if self.pos < 0.0 {
            self.pos = 0.0;
        }
        self.last = input.last().copied();

        output
    }
}

#[derive(Debug, Default, Clone)]
pub struct FrameChunker {
    buf: Vec<i16>,
}

impl FrameChunker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, samples: &[i16]) -> Vec<Vec<i16>> {
        self.buf.extend_from_slice(samples);
        let mut frames = Vec::new();
        while self.buf.len() >= TARGET_FRAME_SAMPLES {
            frames.push(self.buf.drain(..TARGET_FRAME_SAMPLES).collect());
        }
        frames
    }

    pub fn pending_len(&self) -> usize {
        self.buf.len()
    }
}
