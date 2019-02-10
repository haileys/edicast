use std::io::{self, Read};

use super::PcmData;

#[derive(Debug)]
pub enum PcmReadError {
    Io(io::Error),
    Eof,
    SkippedData,
    MalformedData,
}

pub trait PcmRead {
    fn read(&mut self) -> Result<PcmData, PcmReadError>;
}

pub struct Mp3<T: Read> {
    mp3: minimp3::Decoder<T>,
}

impl<T: Read> PcmRead for Mp3<T> {
    fn read(&mut self) -> Result<PcmData, PcmReadError> {
        match self.mp3.next_frame() {
            Ok(frame) => {
                // TODO - handle variable sample rate:
                assert!(frame.sample_rate == 44100);

                match frame.channels {
                    0 => Err(PcmReadError::MalformedData),
                    1 => Ok(PcmData { left: frame.data.clone().into_boxed_slice(), right: frame.data.clone().into_boxed_slice() }),
                    _ => {
                        let mut left = Vec::new();
                        let mut right = Vec::new();

                        for ch in frame.data.chunks(frame.channels) {
                            left.push(ch[0]);
                            right.push(ch[1]);
                        }

                        Ok(PcmData { left: left.into_boxed_slice(), right: right.into_boxed_slice() })
                    }
                }
            }
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
