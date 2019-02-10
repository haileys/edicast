use lame::Lame;

use crate::audio::PcmData;
use crate::config::{self, CodecConfig};

pub trait Codec {
    fn encode(&mut self, data: &PcmData) -> Box<[u8]>;
}

pub fn from_config(config: &CodecConfig) -> Box<Codec> {
    match config {
        CodecConfig::Mp3(mp3) => Box::new(Mp3::new(mp3)) as Box<Codec>,
    }
}

pub fn header_from_config(config: &CodecConfig) -> Box<[u8]> {
    match config {
        CodecConfig::Mp3(_) => Box::new([]),
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
    fn encode(&mut self, data: &PcmData) -> Box<[u8]> {
        let num_samples = data.left.len();

        // vector size calculation is a suggestion from lame/lame.h:
        let mut mp3buff: Vec<u8> = vec![0; (num_samples * 5) / 4 + 7200];

        match self.lame.encode(&data.left, &data.right, &mut mp3buff) {
            Ok(sz) => {
                mp3buff.resize(sz, 0);
                mp3buff.into_boxed_slice()
            }
            Err(e) => panic!("lame encode error! {:?}", e)
        }
    }
}
