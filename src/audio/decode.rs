use std::io::{self, Read};

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

pub struct Mp3<T: Read> {
    mp3: minimp3::Decoder<T>,
}

impl<T: Read> Mp3<T> {
    pub fn new(io: T) -> Self {
        Mp3 { mp3: minimp3::Decoder::new(io) }
    }
}

impl<T: Read> PcmRead for Mp3<T> {
    fn read(&mut self) -> Result<PcmData, PcmReadError> {
        match self.mp3.next_frame() {
            Ok(frame) => Ok(PcmData {
                sample_rate: frame.sample_rate as usize,
                channels: frame.channels,
                samples: frame.data.into_boxed_slice(),
            }),
            Err(minimp3::Error::Eof) => Err(PcmReadError::Eof),
            Err(minimp3::Error::Io(e)) => Err(PcmReadError::Io(e)),
            Err(minimp3::Error::SkippedData) => Err(PcmReadError::SkippedData),
            Err(minimp3::Error::InsufficientData) => {
                // InsufficientData is an error returned by decode_frame that
                // is handled within next_frame. it should never appear here.
                unreachable!()
            }
        }
    }
}
