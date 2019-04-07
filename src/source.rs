use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::thread;
use std::time::{Instant, Duration};

use crossbeam_channel::{Sender, Receiver};
use num_rational::Ratio;
use slog::Logger;

use crate::audio::PcmData;
use crate::audio::decode::{PcmRead, PcmReadError};
use crate::config::{OfflineBehaviour, SourceConfig};
use crate::fanout::{live_channel, LivePublisher, LiveSubscriber};
use crate::sync::{rendezvous, RendezvousReceiver, RendezvousSender, RecvError, RecvTimeoutError, SendError};

pub enum ConnectSourceError {
    AlreadyConnected,
    NoSuchSource,
}

pub struct SourceSet {
    sources: HashMap<String, Source>
}

impl SourceSet {
    pub fn new(log: Logger, config: &HashMap<String, SourceConfig>) -> Self {
        let mut sources = HashMap::new();

        for (name, config) in config.iter() {
            let (cmd_send, cmd_recv) = rendezvous();
            let (publisher, subscriber) = live_channel();

            let thread_context = SourceThreadContext {
                name: name.clone(),
                command: cmd_recv,
                config: config.clone(),
                log: log.clone(),
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
    pub fn connect_source(&self, name: &str, log: Logger) -> Result<StartSource, ConnectSourceError> {
        let source = self.sources.get(name)
            .ok_or(ConnectSourceError::NoSuchSource)?;

        let (tx, rx) = crossbeam_channel::bounded(0);

        match source.command.send(NewSource { log, rx }) {
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

    pub fn source_stream(&self, name: &str) -> Option<Receiver<Arc<PcmData>>> {
        self.sources.get(name)
            .and_then(|source| source.output.subscribe().ok())
    }
}

pub struct StartSource {
    send: Sender<Box<PcmRead + Send>>,
}

impl StartSource {
    pub fn start(self, io: Box<PcmRead + Send>) -> Result<(), ()> {
        self.send.send(io).map_err(|_| ())
    }
}

struct NewSource {
    log: Logger,
    rx: Receiver<Box<PcmRead + Send>>
}

struct Source {
    command: RendezvousSender<NewSource>,
    output: LiveSubscriber<Arc<PcmData>>,
}

struct SourceThreadContext {
    name: String,
    command: RendezvousReceiver<NewSource>,
    config: SourceConfig,
    log: Logger,
    output: LivePublisher<Arc<PcmData>>,
}

fn source_thread_main(source: SourceThreadContext) {
    slog::info!(source.log, "Starting source"; "source" => &source.name);

    match source.config.offline {
        OfflineBehaviour::Silence => {
            let silence_duration = Duration::from_millis(source.config.buffer_ms as u64);
            let silence = Arc::new(PcmData::silence(silence_duration));

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
    match new_source.rx.recv() {
        Ok(mut io) => {
            let epoch = Instant::now();

            let result = run_source(source, epoch, &mut *io);

            let duration = Instant::now() - epoch;

            match result {
                Ok(()) => {
                    slog::info!(new_source.log, "Live source finished"; "duration_sec" => duration.as_secs());
                }
                Err(e) => {
                    slog::error!(new_source.log, "I/O error reading from live source";
                        "error" => e.to_string(),
                        "duration_sec" => duration.as_secs(),
                    );
                }
            }

            Ok(())
        }
        Err(_) => Err(())
    }
}

fn sleep_until(deadline: Instant) {
    let now = Instant::now();

    if deadline > now {
        thread::sleep(deadline - now);
    }
}

fn run_source(source: &SourceThreadContext, epoch: Instant, io: &mut PcmRead)
    -> Result<(), io::Error>
{
    let mut elapsed = Ratio::new(0u64, 1u64);
    let mut buffer = Vec::new();

    loop {
        let elapsed_nanos = (elapsed * Ratio::new(1_000_000_000, 1)).to_integer();
        sleep_until(epoch + Duration::from_nanos(elapsed_nanos));

        match io.read() {
            Ok(pcm) => {
                buffer.extend(pcm.samples.into_iter());

                let buffer_samples = source.config.buffer_ms * pcm.sample_rate / 1000;

                while buffer.len() > buffer_samples {
                    let chonk = buffer.drain(0..buffer_samples)
                        .collect::<Vec<_>>()
                        .into_boxed_slice();

                    source.output.publish(Arc::new(PcmData {
                        channels: pcm.channels,
                        sample_rate: pcm.sample_rate,
                        samples: chonk,
                    }));
                }

                elapsed += Ratio::<u64>::new(
                    (pcm.samples.len() / pcm.channels) as u64,
                    pcm.sample_rate as u64);
            }
            Err(PcmReadError::Eof) => {
                return Ok(());
            }
            Err(PcmReadError::SkippedData) => {
                // just ignore and read again, may be metadata
            }
            Err(PcmReadError::Io(e)) => {
                return Err(e);
            }
        }
    }
}
