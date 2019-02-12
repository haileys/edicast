use std::io::{self, Read, Seek, SeekFrom};

use crate::audio::decode::{PcmRead, PcmReadError};
use crate::audio::PcmData;

use ogg::{PacketReader, OggReadError};
use lewton::VorbisError;
use lewton::inside_ogg::read_headers;
use lewton::audio::{read_audio_packet, PreviousWindowRight, AudioReadError};
use lewton::header::{IdentHeader, SetupHeader};

struct NonSeekStream<T: Read> {
    stream: T,
}

impl<T> Read for NonSeekStream<T> where T: Read {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        self.stream.read(buf)
    }
}

impl<T> Seek for NonSeekStream<T> where T: Read {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, io::Error> {
        panic!("trying to seek NonSeekStream: {:?}", pos);
    }
}

impl<T> NonSeekStream<T> where T: Read {
    pub fn new(stream: T) -> NonSeekStream<T> {
        NonSeekStream { stream: stream }
    }
}

pub struct Ogg<T: Read> {
    rdr: PacketReader<NonSeekStream<T>>,
    pwr: PreviousWindowRight,
    ident_hdr: IdentHeader,
    setup_hdr: SetupHeader,
}

impl<T: Read> Ogg<T> {
    // TODO - move header read into PcmRead::read so that new is pure
    pub fn new(io: T) -> Result<Self, VorbisError> {
        let mut rdr = PacketReader::new(NonSeekStream::new(io));

        let ((ident_hdr, _, setup_hdr), _) = read_headers(&mut rdr)?;

        Ok(Ogg {
            rdr,
            pwr: PreviousWindowRight::new(),
            ident_hdr,
            setup_hdr,
        })
    }
}

impl<T: Read> PcmRead for Ogg<T> {
    fn read(&mut self) -> Result<PcmData, PcmReadError> {
        let packet = match self.rdr.read_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => return Err(PcmReadError::Eof),
            Err(OggReadError::ReadError(e)) => return Err(PcmReadError::Io(e)),
            Err(OggReadError::NoCapturePatternFound) |
            Err(OggReadError::InvalidStreamStructVer(_)) |
            Err(OggReadError::HashMismatch(_, _)) |
            Err(OggReadError::InvalidData) => return Err(PcmReadError::SkippedData),
        };

        let decoded_packet = read_audio_packet(&self.ident_hdr,
            &self.setup_hdr, &packet.data, &mut self.pwr);

        match decoded_packet {
            Ok(pcm) => {
                // lewton gives us audio data per channel
                // interleave it:
                let mut channel_iters = pcm.into_iter()
                    .map(|channel| channel.into_iter())
                    .collect::<Vec<_>>();

                let mut interleaved_pcm = Vec::new();

                'outer: loop {
                    for channel in &mut channel_iters {
                        match channel.next() {
                            Some(sample) => interleaved_pcm.push(sample),
                            None => break 'outer,
                        }
                    }
                }

                Ok(PcmData {
                    sample_rate: self.ident_hdr.audio_sample_rate as usize,
                    channels: self.ident_hdr.audio_channels as usize,
                    samples: interleaved_pcm.into_boxed_slice(),
                })
            }
            Err(AudioReadError::AudioIsHeader) => {
                // this is where we would potentially read out stream metadata
                return Err(PcmReadError::SkippedData);
            }
            Err(_) => {
                return Err(PcmReadError::SkippedData);
            }
        }
    }
}
