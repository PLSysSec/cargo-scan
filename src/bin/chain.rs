use cargo_scan::audit_chain::{create_new_audit_chain, AuditChain, Create};
use cargo_scan::auditing::audit::audit_policy;
use cargo_scan::auditing::info::Config as AuditConfig;
use cargo_scan::auditing::review::review_policy;
use cargo_scan::policy::PolicyFile;
use cargo_scan::scanner;

use anyhow::{anyhow, Result};
use clap::{Args as ClapArgs, Parser, Subcommand, ValueEnum};
use std::collections::HashSet;
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

#[derive(ValueEnum, Clone, Copy, Debug)]
enum ReviewInfo {
    PubFuns,
    All,
}

impl std::fmt::Display for ReviewInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ReviewInfo::PubFuns => "pub-funs",
            ReviewInfo::All => "all",
        };
        write!(f, "{}", s)
    }
}

impl CommandRunner for Review {
    fn run_command(self, args: OuterArgs) -> Result<()> {
        let chain = match AuditChain::read_audit_chain(PathBuf::from(&self.manifest_path))
        {
            Ok(Some(chain)) => Ok(chain),
            Ok(None) => Err(anyhow!(
                "Couldn't find audit chain manifest at {}",
                &self.manifest_path
            )),
            Err(e) => Err(e),
        }?;

        let crates_to_review = match self.review_target {
            None => chain.all_crates(),
            Some(crate_name) => chain.matching_crates_no_version(&crate_name),
        };

        for review_crate in crates_to_review {
            println!("Reviewing policy for {}", review_crate);
            let policy = chain.read_policy(review_crate).ok_or_else(|| {
                anyhow!(format!(
                    "Couldn't find policy for crate {} in chain",
                    review_crate
                ))
            })?;
            let mut crate_path = PathBuf::from(&args.crate_download_path);
            crate_path.push(review_crate);
            review_crate_policy(&policy, crate_path, self.review_info)?;
        }
        Ok(())
    }
}

#[derive(Clone, ClapArgs, Debug)]
struct Audit {
    /// Path to manifest
    manifest_path: String,
    /// Crate to review
    crate_name: String,
}

impl CommandRunner for Audit {
    fn run_command(self, args: OuterArgs) -> Result<()> {
        println!("Auditing crate: {}", self.crate_name);
        match AuditChain::read_audit_chain(PathBuf::from(&self.manifest_path)) {
            Ok(Some(mut chain)) => {
                let mut policies = chain.read_policy_no_version(&self.crate_name)?;
                if policies.is_empty() {
                    println!("No policies matching the crate {}", &self.manifest_path);
                    Ok(())
                } else if policies.len() > 1 {
                    // TODO: Allow for auditing more than one policy matching a crate
                    println!("More than one policy for crate {}", &self.manifest_path);
                    Ok(())
                } else {
                    let (full_crate_name, orig_policy) = policies.pop().unwrap();
                    let mut new_policy = orig_policy.clone();
                    let mut crate_path = PathBuf::from(&args.crate_download_path);
                    crate_path.push(&full_crate_name);
                    let scan_res = scanner::scan_crate(&crate_path)?;
                    let audit_config = AuditConfig::default();
                    audit_policy(&mut new_policy, scan_res, &audit_config)?;

                    // if any public function annotations have changed,
                    // update parent packages
                    let removed_fns = orig_policy
                        .pub_caller_checked
                        .difference(&new_policy.pub_caller_checked)
                        .cloned()
                        .collect::<HashSet<_>>();

                    chain.remove_cross_crate_effects(removed_fns, &full_crate_name)?;

                    Ok(())
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
        // TODO: Plug in to existing policy review
        ReviewInfo::All => review_policy(policy, &crate_path, &AuditConfig::default()),
        ReviewInfo::PubFuns => {
            println!("Public functions marked caller-checked:");
            for pub_fn in policy.pub_caller_checked.iter() {
                // TODO: Print more info
                println!("  {}", pub_fn);
            }
            Ok(())
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
