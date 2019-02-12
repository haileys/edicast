use std::io;
use std::str;

use percent_encoding::percent_decode;
use tiny_http::{Method, Response, Request};

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

pub fn dispatch(req: Request, edicast: &Edicast) -> Result<(), io::Error> {
    let url = req.url();

    if url.starts_with("/source/") {
        match req.method() {
            Method::Source | Method::Put => {},
            _ => return common::method_not_allowed(req),
        };

        let source_name_enc = &url["/source/".len()..];
        let source_name_dec = percent_decode(source_name_enc.as_bytes());
        let source_name = match source_name_dec.decode_utf8() {
            Ok(name) => name,
            Err(_) => {
                // if we couldn't decode the source name as valid UTF-8, it
                // cannot possibly be a valid source name
                return common::not_found(req);
            }
        };

        let content_type = get_header(&req, "Content-Type")
            .and_then(|val| val.split(';').nth(0));

        // verify content type is legit before proceeding
        let media_type = match content_type {
            Some("audio/mpeg") | Some("audio/mp3") => MediaType::Mp3,
            Some("audio/ogg") | Some("application/ogg") => MediaType::Ogg,
            _ => return common::unsupported_media_type(req),
        };

        let source = match edicast.sources.connect_source(&source_name) {
            Ok(source) => source,
            Err(ConnectSourceError::NoSuchSource) => return common::not_found(req),
            Err(ConnectSourceError::AlreadyConnected) => return common::conflict(req),
        };

        let io = req.upgrade("icecast", Response::empty(200));

        let pcm_read = match media_type {
            MediaType::Mp3 =>
                Box::new(decode::Mp3::new(io)) as Box<PcmRead + Send>,
            MediaType::Ogg => {
                match decode::Ogg::new(io) {
                    Ok(ogg) => Box::new(ogg) as Box<PcmRead + Send>,
                    Err(_) => {
                        // there was some issue reading the start of the stream
                        // just kick the client and abort the connect_source
                        return Ok(());
                    }
                }
            }
        };

        match source.start(pcm_read) {
            Ok(()) => return Ok(()),
            Err(()) => panic!("the source thread must have died or something?"),
        }
    }

    common::not_found(req)
}
