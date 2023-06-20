/*
    Parse a Rust source file and find all potentially dangerous effects,
    printing them to stdout (one per line).

    Effects are printed in a CSV format -- run --bin csv_header to get
    the header or see effect.rs.
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
    cargo_scan::util::init_logging();
    let args = Args::parse();

    let results = scanner::scan_crate(&args.crate_path)?;

    for effect in results.effects {
        if effect.is_dangerous() {
            println!("{}", effect.to_csv());
        } else if args.verbose {
            println!("Skipping: {}", effect.to_csv());
        }
    }

    if args.verbose {
        if !results.skipped_fn_calls.is_empty() {
            eprintln!(
                "Note: analysis skipped {} LoC of function calls \
                (closures or other complex expressions called as functions)",
                results.skipped_fn_calls.as_loc()
            );
        }
        if !results.skipped_macros.is_empty() {
            eprintln!(
                "Note: analysis skipped {} LoC of macro invocations",
                results.skipped_macros.as_loc()
            );
        }
    }

    Ok(())
}
