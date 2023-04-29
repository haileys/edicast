use std::io::{self, Read};
use std::str;

use percent_encoding::percent_decode;
use slog::Logger;
use tiny_http::{Method, Response, Request};
use uuid::Uuid;

use crate::audio::decode::{self, PcmRead};
use crate::source::ConnectSourceError;
use super::common;
use super::Edicast;

fn get_header<'a>(req: &'a Request, header_name: &'static str) -> Option<&'a str> {
    req.headers().iter()
        .find(|hdr| hdr.field.equiv(header_name))
        .map(|hdr| hdr.value.as_str())
}

enum MediaType {
    Mp3,
    Ogg,
}

fn init_decoder(media_type: MediaType, io: impl Read + Send + 'static)
    -> Result<Box<dyn PcmRead + Send>, String>
{
    use decode::{Mp3, Ogg};

    match media_type {
        MediaType::Mp3 =>
            Ok(Box::new(Mp3::new(io)) as Box<dyn PcmRead + Send>),
        MediaType::Ogg => {
            match Ogg::new(io) {
                Ok(ogg) => Ok(Box::new(ogg) as Box<dyn PcmRead + Send>),
                Err(err) => Err(err.to_string()),
            }
        }
    }
}

enum SourceKind {
    IcecastLegacy,
    Icecast24Put,
}

pub fn dispatch(req: Request, log: Logger, edicast: &Edicast) {
    let request_id = Uuid::new_v4();
    let log = log.new(slog::o!("request_id" => request_id));

    let url = req.url();

    if url.starts_with("/source/") {
        let source_kind = match req.method() {
            // SOURCE is sent by legacy icecast clients
            Method::NonStandard(method) if method == "SOURCE" => {
                SourceKind::IcecastLegacy
            }
            Method::Put => {
                SourceKind::Icecast24Put
            }
            _ => {
                let _ = common::method_not_allowed(req);
                return;
            }
        };

        let source_name_enc = &url["/source/".len()..];
        let source_name_dec = percent_decode(source_name_enc.as_bytes());
        let source_name = match source_name_dec.decode_utf8() {
            Ok(name) => name,
            Err(_) => {
                // if we couldn't decode the source name as valid UTF-8, it
                // cannot possibly be a valid source name
                let _ = common::not_found(req);
                return;
            }
        };

        let log = log.new(slog::o!("source" => source_name.to_string()));
        slog::info!(log, "Live source connecting";
            common::request_log_keys(&req));

        let content_type = get_header(&req, "Content-Type")
            .and_then(|val| val.split(';').nth(0));

        // verify content type is legit before proceeding
        let media_type = match content_type {
            Some("audio/mpeg") | Some("audio/mp3") => MediaType::Mp3,
            Some("audio/ogg") | Some("application/ogg") => MediaType::Ogg,
            _ => {
                slog::warn!(log, "Unsupported media type for source stream";
                    "content_type" => content_type);

                let _ = common::unsupported_media_type(req);
                return;
            }
        };

        let source = match edicast.sources.connect_source(&source_name, log.clone()) {
            Ok(source) => source,
            Err(ConnectSourceError::NoSuchSource) => {
                slog::warn!(log, "Source does not exist");

                let _ = common::not_found(req);
                return;
            }
            Err(ConnectSourceError::AlreadyConnected) => {
                slog::warn!(log, "Source is already live");

                let _ = common::conflict(req);
                return;
            }
        };

        let decoder_result = match source_kind {
            SourceKind::IcecastLegacy => {
                // responding with connection upgrade is not strictly
                // necessary per the legacy protocol, but is needed to
                // enable the non-standard protocol to work properly
                // through proxies which expect conforming requests
                eprintln!("---> legacy");
                let io = req.upgrade("icecast", Response::empty(200));
                init_decoder(media_type, io)
            }
            SourceKind::Icecast24Put => {
                // tiny-http automatically response 100-Continue for us:
                init_decoder(media_type, RequestBody(req))
            }
        };

        let decoder = match decoder_result {
            Ok(decoder) => decoder,
            Err(msg) => {
                slog::error!(log, "Error initialising decoder";
                    "error" => msg);
                return;
            }
        };

        match source.start(decoder) {
            Ok(()) => {}
            Err(()) => panic!("the source thread must have died or something?"),
        }
    } else {
        let _ = common::not_found(req);
    }
}

struct RequestBody(tiny_http::Request);

impl Read for RequestBody {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let reader = self.0.as_reader();
        reader.read(buf)
    }
}
