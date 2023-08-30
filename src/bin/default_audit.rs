use std::path::PathBuf;

use cargo_scan::{audit_file::AuditFile, effect::EffectType};

use anyhow::{anyhow, Result};
use clap::Parser;

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
}

fn runner(args: Args) -> Result<()> {
    if args.audit_file_path.is_dir() {
        return Err(anyhow!("Audit path is a directory"));
    }
    if args.audit_file_path.is_file() && !args.overwrite_audit {
        return Err(anyhow!("Audit file already exists"));
    }

    // We can correctly create and save the audit file now
    let audit_file = AuditFile::new_caller_checked_default(
        &args.crate_path,
        &EffectType::unsafe_effects(),
    )?;

    audit_file.save_to_file(args.audit_file_path)?;

    Ok(())
}

fn main() {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    match runner(args) {
        Ok(_) => println!("Created new default audit"),
        Err(e) => println!("Error: {:?}", e),
    };
}
