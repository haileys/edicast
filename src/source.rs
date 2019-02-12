use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{sync_channel, SyncSender, Receiver};
use std::time::{Instant, Duration};
use std::thread;

use crate::audio::PcmData;
use crate::audio::decode::{PcmRead, PcmReadError};
use crate::config::{OfflineBehaviour, SourceConfig};
use crate::fanout::{live_channel, LivePublisher, LiveSubscriber, LiveSubscription};
use crate::sync::{rendezvous, RendezvousReceiver, RendezvousSender, RecvError, RecvTimeoutError, SendError};

pub enum ConnectSourceError {
    AlreadyConnected,
    NoSuchSource,
}

pub struct SourceSet {
    sources: HashMap<String, Source>
}

impl SourceSet {
    pub fn from_config(config: &HashMap<String, SourceConfig>) -> Self {
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

        SourceSet { sources }
    }

    // This method does not start the source stream directly, but instead
    // expresses intention to start streaming. The object returned allows the
    // caller to either commit to begin streaming, or abort. This is used
    // to reserve the source slot before the caller has a PcmRead available
    // and allows the HTTP server to respond with the right headers in the case
    // that a source stream could not begin without upgrading the connection.
    pub fn connect_source(&self, name: &str) -> Result<StartSource, ConnectSourceError> {
        let source = self.sources.get(name)
            .ok_or(ConnectSourceError::NoSuchSource)?;

        let (tx, rx) = sync_channel(0);

        match source.command.send(NewSource(rx)) {
            Ok(()) => {
                // the source thread is reserved busy for us
                // return a handle to the connecting source to proceed and
                // begin sending audio
                Ok(StartSource { send: tx })
            }
            Err(SendError::Busy) => Err(ConnectSourceError::AlreadyConnected),
            Err(SendError::Disconnected) => panic!("source thread died! wtf! we should restart it!"),
        }
    }

    pub fn source_stream(&self, name: &str) -> Option<LiveSubscription<Arc<PcmData>>> {
        self.sources.get(name)
            .and_then(|source| source.output.subscribe().ok())
    }
}

pub struct StartSource {
    send: SyncSender<Box<PcmRead + Send>>,
}

impl StartSource {
    pub fn start(self, io: Box<PcmRead + Send>) -> Result<(), ()> {
        self.send.send(io).map_err(|_| ())
    }
}

struct NewSource(Receiver<Box<PcmRead + Send>>);

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
                        Ok(cmd) => match incoming_source(&source, &cmd) {
                            Ok(()) => break 'silence_timer,
                            Err(()) => {}
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
                    Ok(cmd) => {
                        let _ = incoming_source(&source, &cmd);
                    }
                    Err(RecvError::Disconnected) => {
                        // sender end disconnected, exit thread
                        return;
                    }
                }
            }
        }
    }
}

fn incoming_source(source: &SourceThreadContext, new_source: &NewSource) -> Result<(), ()> {
    match new_source.0.recv() {
        Ok(mut io) => {
            run_source(source, &mut *io);
            Ok(())
        }
        Err(_) => Err(())
    }
}

fn run_source(source: &SourceThreadContext, io: &mut PcmRead) {
    let mut buffer = Vec::with_capacity(source.config.buffer_samples);

    loop {
        match io.read() {
            Ok(pcm) => {
                buffer.extend(pcm.samples.into_iter());

                while buffer.len() > source.config.buffer_samples {
                    let chonk = buffer.drain(0..source.config.buffer_samples)
                        .collect::<Vec<_>>()
                        .into_boxed_slice();

                    source.output.publish(Arc::new(PcmData {
                        channels: pcm.channels,
                        sample_rate: pcm.sample_rate,
                        samples: chonk,
                    }));
                }
            }
            Err(PcmReadError::Eof) => return,
            Err(e) => {
                eprintln!("Error reading from source in run_source: {:?}", e);
                break;
            }
        }
    }
}
