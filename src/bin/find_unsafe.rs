/*
    Parse a Rust source file and find all instances of unsafe
    code or FFI calls, printing them to stdout.
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

    if !results.effect_blocks.is_empty() {
        println!("=== Effect blocks ===");
        for bl_decl in results.effect_blocks {
            println!("{:?}", bl_decl);
        }
    }

    if !results.unsafe_traits.is_empty() {
        println!("=== Unsafe trait declarations ===");
        for tr_decl in results.unsafe_traits {
            println!("{:?}", tr_decl);
        }
    }

    if !results.unsafe_impls.is_empty() {
        println!("=== Unsafe trait impls ===");
        for impl_decl in results.unsafe_impls {
            println!("{:?}", impl_decl);
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
