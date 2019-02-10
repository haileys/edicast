pub mod encode;
pub mod decode;

#[derive(Clone)]
pub struct PcmData {
    pub sample_rate: usize,
    pub channels: usize,
    pub samples: Box<[i16]>,
}
