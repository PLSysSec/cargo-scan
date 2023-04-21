use std::path::PathBuf;

use cargo_scan::policy::PolicyFile;

use anyhow::{anyhow, Result};
use clap::Parser;

/// Interactively vet a package policy
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to crate
    crate_path: PathBuf,
    /// path to the policy file (will create a new one if it doesn't exist)
    policy_path: PathBuf,

    // TODO: Add flags for different default policies
    /// Ovewrite the policy file if a new version of the crate is detected
    #[clap(short = 'o', long = "overwrite-policy", default_value_t = false)]
    overwrite_policy: bool,
}

fn runner(args: Args) -> Result<()> {
    if args.policy_path.is_dir() {
        return Err(anyhow!("Policy path is a directory"));
    }
    if args.policy_path.is_file() && !args.overwrite_policy {
        return Err(anyhow!("Policy file already exists"));
    }

    // We can correctly create and save the policy file now
    let policy = PolicyFile::new_caller_checked_default(&args.crate_path)?;

    policy.save_to_file(args.policy_path)?;

    Ok(())
}

fn main() {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    match runner(args) {
        Ok(_) => println!("Created new default policy"),
        Err(e) => println!("Error: {:?}", e),
    };
}
