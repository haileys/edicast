use std::collections::HashMap;
use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::Path;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub listen: ListenConfig,
    pub source: HashMap<String, SourceConfig>,
    pub stream: HashMap<String, StreamConfig>,
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Toml(toml::de::Error),
    StreamRefersToInvalidSource { stream_name: String, source_name: String },
}

impl Config {
    pub fn load(file: impl AsRef<Path>) -> Result<Self, Error> {
        let contents = fs::read_to_string(file).map_err(Error::Io)?;
        let config = toml::from_str::<Config>(&contents).map_err(Error::Toml)?;

        // validate that all stream point to valid sources
        for (name, stream) in config.stream.iter() {
            if !config.source.contains_key(&stream.source) {
                return Err(Error::StreamRefersToInvalidSource {
                    stream_name: name.to_owned(),
                    source_name: stream.source.to_owned(),
                });
            }
        }

        Ok(config)
    }
}

#[derive(Deserialize, Debug)]
pub struct ListenConfig {
    pub public: SocketAddr,
    pub control: SocketAddr,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum OfflineBehaviour {
    #[serde(rename = "inactive")]
    Inactive,
    #[serde(rename = "silence")]
    Silence,
}

impl Default for OfflineBehaviour {
    fn default() -> Self {
        OfflineBehaviour::Inactive
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SourceConfig {
    pub offline: OfflineBehaviour,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Mp3Config {
    pub bitrate: usize,
    pub quality: usize,
}

#[derive(Deserialize, Debug, Clone)]
pub enum CodecConfig {
    #[serde(rename = "mp3")]
    Mp3(Mp3Config),
}

#[derive(Deserialize, Debug, Clone)]
pub struct StreamConfig {
    pub path: String,
    pub source: String,
    pub codec: CodecConfig,
}
