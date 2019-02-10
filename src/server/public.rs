use std::io;

use tiny_http::{Request, Response};

use super::Edicast;
use crate::audio::encode;
use crate::fanout::SubscribeError;

fn not_found(req: Request) {
    req.respond(Response::from_string("<h1>Not found</h1>")
        .with_status_code(404));
}

fn dispatch_io(req: Request, edicast: &Edicast) -> Result<(), io::Error> {
    let stream_id = match edicast.public_routes.get(req.url()) {
        Some(stream_id) => stream_id,
        None => {
            not_found(req);
            return Ok(());
        }
    };

    let content_type = encode::mime_type_from_config(
        &edicast.config.stream[stream_id].codec);

    let stream = match edicast.streams.subscribe_stream(stream_id) {
        Some(stream) => stream,
        None => {
            not_found(req);
            return Ok(());
        }
    };

    let mut response = req.into_writer();
    write!(response, "HTTP/1.1 200 OK\r\nContent-Type: {}\r\n\r\n", content_type)?;

    loop {
        match stream.recv() {
            Ok(data) => response.write_all(&data)?,
            Err(SubscribeError::NoPublisher) => {
                // publisher went away, terminate stream
                return Ok(());
            }
        }
    }
}

pub fn dispatch(req: Request, edicast: &Edicast) {
    match dispatch_io(req, edicast) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("broken write to client: {:?}", e);
        }
    }
}
