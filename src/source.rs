use std::collections::HashMap;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver, RecvError, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Instant, Duration};
use std::thread;

use crate::audio::{PcmData, PcmRead, PcmReadError};
use crate::config::{OfflineBehaviour, SourceConfig};
use crate::fanout::{live_channel, LivePublisher, LiveSubscriber, LiveSubscription};

enum SourceThreadCommand {
    NewSource(Box<PcmRead + Send>),
}

pub struct SourceSet {
    config: HashMap<String, SourceConfig>,
    source_threads: HashMap<String, SyncSender<SourceThreadCommand>>,
    source_outputs: HashMap<String, LiveSubscriber<Arc<PcmData>>>,
}

impl SourceSet {
    pub fn from_config(config: HashMap<String, SourceConfig>) -> Self {
        let mut source_outputs = HashMap::new();
        let mut source_threads = HashMap::new();

        for (name, config) in config.iter() {
            let (cmd_tx, cmd_rx) = sync_channel(0);
            let (publisher, subscriber) = live_channel();

            let source = Source {
                command: cmd_rx,
                config: config.clone(),
                output: publisher,
            };

            thread::spawn(move || source_thread_main(source));

            source_threads.insert(name.to_string(), cmd_tx);
            source_outputs.insert(name.to_string(), subscriber);
        }

        SourceSet {
            config,
            source_threads,
            source_outputs,
        }
    }

    pub fn source_stream(&self, name: &str) -> Option<LiveSubscription<Arc<PcmData>>> {
        self.source_outputs.get(name)
            .and_then(|subscriber| subscriber.subscribe().ok())
    }
}

struct Source {
    command: Receiver<SourceThreadCommand>,
    config: SourceConfig,
    output: LivePublisher<Arc<PcmData>>,
}

fn source_thread_main(source: Source) {
    match source.config.offline {
        OfflineBehaviour::Silence => {
            let silence_duration = Duration::from_millis(100);
            let silence = Arc::new(PcmData {
                // silence_duration is 100 milliseconds, so make 4,410 samples
                // of zero (assuming sample rate is 44.1 kHz)
                left: Box::new([0i16; 4410]),
                right: Box::new([0i16; 4410]),
            });

            loop {
                let epoch = Instant::now();
                let duration = silence_duration;

                'silence_timer: loop {
                    let now = Instant::now();

                    let deadline = epoch + duration;

                    let timeout = if deadline > now {
                        deadline - now
                    } else {
                        Duration::from_secs(0)
                    };

                    match source.command.recv_timeout(timeout) {
                        Ok(SourceThreadCommand::NewSource(mut io)) => {
                            run_source(&source, &mut *io);
                            break 'silence_timer;
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
                    Ok(SourceThreadCommand::NewSource(mut io)) => {
                        run_source(&source, &mut *io)
                    }
                    Err(RecvError) => {
                        // sender end disconnected, exit thread
                        break;
                    }
                }
            }
        }
    }
}

fn run_source(source: &Source, io: &mut PcmRead) {
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
