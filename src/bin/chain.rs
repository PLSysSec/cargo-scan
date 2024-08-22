/*
    This binary is intended for internal use.

    The main supported binaries are `--bin scan` and `--bin audit`.
    See README.md for usage instructions.
*/

use cargo_scan::auditing::chain::{Command, CommandRunner, OuterArgs};
use clap::Parser;

#[derive(Parser, Debug)]
struct Args {
    #[clap(flatten)]
    outer_args: OuterArgs,

    #[clap(subcommand)]
    command: Command,
}

fn main() {
    eprintln!("Warning: `--bin chain` is not recommended. The primary supported binaries are `--bin scan` and `--bin audit`.");

    cargo_scan::util::init_logging();
    let args = Args::parse();

    match args.command.run_command(args.outer_args) {
        Ok(()) => (),
        Err(e) => println!("Error running command: {}", e),
    }
}
