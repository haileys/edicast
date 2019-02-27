use lame::Lame;

use crate::audio::PcmData;
use crate::config::{self, CodecConfig};

pub trait Codec {
    fn describe(&self) -> String;
    fn encode(&mut self, data: &PcmData) -> Box<[u8]>;
}

pub fn from_config(config: &CodecConfig) -> Box<Codec> {
    match config {
        CodecConfig::Mp3(mp3) => Box::new(Mp3::new(mp3)) as Box<Codec>,
    }
}

pub fn mime_type_from_config(config: &CodecConfig) -> &'static str {
    match config {
        CodecConfig::Mp3(_) => "audio/mpeg",
    }
}

pub struct Mp3 {
    lame: Lame,
}

impl Mp3 {
    pub fn new(config: &config::Mp3Config) -> Self {
        let mut lame = Lame::new().expect("Lame::new");
        lame.set_quality(config.quality as u8).expect("Lame::set_quality");
        lame.set_kilobitrate(config.bitrate as i32).expect("Lame::set_kilobitrate");
        lame.init_params().expect("Lame::init_params");
        Mp3 { lame }
    }
}

impl Codec for Mp3 {
    fn describe(&self) -> String {
        format!("MP3 (libmp3lame, V{}, {} kbps)",
            self.lame.quality(),
            self.lame.kilobitrate())
    }

    fn encode(&mut self, data: &PcmData) -> Box<[u8]> {
        // we must deinterleave audio data for LAME and discard channels beyond
        // stereo. LAME does have an interleaved encode function, but it still
        // bakes in 2 channel left/right assumptions which makes it unsafe to
        // generalise for arbitrary PcmData which may have >2 channels
        let mut left = Vec::new();
        let mut right = Vec::new();

        if data.channels == 1 {
            left = data.samples.to_vec();
            right = data.samples.to_vec();
        } else {
            for chunk in data.samples.chunks(data.channels) {
                left.push(chunk[0]);
                right.push(chunk[1]);
            }
        }

        // vector size calculation is a suggestion from lame/lame.h:
        let mut mp3buff: Vec<u8> = vec![0; (left.len() * 5) / 4 + 7200];

        match self.lame.encode(&left, &right, &mut mp3buff) {
            Ok(sz) => {
                mp3buff.resize(sz, 0);
                mp3buff.into_boxed_slice()
            }
            Err(e) => panic!("lame encode error! {:?}", e)
        }
    }
}
