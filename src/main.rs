mod audio;
mod config;
mod fanout;
mod net;
mod server;
mod source;
mod stream;
mod sync;
mod thread;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

use std::env;
use std::path::{Path, PathBuf};
use std::process;

use slog::{Drain, Logger};

use config::Config;

fn logger() -> Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();
    Logger::root(drain, slog::o!())
}

fn config_path() -> PathBuf {
    match env::args_os().nth(1) {
        Some(path) => path.into(),
        None => {
            eprintln!("usage: edicast <config file>");
            process::exit(1);
        }
    }
}

fn handle_config_error(log: &Logger, config_path: &Path, err: config::Error) {
    use config::Error;

    match err {
        Error::Io(err) => {
            slog::error!(log, "Could not read config file";
                "path" => config_path.display(),
                "error" => err.to_string(),
            );
        }
        Error::Toml(err) => {
            slog::error!(log, "Could not parse config file";
                "path" => config_path.display(),
                "error" => err.to_string(),
            );
        }
        Error::StreamRefersToInvalidSource { stream_name, source_name } => {
            slog::error!(log, "Invalid source in stream config";
                "path" => config_path.display(),
                "source" => source_name,
                "stream" => stream_name,
            );
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // this inner function makes sure Logger instance is cleanly dropped and
    // any logged errors are properly flushed before we call process::exit
    async fn run() -> Result<(), ()> {
        let log = logger();
        let _ = slog_scope::set_global_logger(log.clone());

        let config_path = config_path();

        let config = match Config::load(&config_path) {
            Ok(config) => config,
            Err(e) => {
                handle_config_error(&log, &config_path, e);
                slog::crit!(log, "Error loading initial config");
                return Err(());
            }
        };

        match server::run(log.clone(), config).await {
            Ok(()) => {}
            Err(error) => {
                slog::crit!(log, "Error running server: {}", error);
                return Err(());
            }
        }

        Ok(())
    }

    match run().await {
        Ok(()) => {}
        Err(()) => process::exit(1),
    }
}
