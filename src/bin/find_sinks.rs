/*
    Parse a Rust source file and find all function calls
    which are of interest according to sinks.rs,
    printing them to stdout (one per line).
*/

use cargo_scan::scanner;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to crate directory; should contain a 'src' directory and a Cargo.toml file
    crate_path: PathBuf,
    /// Show verbose output
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let results = scanner::scan_crate(&args.crate_path)?;

    for effect in results.effects {
        if effect.is_sink() {
            println!("{}", effect.to_csv());
        } else if args.verbose {
            eprintln!("Skipping: {}", effect.to_csv());
        }
    }

    if args.verbose {
        if results.skipped_fn_calls > 0 {
            eprintln!(
                "Note: analysis skipped {} function calls \
                (closures or other complex expressions called as functions)",
                results.skipped_fn_calls
            );
        }
        if results.skipped_macros > 0 {
            eprintln!(
                "Note: analysis skipped {} macro invocations",
                results.skipped_macros
            );
        }
    }

    Ok(())
}
