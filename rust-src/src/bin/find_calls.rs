/*
    Parse a Rust source file and find all function calls, printing them to stdout
    (one per line).
*/

use cargo_scan::scanner;
use clap::Parser;

use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // ../path/to/my_rust_crate/src/my_mod/my_file.rs
    filepath: PathBuf,
    #[arg(short, long, default_value_t = false)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    let results = scanner::load_and_scan(&args.filepath);

    for effect in results.effects {
        println!("{}", effect.to_csv());
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
}
