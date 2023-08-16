use cargo_scan::audit_chain::{create_new_audit_chain, AuditChain, Create};
use cargo_scan::auditing::audit::{audit_policy, audit_pub_fn};
use cargo_scan::auditing::info::Config as AuditConfig;
use cargo_scan::auditing::review::review_policy;
use cargo_scan::effect::Effect;
use cargo_scan::policy::PolicyFile;
use cargo_scan::{download_crate, scanner};

use anyhow::{anyhow, Context, Result};
use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use std::collections::HashSet;
use std::fs::create_dir_all;
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
struct OuterArgs {
    // TODO: Can probably use the default rust build location
    /// Path to download crates to for auditing
    #[clap(short = 'd', long = "crate-download-path", default_value = ".audit_crates")]
    crate_download_path: String,
}

#[derive(Parser, Debug)]
struct Args {
    #[clap(flatten)]
    outer_args: OuterArgs,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Create(Create),
    Review(Review),
    Audit(Audit),
}

trait CommandRunner {
    fn run_command(self, args: OuterArgs) -> Result<()>;
}

impl CommandRunner for Command {
    fn run_command(self, args: OuterArgs) -> Result<()> {
        match self {
            Self::Create(create) => create.run_command(args),
            Self::Review(review) => review.run_command(args),
            Self::Audit(audit) => audit.run_command(args),
        }
    }
}

impl CommandRunner for Create {
    fn run_command(self, args: OuterArgs) -> Result<()> {
        if let (Some(crate_name), Some(crate_version)) =
            (&self.download_root_crate, &self.download_version)
        {
            let crate_path = PathBuf::from(self.crate_path.clone());
            if crate_path.exists() {
                return Err(anyhow!(
                    "Something already exists at the root crate path: {}",
                    self.crate_path
                ));
            }

            create_dir_all(crate_path)?;
            let downloaded_path = download_crate::download_crate_from_info(
                crate_name,
                crate_version,
                &self.crate_path,
            )?;

            // We have now downloaded the crate into a subfolder of the
            // crate_path, so we should move it up where the user expects it
            let mut tmp_path = PathBuf::from(&self.crate_path);
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

            std::fs::rename(&tmp_path, &self.crate_path)?;
        }

        let chain = create_new_audit_chain(self, &args.crate_download_path)?;
        chain.save_to_file()?;
        Ok(())
    }
}

#[derive(Clone, ClapArgs, Debug)]
struct Review {
    /// Path to chain manifest
    manifest_path: String,
    /// What information to display
    #[clap(short = 'i', long, default_value_t = ReviewInfo::PubFuns)]
    review_info: ReviewInfo,
    /// What crate to review, defaults to all crates.
    review_target: Option<String>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum ReviewInfo {
    Crates,
    PubFuns,
    All,
}

impl std::fmt::Display for ReviewInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ReviewInfo::Crates => "crates",
            ReviewInfo::PubFuns => "pub-funs",
            ReviewInfo::All => "all",
        };
        write!(f, "{}", s)
    }
}

impl CommandRunner for Review {
    fn run_command(self, args: OuterArgs) -> Result<()> {
        let mut chain =
            match AuditChain::read_audit_chain(PathBuf::from(&self.manifest_path)) {
                Ok(Some(chain)) => Ok(chain),
                Ok(None) => Err(anyhow!(
                    "Couldn't find audit chain manifest at {}",
                    &self.manifest_path
                )),
                Err(e) => Err(e),
            }?;

        // Don't have to do the usual review process of loading up the crate's
        // policy if we are just printing out the list of crates for the given
        // manifest file
        if self.review_info == ReviewInfo::Crates {
            println!("Dependency crates:");
            for krate in chain.all_crates() {
                println!("  - {}", krate);
            }

            return Ok(());
        }

        let crates_to_review = match self.review_target {
            None => chain.all_crates().into_iter().cloned().collect::<Vec<_>>(),
            Some(crate_name) => chain.matching_crates_no_version(&crate_name),
        };

        for review_crate in crates_to_review {
            println!("Reviewing policy for {}", review_crate);
            let policy = chain.read_policy(&review_crate)?.ok_or_else(|| {
                anyhow!(format!(
                    "Couldn't find policy for crate {} in chain",
                    review_crate
                ))
            })?;
            let mut crate_path = PathBuf::from(&args.crate_download_path);
            crate_path.push(format!("{}", review_crate));
            review_crate_policy(&policy, crate_path, self.review_info)?;
        }
        Ok(())
    }
}

// TODO: Default to top-level package
// TODO: Add option to audit a public function from a particular package
// TODO: Add info if the user doesn't have anything left to audit
// TODO: Add effect reset command
#[derive(Clone, ClapArgs, Debug)]
struct Audit {
    /// Path to manifest
    manifest_path: String,
    /// Name of the crate to review (defaults to the root crate if none is provided)
    crate_name: Option<String>,
}

// TODO: print more info during auding (e.g. saving files)
impl CommandRunner for Audit {
    fn run_command(self, _args: OuterArgs) -> Result<()> {
        match AuditChain::read_audit_chain(PathBuf::from(&self.manifest_path)) {
            Ok(Some(mut chain)) => {
                let crate_id = match self.crate_name {
                    Some(crate_name) => chain.resolve_crate_id(&crate_name).context(
                        format!("Couldn't resolve crate_name for {}", &crate_name),
                    )?,
                    None => chain.root_crate()?,
                };

                // TODO: Handle more than one policy matching a crate
                if let Some(orig_policy) = chain.read_policy(&crate_id)? {
                    let mut new_policy = orig_policy.clone();
                    let crate_path = PathBuf::from(&orig_policy.base_dir);

                    // Iterate through the crate's dependencies and add the
                    // public functions to the scan sinks
                    let scan_res = scanner::scan_crate(&crate_path)?;

                    let audit_config = AuditConfig::default();

                    // TODO: Mechanism for re-auditing the default policies
                    // NOTE: audit_res will contain an EffectBlock if the user
                    //       needs to audit a child package's effects
                    let audit_res =
                        audit_policy(&mut new_policy, scan_res, &audit_config);
                    // Save the policy immediately after audit so we don't error
                    // out and forget to save
                    chain.save_policy(&crate_id, &new_policy)?;
                    let removed_fns = if let Some(dep_effect) = audit_res? {
                        // TODO: Print parents of an effect the user audits when
                        //       auditing children
                        match dep_effect.eff_type() {
                            Effect::SinkCall(sink_ident) => {
                                audit_pub_fn(&mut chain, sink_ident)?
                            }
                            _ => {
                                return Err(anyhow!(
                                    "Can only audit dependency effects for sinks"
                                ))
                            }
                        }
                    } else {
                        HashSet::new()
                    };

                    // if any public function annotations have changed,
                    // update parent packages
                    if !removed_fns.is_empty() {
                        chain.remove_cross_crate_effects(removed_fns, &crate_id)?;
                    }

                    Ok(())
                } else {
                    Err(anyhow!("We require exactly one policy matching the crate name"))
                }
            }
            Ok(None) => Err(anyhow!(
                "Couldn't find audit chain manifest at {}",
                &self.manifest_path
            )),
            Err(e) => Err(e),
        }
    }
}

fn review_crate_policy(
    policy: &PolicyFile,
    crate_path: PathBuf,
    review_type: ReviewInfo,
) -> Result<()> {
    match review_type {
        ReviewInfo::All => review_policy(policy, &crate_path, &AuditConfig::default()),
        ReviewInfo::PubFuns => {
            println!("Public functions marked caller-checked:");
            for pub_fn in policy.pub_caller_checked.keys() {
                // TODO: Print more info
                println!("  {}", pub_fn);
            }
            Ok(())
        }

        ReviewInfo::Crates => {
            Err(anyhow!("Shouldn't review a crate policy when printing crates"))
        }
    }
}

fn main() {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    match args.command.run_command(args.outer_args) {
        Ok(()) => (),
        Err(e) => println!("Error running command: {}", e),
    }
}
