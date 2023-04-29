use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::mpsc::{self, SyncSender};

use crossbeam::thread::Scope;
use slog::Logger;
use tiny_http::Request;

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
    Bind(SocketAddr, Box<dyn std::error::Error + Send + Sync + 'static>),
}

fn channel_iterate<'a, T, Iter>(
    scope: &Scope<'a>,
    sender: SyncSender<T>,
    iter: Iter,
) where
    T: Send + 'a,
    Iter: IntoIterator<Item = T> + Send + 'a
{
    scope.spawn(move |_| {
        for item in iter {
            if let Err(_) = sender.send(item) {
                break;
            }
        }
    });
}

enum IncomingRequest {
    Public(Request),
    Control(Request),
}

pub fn run(log: Logger, config: Config) -> Result<(), StartError> {
    slog::info!(log, "Starting edicast";
        "public" => config.listen.public,
        "control" => config.listen.control,
    );

    let public = tiny_http::Server::http(&config.listen.public)
        .map_err(|e| StartError::Bind(config.listen.public, e))?;

    let control = tiny_http::Server::http(&config.listen.control)
        .map_err(|e| StartError::Bind(config.listen.public, e))?;

    let edicast = Edicast::new(log.clone(), config);

    crossbeam::scope(|scope| {
        let requests = {
            let (reqs_tx, reqs_rx) = mpsc::sync_channel(0);

            channel_iterate(scope, reqs_tx.clone(),
                public.incoming_requests().map(IncomingRequest::Public));

            channel_iterate(scope, reqs_tx.clone(),
                control.incoming_requests().map(IncomingRequest::Control));

            reqs_rx
        };

        for req in requests {
            let thread_name = thread_name(&req);

            let result = scope.builder()
                .name(thread_name.clone())
                .spawn({
                    let edicast = &edicast;
                    let log = log.clone();
                    move |_| match req {
                        IncomingRequest::Public(req) => {
                            public::dispatch(req, log, edicast)
                        }
                        IncomingRequest::Control(req) => {
                            control::dispatch(req, log, edicast)
                        }
                    }
                });

            if let Err(e) = result {
                slog::crit!(log, "Could not spawn thread";
                    "error" => format!("{:?}", e),
                    "name" => thread_name,
                );
            }
        }
    }).expect("scoped thread panicked");

    Ok(())
}

fn thread_name(req: &IncomingRequest) -> String {
        let (label, req) = match &req {
            IncomingRequest::Public(req) => {
                ("edicast/public", req)
            }
            IncomingRequest::Control(req) => {
                ("edicast/control", req)
            }
        };

        let remote_addr = req.remote_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();

        let method = req.method();
        let url = req.url();

        format!("{label}: {remote_addr} {method} {url:40}")
}
