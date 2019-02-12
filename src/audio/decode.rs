use std::io;

use super::PcmData;

#[derive(Debug)]
pub enum PcmReadError {
    Io(io::Error),
    Eof,
    SkippedData,
}

pub trait PcmRead {
    fn read(&mut self) -> Result<PcmData, PcmReadError>;
}

mod mp3;
pub use self::mp3::Mp3;

mod ogg;
pub use self::ogg::Ogg;
