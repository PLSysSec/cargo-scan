/*
    Parse a Rust source file and find all potentially dangerous effects,
    printing them to stdout (one per line).

    Effects are printed in a CSV format -- run --bin csv_header to get
    the header or see effect.rs.
*/

use cargo_scan::audit_file::AuditFile;
use cargo_scan::effect::{EffectInstance, EffectType, DEFAULT_EFFECT_TYPES};
use cargo_scan::loc_tracker::LoCTracker;

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

    // Turned off for now -- chain binary not being used
    // /// Include transitive effects in dependency crates
    // #[arg(short, long, default_value_t = false)]
    // transitive: bool,
    /// Path to download crates to for auditing
    #[clap(short = 'd', long = "crate-download-path", default_value = ".stats_tmp")]
    crate_download_path: String,

    /// The types of Effects the audit should track. Defaults to all unsafe
    /// behavior.
    #[clap(long, value_parser, num_args = 1.., default_values_t = DEFAULT_EFFECT_TYPES)]
    effect_types: Vec<EffectType>,
}

fn main() -> Result<()> {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    let (_audit, results) = AuditFile::new_caller_checked_default_with_results(
        &args.crate_path,
        &args.effect_types,
    )?;

    // Note: old version without default_audit:
    // scanner::scan_crate(&args.crate_path, &args.effect_types)?

    println!("{}", EffectInstance::csv_header());
    for effect in results.effects {
        println!("{}", effect.to_csv());
    }

    // TODO: print out all the metadata to stderr :-)

    if args.verbose {
        eprintln!("Total LoC scanned: {}", results.total_loc.as_loc());

        fn print_ignored_items<T>(ignored: &[T], msg: &str) {
            if !ignored.is_empty() {
                eprintln!("Note: analysis ignored {} {}", ignored.len(), msg);
            }
        }
        fn print_skipped_loc(loc: &LoCTracker, msg: &str) {
            if !loc.is_empty() {
                eprintln!("Note: analysis skipped {} LoC of {}", loc.as_loc(), msg);
            }
        }

        print_ignored_items(&results.unsafe_traits, "unsafe traits");
        print_ignored_items(&results.unsafe_impls, "unsafe trait impls");
        print_skipped_loc(&results.skipped_macros, "macro invocations");
        print_skipped_loc(&results.skipped_conditional_code, "conditional code");
        print_skipped_loc(
            &results.skipped_fn_calls,
            "function calls (closures or other \
            complex expressions called as functions)",
        );
        print_skipped_loc(&results.skipped_fn_ptrs, "function pointers");
        print_skipped_loc(&results.skipped_other, "other unsupported code");
    }

    Ok(())
}
