pub mod encode;
pub mod decode;

#[derive(Clone)]
pub struct PcmData {
    pub left: Box<[i16]>,
    pub right: Box<[i16]>,
}
