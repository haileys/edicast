use std::time::Duration;

pub mod encode;
pub mod decode;

#[derive(Clone)]
pub struct PcmData {
    pub sample_rate: usize,
    pub channels: usize,
    pub samples: Box<[i16]>,
}

impl PcmData {
    pub fn silence(duration: Duration) -> Self {
        let sample_rate = 44100;
        let channels = 2;

        let channel_sample_count = (duration.as_nanos() * (sample_rate as u128) / 1_000_000_000) as usize;
        let sample_count = channel_sample_count * channels;

        let samples = {
            let mut samples = Vec::new();
            samples.resize(sample_count, 0i16);
            samples.into_boxed_slice()
        };

        PcmData { sample_rate, channels, samples }
    }
}
