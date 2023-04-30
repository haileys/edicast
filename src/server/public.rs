use std::io::{self, ErrorKind, Read, Cursor};
use std::sync::Arc;

use slog::Logger;
use tiny_http::{Request, Response, StatusCode, Header};
use uuid::Uuid;

use super::common;
use super::Edicast;
use crate::audio::encode;
use crate::stream::StreamSubscription;

pub fn dispatch(req: Request, log: Logger, edicast: &Edicast) {
    let request_id = Uuid::new_v4();
    let log = log.new(slog::o!("request_id" => request_id));

    match run_listener(req, log.clone(), edicast) {
        Ok(RunResult::NotFound) => {
            // don't print listener disconnected, we haven't printed listener connected yet
        }
        Ok(RunResult::StreamEnd) => {
            // don't print listener disconnected since there could be a large
            // number all at once. we'll log when instead ending the stream
        }
        Err(ref e) if e.kind() == ErrorKind::BrokenPipe => {
            slog::info!(log, "Listener disconnected")
        }
        Err(e) => {
            slog::warn!(log, "I/O error writing to listener, dropping";
                "error" => e.to_string());
        }
    }
}

enum RunResult {
    NotFound,
    StreamEnd,
}

fn run_listener(req: Request, log: Logger, edicast: &Edicast) -> Result<RunResult, io::Error> {
    let stream_id = match edicast.public_routes.get(req.url()) {
        Some(stream_id) => stream_id,
        None => {
            let _ = common::not_found(req);
            return Ok(RunResult::NotFound);
        }
    };

    let content_type = encode::mime_type_from_config(
        &edicast.config.stream[stream_id].codec);

    let stream = match edicast.streams.subscribe_stream(stream_id) {
        Some(stream) => stream,
        None => {
            let _ = common::not_found(req);
            return Ok(RunResult::NotFound);
        }
    };

    slog::info!(log, "Listener connected";
        "stream" => stream_id,
        common::request_log_keys(&req),
    );

    req.respond(Response::new(
        StatusCode(200),
        vec![
            Header::from_bytes("content-type".as_bytes(), content_type.as_bytes()).unwrap(),
        ],
        Reader::new(stream),
        None,
        None,
    ))?;

    Ok(RunResult::StreamEnd)
}

struct Reader {
    recv: StreamSubscription,
    buffer: Option<Cursor<Arc<[u8]>>>,
}

impl Reader {
    pub fn new(recv: StreamSubscription) -> Self {
        Reader { recv, buffer: None }
    }
}

impl Read for Reader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut written = 0;

        while written < out.len() {
            match self.buffer.as_mut() {
                Some(buffer) => {
                    match buffer.read(&mut out[written..])? {
                        0 => { self.buffer = None; }
                        n => { written += n; }
                    }
                }
                None => {
                    let buffer = match written {
                        // block on receiving next chunk if we've not yet
                        // returned anything to the caller:
                        0 => self.recv.recv().ok(),
                        // attempt to read more data if we have, but don't
                        // block:
                        _ => match self.recv.try_recv() {
                            Ok(buf) => Some(buf),
                            Err(_) => { break; }
                        },
                    };

                    self.buffer = buffer.map(Cursor::new);
                }
            }
        }

        Ok(written)
    }
}
