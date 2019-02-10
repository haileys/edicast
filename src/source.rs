use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::thread;

use crate::audio::PcmData;
use crate::audio::decode::{PcmRead, PcmReadError};
use crate::config::{OfflineBehaviour, SourceConfig};
use crate::fanout::{live_channel, LivePublisher, LiveSubscriber, LiveSubscription};
use crate::sync::{rendezvous, RendezvousReceiver, RendezvousSender, RecvError, RecvTimeoutError, SendError};

struct NewSource(Box<PcmRead + Send>);

pub enum ConnectSourceError {
    AlreadyConnected,
    NoSuchSource,
}

pub struct SourceSet {
    config: HashMap<String, SourceConfig>,
    sources: HashMap<String, Source>
}

impl SourceSet {
    pub fn from_config(config: HashMap<String, SourceConfig>) -> Self {
        let mut sources = HashMap::new();

        for (name, config) in config.iter() {
            let (cmd_send, cmd_recv) = rendezvous();
            let (publisher, subscriber) = live_channel();

            let thread_context = SourceThreadContext {
                command: cmd_recv,
                config: config.clone(),
                output: publisher,
            };

            let source = Source {
                command: cmd_send,
                output: subscriber,
            };

            thread::spawn(move || source_thread_main(thread_context));

            sources.insert(name.to_string(), source);
        }

        SourceSet {
            config,
            sources,
        }
    }

    pub fn connect_source(&self, name: &str, io: Box<PcmRead + Send>) -> Result<(), ConnectSourceError> {
        let source = self.sources.get(name)
            .ok_or(ConnectSourceError::NoSuchSource)?;

        match source.command.send(NewSource(io)) {
            Ok(()) => Ok(()),
            Err(SendError::Busy) => Err(ConnectSourceError::AlreadyConnected),
            Err(SendError::Disconnected) => panic!("source thread died! wtf! we should restart it!"),
        }
    }

    pub fn source_stream(&self, name: &str) -> Option<LiveSubscription<Arc<PcmData>>> {
        self.sources.get(name)
            .and_then(|source| source.output.subscribe().ok())
    }
}

struct Source {
    command: RendezvousSender<NewSource>,
    output: LiveSubscriber<Arc<PcmData>>,
}

struct SourceThreadContext {
    command: RendezvousReceiver<NewSource>,
    config: SourceConfig,
    output: LivePublisher<Arc<PcmData>>,
}

fn source_thread_main(source: SourceThreadContext) {
    match source.config.offline {
        OfflineBehaviour::Silence => {
            let silence_duration = Duration::from_millis(100);
            let silence = Arc::new(PcmData {
                sample_rate: 44100,
                channels: 2,
                samples: Box::new([0i16; 44100 / 10 * 2]),
            });

            loop {
                let epoch = Instant::now();
                let mut duration = Duration::from_secs(0);

                'silence_timer: loop {
                    duration += silence_duration;

                    match source.command.recv_deadline(epoch + duration) {
                        Ok(mut cmd) => match *cmd {
                            NewSource(ref mut io) => {
                                run_source(&source, &mut **io);
                                break 'silence_timer;
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            source.output.publish(Arc::clone(&silence));
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            // command sender end disconnected, exit thread
                            return;
                        }
                    }
                }
            }
        }
        OfflineBehaviour::Inactive => {
            loop {
                match source.command.recv() {
                    Ok(mut cmd) => match *cmd {
                        NewSource(ref mut io) => run_source(&source, &mut **io),
                    }
                    Err(RecvError::Disconnected) => {
                        // sender end disconnected, exit thread
                        break;
                    }
                }
            }
        }
    }
}

fn run_source(source: &SourceThreadContext, io: &mut PcmRead) {
    loop {
        match io.read() {
            Ok(pcm) => source.output.publish(Arc::new(pcm)),
            Err(PcmReadError::Eof) => return,
            Err(e) => {
                eprintln!("Error reading from source in run_source: {:?}", e);
                break;
            }
        }
    }
}
