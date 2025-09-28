/*
    This binary is intended for internal use.

    The main supported binaries are `--bin scan` and `--bin audit`.
    See README.md for usage instructions.
*/

use std::{collections::HashSet, path::PathBuf};

use cargo_scan::{audit_file::AuditFile, effect::EffectType};

use anyhow::{anyhow, Result};
use clap::Parser;
use parse_display::{Display, FromStr};

/// Interactively vet a package audit
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to crate
    crate_path: PathBuf,
    /// path to the audit file (will create a new one if it doesn't exist)
    audit_file_path: PathBuf,

    // TODO: Add flags for different default policies
    /// Ovewrite the audit file if a new version of the crate is detected
    #[clap(short = 'o', long = "overwrite-audit", default_value_t = false)]
    overwrite_audit: bool,

    /// Default audit type
    #[clap(short, long, default_value_t = AuditType::CallerChecked)]
    audit_type: AuditType,

    /// Run in quick mode (turns off RustAnalyzer)
    #[clap(long, default_value_t = false)]
    quick_mode: bool,

    /// Whether to analyze macro expansions for effects
    #[clap(long, default_value_t = true)]
    expand_macro: bool,
}

// TODO: Combine this with DefaultAuditType once we implement every version
#[derive(Debug, Clone, Copy, PartialEq, Display, FromStr)]
enum AuditType {
    CallerChecked,
    Safe,
}

fn runner(args: Args) -> Result<()> {
    if args.audit_file_path.is_dir() {
        return Err(anyhow!("Audit path is a directory"));
    }
    if args.audit_file_path.is_file() && !args.overwrite_audit {
        return Err(anyhow!("Audit file already exists"));
    }

    let audit_file = match args.audit_type {
        AuditType::CallerChecked => AuditFile::new_caller_checked_default(
            &args.crate_path,
            &EffectType::unsafe_effects(),
            args.quick_mode,
            args.expand_macro,
        )?,
        AuditType::Safe => AuditFile::new_safe_default_with_sinks(
            &args.crate_path,
            HashSet::new(),
            &EffectType::unsafe_effects(),
            args.quick_mode,
            args.expand_macro,
        )?,
    };

    // We can correctly create and save the audit file now

    audit_file.save_to_file(args.audit_file_path)?;

    Ok(())
}

fn main() {
    eprintln!("Warning: `--bin default_audit` is intended for internal use. The primary supported binaries are `--bin scan` and `--bin audit`.");

    cargo_scan::util::init_logging();
    let args = Args::parse();

    match runner(args) {
        Ok(_) => println!("Created new default audit"),
        Err(e) => println!("Error: {:?}", e),
    };
}
