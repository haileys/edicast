use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{Receiver, RecvError};
use slog::Logger;

use crate::audio::PcmData;
use crate::audio::encode;
use crate::config::StreamConfig;
use crate::fanout::{live_channel, LivePublisher, LiveSubscriber};
use crate::source::SourceSet;

pub type StreamSubscription = Receiver<Arc<[u8]>>;

pub struct StreamSet {
    stream_outputs: HashMap<String, LiveSubscriber<Arc<[u8]>>>,
}

impl StreamSet {
    pub fn new(log: Logger, config: &HashMap<String, StreamConfig>, source_set: &SourceSet) -> Self {
        let mut stream_outputs = HashMap::new();

        for (name, config) in config.iter() {
            let (publisher, subscriber) = live_channel();

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
                output: publisher,
            };

            thread::spawn(move || stream_thread_main(source));

            stream_outputs.insert(name.to_string(), subscriber);
        }

        StreamSet { stream_outputs }
    }

    pub fn subscribe_stream(&self, name: &str) -> Option<StreamSubscription> {
        self.stream_outputs.get(name)
            .and_then(|subscriber| subscriber.subscribe().ok())
    }
}

pub struct StreamThreadContext {
    config: StreamConfig,
    input: Receiver<Arc<PcmData>>,
    log: Logger,
    name: String,
    output: LivePublisher<Arc<[u8]>>,
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
                stream.output.publish(encoded.into());
            }
            Err(RecvError) => {
                panic!("source stream terminated unexpectedly!");
            }
        }
    }
}
