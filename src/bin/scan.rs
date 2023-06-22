/*
    Parse a Rust source file and find all potentially dangerous effects,
    printing them to stdout (one per line).

    Effects are printed in a CSV format -- run --bin csv_header to get
    the header or see effect.rs.
*/

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

    /// Include dependency effects
    #[arg(short, long, default_value_t = false)]
    dependencies: bool,

    /// Path to download crates to for auditing
    #[clap(short = 'd', long = "crate-download-path", default_value = ".stats_tmp")]
    crate_download_path: String,
}

fn main() -> Result<()> {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    let results = if args.dependencies {
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
        );

        let sinks =
            audit_chain::create_dependency_sinks(create, &args.crate_download_path)?;
        scanner::scan_crate_with_sinks(&args.crate_path, sinks)?
    } else {
        scanner::scan_crate(&args.crate_path)?
    };

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
