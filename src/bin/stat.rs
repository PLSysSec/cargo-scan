use std::fs::create_dir_all;
use std::{fs::remove_dir_all, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use cargo_scan::auditing::info::Config;
use cargo_scan::download_crate;
use cargo_scan::{
    audit_chain::{create_new_audit_chain, Create},
    auditing::review::review_policy,
};
use clap::{Parser, ValueEnum};

#[derive(Parser, Debug, Clone)]
struct Args {
    /// Path or name of the crate to audit. If downloading a crate, it will be saved here.
    crate_path: String,

    #[clap(short = 't', long = "crate-path-type", default_value_t = PathType::Local)]
    path_type: PathType,

    /// Path to download crates to for auditing
    #[clap(short = 'd', long, default_value = ".stats_policies_tmp")]
    policy_file_path: String,

    /// Download the crate and save it to the crate_path instead of using an
    /// existing crate. If this value is set, requires `download_version` to be
    /// set as well.
    #[clap(short = 'd', long, requires = "download_version")]
    pub download_root_crate: Option<String>,

    /// The crate version to be downloaded. Should be used alongside
    /// `download_root_crate`.
    #[clap(short = 'v', long)]
    pub download_version: Option<String>,
}

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

// TODO: Figure out who is responsible for clearing the policy path so we don't
//       re-use audited policies.
fn main() -> Result<()> {
    let args = Args::parse();

    if let (Some(crate_name), Some(crate_version)) =
        (&args.download_root_crate, &args.download_version)
    {
        let crate_path = PathBuf::from(args.crate_path.clone());
        if crate_path.exists() {
            return Err(anyhow!(
                "Something already exists at the root crate path: {}",
                args.crate_path
            ));
        }

        create_dir_all(crate_path)?;
        let downloaded_path = download_crate::download_crate_from_info(
            crate_name,
            crate_version,
            &args.crate_path,
        )?;

        // We have now downloaded the crate into a subfolder of the
        // crate_path, so we should move it up where the user expects it
        let mut tmp_path = PathBuf::from(&args.crate_path);
        let bottom_folder = tmp_path
            .file_name()
            .context("No bottom folder in user crate path")?
            .to_os_string();
        let bottom_folder_str = bottom_folder.to_string_lossy();
        tmp_path.pop();
        tmp_path.push(format!("{}-tmp", bottom_folder_str));
        std::fs::rename(&downloaded_path, &tmp_path)?;

        let mut parent_downloaded_path = PathBuf::from(&downloaded_path);
        parent_downloaded_path.pop();
        std::fs::remove_dir(&parent_downloaded_path)?;

        std::fs::rename(&tmp_path, &args.crate_path)?;
    }

    let create = Create::new(
        args.crate_path.clone(),
        format!("{}/crate.manifest", &args.policy_file_path),
        args.policy_file_path.clone(),
        false,
        None,
        None,
    );

    let mut chain = create_new_audit_chain(create, &args.policy_file_path)?;
    let root_crate = chain.root_crate()?;
    let root_policy = chain
        .read_policy(&root_crate)?
        .ok_or_else(|| anyhow!("Couldn't read root crate from the policy"))?;
    let review_config = Config::new(0, 0);
    review_policy(&root_policy, &PathBuf::from(&args.crate_path), &review_config)?;

    remove_dir_all(&args.policy_file_path)?;

    Ok(())
}
