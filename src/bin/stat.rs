use std::{fs::remove_dir_all, path::PathBuf};

use anyhow::{anyhow, Result};
use cargo_scan::auditing::info::Config;
use cargo_scan::{
    audit_chain::{create_new_audit_chain, Create},
    auditing::review::review_policy,
};
use clap::{Parser, ValueEnum};

#[derive(Parser, Debug, Clone)]
struct Args {
    /// Path or name of the crate to audit
    crate_path: String,

    #[clap(short = 't', long = "crate-path-type", default_value_t = PathType::Local)]
    path_type: PathType,

    /// Path to download crates to for auditing
    #[clap(short = 'd', long = "crate-download-path", default_value = ".stats_tmp")]
    crate_download_path: String,
}

// TODO: Add non crates.io remote
#[derive(ValueEnum, Clone, Copy, Debug)]
enum PathType {
    Local,
    Remote,
}

impl std::fmt::Display for PathType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PathType::Local => "local",
            PathType::Remote => "remote",
        };
        write!(f, "{}", s)
    }
}

fn main() -> Result<()> {
    let args = Args::parse();

    // TODO: Download path if applicable

    let create = Create::new(
        args.crate_path.clone(),
        format!("{}/crate.manifest", &args.crate_download_path),
        args.crate_download_path.clone(),
        false,
    );

    let chain = create_new_audit_chain(create, &args.crate_download_path)?;
    let root_crate = chain.root_crate()?;
    let root_policy = chain
        .read_policy(&root_crate)
        .ok_or_else(|| anyhow!("Couldn't read root crate from the policy"))?;
    let review_config = Config::new(0, 0);
    review_policy(&root_policy, &PathBuf::from(&args.crate_path), &review_config)?;

    remove_dir_all(&args.crate_download_path)?;

    Ok(())
}
