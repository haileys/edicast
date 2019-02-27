use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;

use slog::Logger;
use tiny_http::NewServerError;

use crate::config::Config;
use crate::source::SourceSet;
use crate::stream::StreamSet;

mod common;
mod control;
mod public;

pub struct Edicast {
    pub config: Config,
    pub public_routes: HashMap<String, String>,
    pub sources: SourceSet,
    pub streams: StreamSet,
}

impl Edicast {
    pub fn new(log: Logger, config: Config) -> Self {
        let sources = SourceSet::new(log.clone(), &config.source);

        let streams = StreamSet::new(log.clone(), &config.stream, &sources);

        let public_routes = config.stream.iter().map(|(name, config)| {
            (config.path.to_string(), name.to_string())
        }).collect();

        Edicast {
            config,
            public_routes,
            sources,
            streams,
        }
    }
}

#[derive(Debug)]
pub enum StartError {
    Bind(SocketAddr, io::Error),
}

pub fn run(log: Logger, config: Config) -> Result<(), StartError> {
    slog::info!(log, "Starting edicast";
        "public" => config.listen.public,
        "control" => config.listen.control,
    );

    let public = tiny_http::Server::http(&config.listen.public)
        .map_err(|NewServerError::Io(e)|
            StartError::Bind(config.listen.public, e))?;

    let control = tiny_http::Server::http(&config.listen.control)
        .map_err(|NewServerError::Io(e)|
            StartError::Bind(config.listen.public, e))?;

    let edicast = Edicast::new(log.clone(), config);

    crossbeam::scope(|scope| {
        scope.spawn(|scope| {
            for req in public.incoming_requests() {
                let edicast_ref = &edicast;
                let log = log.clone();
                scope.spawn(move |_|
                    public::dispatch(req, log, edicast_ref));
            }
        });

        scope.spawn(|scope| {
            for req in control.incoming_requests() {
                let edicast_ref = &edicast;
                let log = log.clone();
                scope.spawn(move |_|
                    control::dispatch(req, log, edicast_ref));
            }
        });
    }).expect("scoped thread panicked");

    Ok(())
}
