use std::io::{self, ErrorKind};

use slog::Logger;
use tiny_http::Request;
use uuid::Uuid;

use super::common;
use super::Edicast;
use crate::audio::encode;

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

    let mut response = req.into_writer();
    write!(response, "HTTP/1.1 200 OK\r\nContent-Type: {}\r\n\r\n", content_type)?;

    loop {
        match stream.recv() {
            Ok(data) => {
                response.write_all(&data)?;
            }
            Err(_) => {
                // publisher went away, terminate stream
                return Ok(RunResult::StreamEnd);
            }
        }
    }
}
