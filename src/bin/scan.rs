/*
    Parse a Rust source file and find all potentially dangerous effects,
    printing them to stdout (one per line).

    Effects are printed in a CSV format -- run --bin csv_header to get
    the header or see effect.rs.
*/

use cargo_scan::audit_file::AuditFile;
use cargo_scan::effect::{EffectInstance, EffectType, DEFAULT_EFFECT_TYPES};

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to crate directory; should contain a 'src' directory and a Cargo.toml file
    crate_path: PathBuf,

    /// Verbose output:
    /// In addition to effects, print metadata about total LoC scanned and ignored
    #[arg(short, long, default_value_t = false)]
    extras: bool,

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

    if args.extras {
        println!();
        println!(
            "\
            total, loc_lb, loc_ub, \
            macros, loc_lb, loc_ub, \
            conditional_code, loc_lb, loc_ub, \
            skipped_calls, loc_lb, loc_ub, \
            skipped_fn_ptrs, loc_lb, loc_ub, \
            skipped_other, loc_lb, loc_ub, \
            unsafe_trait, loc_lb, loc_ub, \
            unsafe_impl, loc_lb, loc_ub\
            "
        );
        println!(
            "{}, {}, {}, {}, {}, {}, {}, {}",
            results.total_loc.as_csv(),
            results.skipped_macros.as_csv(),
            results.skipped_conditional_code.as_csv(),
            results.skipped_fn_calls.as_csv(),
            results.skipped_fn_ptrs.as_csv(),
            results.skipped_other.as_csv(),
            results.unsafe_traits.as_csv(),
            results.unsafe_impls.as_csv(),
        )

        // println!("Total scanned, {}", results.total_loc.as_csv());
        // println!("Skipped macros, {}", results.skipped_macros.as_csv());
        // println!("Skipped cond. code, {}", results.skipped_conditional_code.as_csv());
        // println!("Skipped function calls, {}", results.skipped_fn_calls.as_csv());
        // println!("Skipped function pointers, {}", results.skipped_fn_ptrs.as_csv());
        // println!("Skipped other, {}", results.skipped_other.as_csv());
        // println!("Unsafe trait keywords, {}", results.unsafe_traits.as_csv());
        // println!("Unsafe trait impl keywords, {}", results.unsafe_impls.as_csv());
    }

    Ok(())
}
