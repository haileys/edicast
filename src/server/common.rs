use std::io;

use tiny_http::{Request, Response};

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
