mod db;
mod process_manager;

use std::path::PathBuf;

#[derive(Debug)]
pub enum Cmd {
    Run,
    Clear,
    List,
}

impl Cmd {
    pub fn try_from_str(s: &str) -> Result<Self, String> {
        match s {
            "run" => Ok(Self::Run),
            "list" => Ok(Self::List),
            "clear" => Ok(Self::Clear),
            cmd => Err(format!("unknown cmd: {cmd}")),
        }
    }
}

const HELP: &str = "\
dairi

USAGE:
  dairi [OPTIONS] [CMD]

FLAGS:
  -h, --help            Prints help information
OPTIONS:
  --db path_to_db       database path
ARGS:
  <CMD> [run | clear | list]
";

#[derive(Debug)]
struct Args {
    db_path: Option<PathBuf>,
    command: Cmd,
}

#[tokio::main]
async fn main() {
    let args = match parse_args() {
        Ok(args) => args,
        Err(e) => {
            eprintln!("args error : {}", e);
            std::process::exit(1);
        }
    };
}

fn parse_args() -> Result<Args, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    Ok(Args {
        db_path: pargs.opt_value_from_str("--db")?,
        command: pargs.free_from_fn(parse_cmd)?,
    })
}

fn parse_cmd(s: &str) -> Result<Cmd, String> {
    Cmd::try_from_str(s)
}
