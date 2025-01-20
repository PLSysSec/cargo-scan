//! The scan binary: Scan a crate and print side effects to stdout in CSV format.
//!
//! Prints out potentially dangerous effects to stdout (one per line),
//! as well as metadata.
//!
//! See README for current usage information.

use cargo_scan::effect::EffectInstance;
use cargo_scan::scan_stats::{self, CrateStats};

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to crate directory; should contain a 'src' directory and a Cargo.toml file
    crate_path: PathBuf,

    // Turned off for now -- chain binary not being used
    // /// Include transitive effects in dependency crates
    // #[arg(short, long, default_value_t = false)]
    // transitive: bool,
    /// Path to download crates to for auditing
    #[clap(short = 'd', long = "crate-download-path", default_value = ".stats_tmp")]
    crate_download_path: String,

    /// Run in quick mode (turns off RustAnalyzer)
    #[clap(short, long, default_value_t = false)]
    quick_mode: bool,

    /// Suppress "total" lines at the bottom of the output
    #[clap(short, long, default_value_t = false)]
    suppress_total: bool,

    /// Expand macros and scan expanded code
    #[clap(short, long, default_value_t = false)]
    expand_macro: bool,
}

fn main() {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    // Note: old version without default_audit:
    // scanner::scan_crate(&args.crate_path, &args.effect_types)?
    let stats = scan_stats::get_crate_stats_default(
        args.crate_path,
        args.quick_mode,
        args.expand_macro,
    );

    println!("{}", EffectInstance::csv_header());
    for effect in &stats.effects {
        println!("{}", effect.to_csv());
    }

    if !args.suppress_total {
        println!();
        println!("{}", CrateStats::metadata_csv_header());
        println!("{}", stats.metadata_csv());
    }
}
