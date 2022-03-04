mod config;

mod process_manager;
mod server;

use config::*;
use thiserror::Error;

const HELP: &str = "\
dairi


USAGE:
  dairi [OPTIONS]

FLAGS:
  -h, --help            Prints help information
";

#[derive(Debug, Error)]
pub enum DairiError {
    #[error("{0}")]
    ArgsError(#[from] pico_args::Error),
}
pub struct Args {}

#[cfg(unix)]
#[tokio::main]
async fn main() {
    let _args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("args error : {}", e);
            std::process::exit(1);
        }
    };

    tracing_subscriber::fmt::init();

    let config = match Config::load_from_default_path_or_create() {
        Ok(config) => config,
        Err(e) => {
            tracing::error!("{}", e);
            std::process::exit(1);
        }
    };

    if let Err(e) = process_manager::init_cmd_table(config.as_cmd_table()) {
        tracing::error!("failed to init cmd table:{:?}", e);
        std::process::exit(1);
    };

    if let Err(e) = server::serve().await {
        tracing::error!("dairi server error: {}", e);
    }
}

fn parse_args() -> Result<Args, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    Ok(Args {})
}

#[cfg(not(unix))]
fn main() {
    println!("dairi run on unix domain socket")
}
