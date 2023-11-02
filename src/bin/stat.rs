use std::fs::create_dir_all;
use std::{fs::remove_dir_all, path::PathBuf};

use anyhow::{anyhow, Context, Result};
use cargo_scan::auditing::info::Config;
use cargo_scan::download_crate;
use cargo_scan::effect::EffectType;
use cargo_scan::{
    audit_chain::{create_new_audit_chain, Create},
    auditing::review::review_audit,
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
    audit_file_path: String,

    /// Download the crate and save it to the crate_path instead of using an
    /// existing crate. If this value is set, requires `download_version` to be
    /// set as well.
    #[clap(short = 'd', long, requires = "download_version")]
    pub download_root_crate: Option<String>,

    /// The crate version to be downloaded. Should be used alongside
    /// `download_root_crate`.
    #[clap(short = 'v', long)]
    pub download_version: Option<String>,

    /// The types of Effects the audit should track. Defaults to all unsafe
    /// behavior.
    #[clap(long, value_parser, num_args = 1.., default_values_t = [
        EffectType::SinkCall,
        EffectType::FFICall,
        EffectType::UnsafeCall,
        EffectType::RawPointer,
        EffectType::UnionField,
        EffectType::StaticMut,
        EffectType::StaticExt,
        EffectType::FnPtrCreation,
        EffectType::ClosureCreation,
    ])]
    effect_types: Vec<EffectType>,
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

// TODO: Figure out who is responsible for clearing the audit path so we don't
//       re-use audited policies.
fn main() -> Result<()> {
    cargo_scan::util::init_logging();
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
        format!("{}/crate.manifest", &args.audit_file_path),
        args.audit_file_path.clone(),
        false,
        None,
        None,
        args.effect_types,
    );

    let mut chain = create_new_audit_chain(create, &args.audit_file_path, false)?;
    let root_crate = chain.root_crate()?;
    let root_audit_file = chain
        .read_audit_file(&root_crate)?
        .ok_or_else(|| anyhow!("Couldn't read root crate from the audit"))?;
    let review_config = Config::new(0, 0, false);
    review_audit(
        &root_audit_file,
        &PathBuf::from(&args.crate_path),
        &review_config,
        false,
    )?;

    remove_dir_all(&args.audit_file_path)?;

    Ok(())
}
