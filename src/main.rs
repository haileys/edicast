mod audio;
mod config;
mod fanout;
mod server;
mod source;
mod stream;
mod sync;

use std::env;
use std::path::PathBuf;
use std::process;

use config::Config;

fn config_path() -> PathBuf {
    match env::args_os().nth(1) {
        Some(path) => path.into(),
        None => {
            eprintln!("usage: edicast <config file>");
            process::exit(1);
        }
    }
}

fn main() {
    let config_path = config_path();

    let config = match Config::load(&config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("could not load {:?}: {:?}", config_path, e);
            process::exit(1);
        }
    };

    server::run(config).expect("server::run");
}
