use std::collections::HashMap;

use crate::config::Config;
use crate::source::SourceSet;
use crate::stream::StreamSet;

mod public;
mod control;

// fn start_http_interfaces(config: &Config) -> Receiver<HttpAction> {
//     let (tx, rx) = sync_sender(0);
// }

pub struct Edicast {
    pub config: Config,
    pub sources: SourceSet,
    pub streams: StreamSet,
    pub public_routes: HashMap<String, String>,
}

impl Edicast {
    pub fn from_config(config: Config) -> Self {
        let sources = SourceSet::from_config(config.source.clone());

        let streams = StreamSet::from_config(config.stream.clone(), &sources);

        let public_routes = config.stream.iter().map(|(name, config)| {
            (config.path.to_string(), name.to_string())
        }).collect();

        Edicast {
            config,
            sources,
            streams,
            public_routes,
        }
    }
}

#[derive(Debug)]
pub enum StartError {
    Bind(tiny_http::NewServerError),
}

pub fn run(config: Config) -> Result<(), StartError> {
    let public = tiny_http::Server::http(&config.listen.public)
        .map_err(StartError::Bind)?;

    let control = tiny_http::Server::http(&config.listen.control)
        .map_err(StartError::Bind)?;

    let edicast = Edicast::from_config(config);

    crossbeam::scope(|scope| {
        scope.spawn(|scope| {
            for req in public.incoming_requests() {
                let edicast_ref = &edicast;
                scope.spawn(move |_| public::dispatch(req, edicast_ref));
            }
        });

        scope.spawn(|scope| {
            for req in control.incoming_requests() {
                scope.spawn(move |_| control::dispatch(req));
            }
        });

        // for source in config.
    }).expect("scoped thread panicked");

    Ok(())
}
