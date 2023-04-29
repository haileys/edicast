use std::io;

use slog::OwnedKVList;
use tiny_http::{Request, Response};

pub fn request_log_keys(request: &Request) -> OwnedKVList {
    (slog::o!{
        "method" => request.method().to_string(),
        "url" => request.url().to_string(),
        "http_version" => request.http_version().to_string(),
        "remote_addr" => request.remote_addr().map(|a| a.to_string()).unwrap_or_default(),
    }).into()
}

pub fn not_found(req: Request) -> Result<(), io::Error> {
    req.respond(Response::from_string("Not found")
        .with_status_code(404))
}

pub fn method_not_allowed(req: Request) -> Result<(), io::Error> {
    req.respond(Response::from_string("Method not allowed")
        .with_status_code(405))
}

pub fn conflict(req: Request) -> Result<(), io::Error> {
    req.respond(Response::from_string("Conflict")
        .with_status_code(409))
}

pub fn unsupported_media_type(req: Request) -> Result<(), io::Error> {
    req.respond(Response::from_string("Unsupported media type")
        .with_status_code(415))
}
