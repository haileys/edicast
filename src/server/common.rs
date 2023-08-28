use std::io;
use std::net::SocketAddr;

use bytes::Bytes;
use slog::OwnedKVList;
use tiny_http::{Request, Response};
use hyper::StatusCode;
use http_body_util::Full;

use crate::net::SocketPeer;

pub fn request_log_keys(request: &Request) -> OwnedKVList {
    (slog::o!{
        "method" => request.method().to_string(),
        "url" => request.url().to_string(),
        "http_version" => request.http_version().to_string(),
        "remote_addr" => request.remote_addr().map(|a| a.to_string()).unwrap_or_default(),
    }).into()
}

pub fn remote_addr<T>(request: &hyper::Request<T>) -> Option<SocketAddr> {
    request.extensions()
        .get::<SocketPeer>()
        .map(|SocketPeer(addr)| *addr)
}

pub fn request_log_keys_hyper(request: &hyper::Request<impl hyper::body::Body>) -> OwnedKVList {
    (slog::o!{
        "method" => request.method().to_string(),
        "url" => request.uri().to_string(),
        "http_version" => format!("{:?}", request.version()),
        "remote_addr" => remote_addr(request).map(|addr| addr.to_string()).unwrap_or_default(),
    }).into()
}

pub fn not_found(req: Request) -> Result<(), io::Error> {
    req.respond(Response::from_string("Not found")
        .with_status_code(404))
}

pub fn status(code: StatusCode) -> hyper::Response<Full<Bytes>> {
    let text = code.canonical_reason().unwrap_or_default();
    let body = Full::new(Bytes::from_static(text.as_bytes()));

    hyper::Response::builder()
        .status(code)
        .body(body)
        .unwrap()
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
