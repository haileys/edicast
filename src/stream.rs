use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};
use std::thread;

use crate::audio::PcmData;
use crate::audio::encode;
use crate::config::StreamConfig;
use crate::fanout::{live_channel, LiveSubscriber, LiveSubscription, LivePublisher, SubscribeError};
use crate::source::SourceSet;

enum StreamThreadCommand {}

pub struct StreamSet {
    config: HashMap<String, StreamConfig>,
    stream_outputs: HashMap<String, LiveSubscriber<Arc<[u8]>>>,
    stream_threads: HashMap<String, SyncSender<StreamThreadCommand>>,
}

impl StreamSet {
    pub fn from_config(config: HashMap<String, StreamConfig>, source_set: &SourceSet) -> Self {
        let mut stream_outputs = HashMap::new();
        let mut stream_threads = HashMap::new();

        for (name, config) in config.iter() {
            let (cmd_tx, cmd_rx) = sync_channel(0);
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

            let source = Stream {
                command: cmd_rx,
                config: config.clone(),
                input: input,
                output: publisher,
            };

            thread::spawn(move || stream_thread_main(source));

            stream_threads.insert(name.to_string(), cmd_tx);
            stream_outputs.insert(name.to_string(), subscriber);
        }

        StreamSet {
            config,
            stream_outputs,
            stream_threads,
        }
    }

    pub fn subscribe_stream(&self, name: &str) -> Option<LiveSubscription<Arc<[u8]>>> {
        self.stream_outputs.get(name)
            .and_then(|subscriber| subscriber.subscribe().ok())
    }
}

pub struct Stream {
    command: Receiver<StreamThreadCommand>,
    config: StreamConfig,
    input: LiveSubscription<Arc<PcmData>>,
    output: LivePublisher<Arc<[u8]>>,
}

fn stream_thread_main(stream: Stream) {
    let mut codec = encode::from_config(&stream.config.codec);

    loop {
        match stream.input.recv() {
            Ok(pcm) => {
                let encoded = codec.encode(&pcm);
                stream.output.publish(encoded.into());
            }
            Err(SubscribeError::NoPublisher) => {
                panic!("source stream terminated unexpectedly!");
            }
        }
    }
}
