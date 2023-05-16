use cargo_scan::auditing::audit::audit_policy;
use cargo_scan::auditing::info::Config;
use cargo_scan::auditing::review::review_policy;
use cargo_scan::auditing::util::{hash_dir, is_policy_scan_valid};
use cargo_scan::effect::EffectBlock;
use cargo_scan::policy::*;
use cargo_scan::scanner;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::Parser;
use inquire::{validator::Validation, Text};

/// Interactively vet a package policy
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// path to crate
    crate_path: PathBuf,
    /// path to the policy file (will create a new one if it doesn't exist)
    policy_path: PathBuf,

    #[clap(flatten)]
    /// Optional config args
    config: Config,

    /// Ovewrite the policy file if a new version of the crate is detected
    #[clap(long = "overwrite-policy", default_value_t = false)]
    overwrite_policy: bool,

    /// Review the policy file without performing an audit
    #[clap(long, short, default_value_t = false)]
    review: bool,

    /// For debugging stuff
    #[clap(long, default_value_t = false)]
    debug: bool,
}

enum ContinueStatus {
    Continue,
    ExitNow,
}

// Asks the user how to handle the invalid policy file. If they continue with a
// new file, will update the policy and policy_path and return Continue;
// otherwise will return ExitNow.
fn handle_invalid_policy(
    policy: &mut PolicyFile,
    policy_path: &mut PathBuf,
    scan_effect_blocks: &HashSet<&EffectBlock>,
    overwrite_policy: bool,
) -> Result<ContinueStatus> {
    // TODO: Colorize
    println!("Crate has changed from last policy audit");

    if overwrite_policy {
        println!("Generating new policy file");

        policy.audit_trees = scan_effect_blocks
            .clone()
            .into_iter()
            .map(|x: &EffectBlock| {
                // TODO: for now we assume that all EffectBlocks include an EffectInstance,
                //       this isn't true, but we have to get caller location into
                //       EffectBocks before we can do this correctly
                let effect_instance = x.effects().first().unwrap();
                (
                    x.clone(),
                    EffectTree::Leaf(
                        EffectInfo::from_instance(effect_instance),
                        SafetyAnnotation::Skipped,
                    ),
                )
            })
            .collect::<HashMap<_, _>>();
        policy.hash = hash_dir(policy.base_dir.clone())?;

        let mut policy_string = policy_path
            .as_path()
            .to_str()
            .ok_or_else(|| anyhow!("Couldn't convert OS Path to str"))?
            .to_string();
        policy_string.push_str(".new");
        println!("New policy file name: {}", &policy_string);
        *policy_path = PathBuf::from(policy_string);

        Ok(ContinueStatus::Continue)
    } else {
        let ans = Text::new(
            r#"Would you like to:
    (c)ontinue with a new policy file, e(x)it tool w/o changes
    "#,
        )
        .with_validator(|x: &str| match x {
            "c" | "x" => Ok(Validation::Valid),
            _ => Ok(Validation::Invalid("Invalid input".into())),
        })
        .prompt()
        .unwrap();

        match ans.as_str() {
            "c" => {
                // TODO: Prompt user for new policy path
                println!("Generating new policy file");

                policy.audit_trees = scan_effect_blocks
                    .clone()
                    .into_iter()
                    .map(|x: &EffectBlock| {
                        // TODO: for now we assume that all EffectBlocks include an EffectInstance,
                        //       this isn't true, but we have to get caller location into
                        //       EffectBocks before we can do this correctly
                        let effect_instance = x.effects().first().unwrap();
                        (
                            x.clone(),
                            EffectTree::Leaf(
                                EffectInfo::from_instance(effect_instance),
                                SafetyAnnotation::Skipped,
                            ),
                        )
                    })
                    .collect::<HashMap<_, _>>();
                policy.hash = hash_dir(policy.base_dir.clone())?;

                let mut policy_string = policy_path
                    .as_path()
                    .to_str()
                    .ok_or_else(|| anyhow!("Couldn't convert OS Path to str"))?
                    .to_string();
                policy_string.push_str(".new");
                println!("New policy file name: {}", &policy_string);
                *policy_path = PathBuf::from(policy_string);

                Ok(ContinueStatus::Continue)
            }
            "x" => {
                println!("Exiting policy tool");
                Ok(ContinueStatus::ExitNow)
            }
            _ => Err(anyhow!("Invalid policy handle selection")),
        }
    }
}

fn audit_crate(args: Args, policy_file: Option<PolicyFile>) -> Result<()> {
    let scan_res = scanner::scan_crate(&args.crate_path)?;
    let scan_effect_blocks = scan_res.unsafe_effect_blocks_set();

    if args.debug {
        println!("{:?}", scan_res);
        return Ok(());
    }

    let mut policy_path = args.policy_path.clone();
    let mut policy_file = match policy_file {
        Some(mut pf) => {
            if !is_policy_scan_valid(&pf, &scan_effect_blocks, args.crate_path.clone())? {
                // TODO: If the policy file diverges from the effects at all, we
                //       should enter incremental mode and detect what's changed
                match handle_invalid_policy(
                    &mut pf,
                    &mut policy_path,
                    &scan_effect_blocks,
                    args.overwrite_policy,
                ) {
                    Ok(ContinueStatus::Continue) => (),
                    Ok(ContinueStatus::ExitNow) => return Ok(()),
                    Err(e) => return Err(e),
                };
            }
            pf
        }
        None => {
            // No policy file yet, so make a new one
            println!("Creating new policy file");
            File::create(policy_path.clone())?;

            // Return an empty PolicyFile, we'll add effects to it later
            let mut pf = PolicyFile::empty(args.crate_path.clone())?;
            pf.set_base_audit_trees(scan_effect_blocks);
            pf
        }
    };

    audit_policy(&mut policy_file, scan_res, &args.config)?;

    policy_file.save_to_file(policy_path)?;

    Ok(())
}

fn runner(args: Args) -> Result<()> {
    let policy_file = PolicyFile::read_policy(args.policy_path.clone())?;

    if args.review {
        match policy_file {
            None => Err(anyhow!("Policy file to review doesn't exist")),
            Some(pf) => review_policy(pf, &args.crate_path, &args.config),
        }
    } else {
        audit_crate(args, policy_file)
    }
}

fn main() {
    cargo_scan::util::init_logging();
    let args = Args::parse();

    match runner(args) {
        Ok(_) => println!("Finished checking policy"),
        Err(e) => println!("Error: {:?}", e),
    };
}
