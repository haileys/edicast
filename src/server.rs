use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use slog::Logger;
use thiserror::Error;

use crate::config::Config;
use crate::net;
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

#[derive(Error, Debug)]
pub enum StartError {
    #[error("could not bind {0}: {1}")]
    Bind(SocketAddr, Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error(transparent)]
    Public(#[from] net::BindError),
}

pub async fn run(log: Logger, config: Config) -> Result<(), StartError> {
    slog::info!(log, "Starting edicast";
        "public" => config.listen.public,
        "control" => config.listen.control,
    );

    let edicast = Arc::new(Edicast::new(log.clone(), config));

    // run public server
    let public = public::start(edicast.config.listen.public, edicast.clone()).await?;

    // setup + run control server
    let control_listener = tiny_http::Server::http(&edicast.config.listen.control)
        .map_err(|e| StartError::Bind(edicast.config.listen.control, e))?;

    let control = crate::thread::spawn_worker("edicast/control", async move {
        crossbeam::scope(|scope| {
            for req in control_listener.incoming_requests() {
                let thread_name = thread_name(&req);

                let result = scope.builder()
                    .name(thread_name.clone())
                    .spawn({
                        let edicast = &edicast;
                        let log = log.clone();
                        move |_| control::dispatch(req, log, edicast)
                    });

                if let Err(e) = result {
                    slog::crit!(log, "Could not spawn thread";
                        "error" => format!("{:?}", e),
                        "name" => thread_name,
                    );
                }
            }
        }).expect("scoped thread panicked");
    });

    futures::future::join(public, control).await;
    Ok(())
}

fn thread_name(req: &tiny_http::Request) -> String {
        let remote_addr = req.remote_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();

        let method = req.method();
        let url = req.url();

        format!("edicast/control: {remote_addr} {method} {url:40}")
}
