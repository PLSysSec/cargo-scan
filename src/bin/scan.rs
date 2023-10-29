//! Run a scan for a single crate.
//!
//! Prints out potentially dangerous effects to stdout (one per line),
//! in CSV format,
//! followed by various metadata.

use cargo_scan::effect::EffectInstance;
use cargo_scan::scan_stats::{self, CrateStats};

use anyhow::Result;
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
}

fn main() -> Result<()> {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    let stats = scan_stats::get_crate_stats_default(args.crate_path)?;

    println!("{}", EffectInstance::csv_header());
    for effect in &stats.effects {
        println!("{}", effect.to_csv());
    }

    println!();
    println!("{}", CrateStats::metadata_csv_header());
    println!("{}", stats.metadata_csv());

    Ok(())
}
