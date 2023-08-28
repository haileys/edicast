use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvError};
use std::thread;

use slog::Logger;
use bytes::Bytes;
use tokio::sync::broadcast;

use crate::audio::PcmData;
use crate::audio::encode;
use crate::config::StreamConfig;
use crate::source::SourceSet;

const BUFFER_SIZE: usize = 8;

pub type StreamSubscription = broadcast::Receiver<Bytes>;

pub struct StreamSet {
    stream_outputs: HashMap<String, broadcast::Sender<Bytes>>,
}

impl StreamSet {
    pub fn new(log: Logger, config: &HashMap<String, StreamConfig>, source_set: &SourceSet) -> Self {
        let mut stream_outputs = HashMap::new();

        for (name, config) in config.iter() {
            let (broadcast, _) = broadcast::channel(BUFFER_SIZE);

            let input = match source_set.source_stream(&config.source) {
                Some(source) => source,
                None => {
                    // this should never happen routinely, we've already
                    // validated that all streams are wired to valid sources.
                    // the only way this could happen is if a source thread
                    // dies in between us setting it up and this stream being
                    // set up
                    panic!("could not get source stream: {:?}", &config.source);
                }
            };

            let source = StreamThreadContext {
                config: config.clone(),
                input: input,
                log: log.clone(),
                name: name.clone(),
                output: broadcast.clone(),
            };

            thread::Builder::new()
                .name(format!("edicast/stream: {}", name))
                .spawn(move || stream_thread_main(source))
                .expect("spawn edicast stream thread");

            stream_outputs.insert(name.to_string(), broadcast);
        }

        StreamSet { stream_outputs }
    }

    pub fn subscribe_stream(&self, name: &str) -> Option<StreamSubscription> {
        self.stream_outputs.get(name)
            .map(|subscriber| subscriber.subscribe())
    }
}

pub struct StreamThreadContext {
    config: StreamConfig,
    input: Receiver<Arc<PcmData>>,
    log: Logger,
    name: String,
    output: broadcast::Sender<Bytes>,
}

fn stream_thread_main(stream: StreamThreadContext) {
    let mut codec = encode::from_config(&stream.config.codec);

    slog::info!(stream.log, "Starting stream";
        "codec" => codec.describe(),
        "path" => stream.config.path,
        "source" => stream.config.source,
        "stream" => stream.name,
    );

    loop {
        match stream.input.recv() {
            Ok(pcm) => {
                let encoded = codec.encode(&pcm);
                let _ = stream.output.send(encoded.into());
            }
            Err(RecvError) => {
                panic!("source stream terminated unexpectedly!");
            }
        }
    }
}
