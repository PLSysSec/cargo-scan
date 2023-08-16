/*
    Parse a Rust source file and find all potentially dangerous effects,
    printing them to stdout (one per line).

    Effects are printed in a CSV format -- run --bin csv_header to get
    the header or see effect.rs.
*/

use cargo_scan::effect::EffectInstance;
use cargo_scan::loc_tracker::LoCTracker;
use cargo_scan::{audit_chain, scanner};

use anyhow::{anyhow, Result};
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

    /// Include transitive effects in dependency crates
    #[arg(short, long, default_value_t = false)]
    transitive: bool,

    /// Path to download crates to for auditing
    #[clap(short = 'd', long = "crate-download-path", default_value = ".stats_tmp")]
    crate_download_path: String,
}

fn main() -> Result<()> {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    let results = if args.transitive {
        let crate_path = args
            .crate_path
            .as_os_str()
            .to_str()
            .ok_or(anyhow!("crate path was not valid UTF-8"))?
            .to_owned();
        let create = audit_chain::Create::new(
            crate_path,
            format!("{}/crate.manifest", &args.crate_download_path),
            args.crate_download_path.clone(),
            false,
            None,
            None,
        );

        let sinks =
            audit_chain::create_dependency_sinks(create, &args.crate_download_path)?;
        scanner::scan_crate_with_sinks(&args.crate_path, sinks)?
    } else {
        scanner::scan_crate(&args.crate_path)?
    };

    println!("{}", EffectInstance::csv_header());
    for effect in results.effects {
        println!("{}", effect.to_csv());
    }

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
