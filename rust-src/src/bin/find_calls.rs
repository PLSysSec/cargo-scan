/*
Parse a Rust source file and find all function calls, printing them to stdout
(one per line).
*/

use cargo_scan::scanner::Scanner;
use clap::Parser;
use std::fs::File;
use std::io::Read;
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

    // based on example at https://docs.rs/syn/latest/syn/struct.File.html
    let mut file = File::open(&args.filepath).unwrap_or_else(|err| {
        panic!(
            "find_calls.rs: Error: Unable to open file: {:?} ({:?})",
            args.filepath, err
        )
    });
    let mut src = String::new();
    file.read_to_string(&mut src).unwrap_or_else(|err| {
        panic!(
            "find_calls.rs: Error: Unable to read file: {:?} ({:?})",
            args.filepath, err
        )
    });

    let syntax_tree = syn::parse_file(&src).unwrap_or_else(|err| {
        panic!(
            "find_calls.rs: Error: Unable to parse file: {:?} ({:?})",
            args.filepath, err
        )
    });
    let mut scanner = Scanner::new(&args.filepath);
    scanner.scan_file(&syntax_tree);

    // for debugging
    // println!("Final scanner state: {:?}", scanner);

    for result in scanner.results {
        println!("{}", result.to_csv());
    }

    if args.verbose {
        if scanner.skipped_fn_calls > 0 {
            eprintln!(
                "Note: analysis skipped {} function calls \
                (closures or other complex expressions called as functions)",
                scanner.skipped_fn_calls
            );
        }
        if scanner.skipped_macros > 0 {
            eprintln!(
                "Note: analysis skipped {} macro invocations",
                scanner.skipped_macros
            );
        }
    }
}
