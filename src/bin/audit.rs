use cargo_scan::audit_file::*;
use cargo_scan::auditing::audit::start_audit;
use cargo_scan::auditing::info::Config;
use cargo_scan::auditing::reset::reset_annotation;
use cargo_scan::auditing::review::review_audit;
use cargo_scan::auditing::util::{hash_dir, is_audit_scan_valid};
use cargo_scan::effect::{EffectInstance, EffectType};
use cargo_scan::scanner::{self, scan_crate};
use cargo_scan::util::load_cargo_toml;

use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use clap::{Parser, ValueEnum};
use home::home_dir;
use inquire::{validator::Validation, Text};
use petgraph::dot::Dot;

/// Interactively vet a package audit
#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Interactively audit a cargo package.",
    long_about = "A tool to help auditing a cargo package. Running the \
                        command on a crate will create a new audit file if one \
                        does not exist, or reuse the existing one. This audit file \
                        keeps track of effects that occur within the crate, and \
                        how the user annotates them. By default, the files are \
                        saved to the $HOME/.cargo_audits directory. \n\n\
                        Note that this tool only tracks effects which originate \
                        within the chosen crate. Effects originating in other \
                        crates must be separately audited."
)]
struct Args {
    /// path to crate
    crate_path: PathBuf,

    #[clap(short, long)]
    /// path to the audit file (will create a new one if it doesn't exist)
    audit_file_path: Option<PathBuf>,

    #[clap(flatten)]
    /// Optional config args
    config: Config,

    /// Ovewrite the audit file if a new version of the crate is detected
    #[clap(long = "overwrite-audit", default_value_t = false)]
    overwrite_audit: bool,

    /// Review the audit file without performing an audit
    #[clap(short, long, default_value_t = false)]
    review: bool,

    /// Which info to review
    #[clap(long, default_value_t = ReviewInfo::PubFuns)]
    review_info: ReviewInfo,

    /// Preview the effects in a package without performing an audit or saving
    /// an audit file
    #[clap(short, long, default_value_t = false)]
    preview: bool,

    /// Reset an annotation to "skipped" for a base effect
    #[clap(long)]
    reset_annotation: bool,

    /// For debugging stuff
    #[clap(long, default_value_t = false)]
    debug: bool,

    /// Ignore the hash of the crate. WARNING: use cautiously - the package files will not be checked
    /// to ensure they are the same when the audit file was created/last audited, but there may be
    /// things like local configuration files that will mess up consistent hashes.
    #[clap(short, long, default_value_t = false)]
    ignore_hash: bool,

    /// Dump the callgraph to the specified file. Uses the DOT format.
    #[clap(long)]
    dump_callgraph: Option<String>,

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

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum ReviewInfo {
    PubFuns,
    All,
}

impl std::fmt::Display for ReviewInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ReviewInfo::All => "all",
            ReviewInfo::PubFuns => "pub-funs",
        };
        write!(f, "{}", s)
    }
}

enum ContinueStatus {
    Continue,
    ExitNow,
}

// Asks the user how to handle the invalid audit file. If they continue with a
// new file, will update the audit and audit_path and return Continue;
// otherwise will return ExitNow.
fn handle_invalid_audit_file<'a, I>(
    audit_file: &mut AuditFile,
    audit_file_path: &mut PathBuf,
    scan_effects: I,
    args: &Args,
) -> Result<ContinueStatus>
where
    I: IntoIterator<Item = &'a EffectInstance>,
{
    // TODO: Colorize
    println!("Crate has changed from last audit");

    if args.overwrite_audit {
        println!("Generating new audit file");

        audit_file.audit_trees = scan_effects
            .into_iter()
            .map(|effect_instance: &EffectInstance| {
                (
                    effect_instance.clone(),
                    EffectTree::Leaf(
                        EffectInfo::from_instance(effect_instance),
                        SafetyAnnotation::Skipped,
                    ),
                )
            })
            .collect::<HashMap<_, _>>();
        audit_file.hash = hash_dir(audit_file.base_dir.clone())?;

        let mut audit_string = audit_file_path
            .as_path()
            .to_str()
            .ok_or_else(|| anyhow!("Couldn't convert OS Path to str"))?
            .to_string();
        audit_string.push_str(".new");
        println!("New audit file name: {}", &audit_string);
        *audit_file_path = PathBuf::from(audit_string);

        Ok(ContinueStatus::Continue)
    } else {
        let ans = Text::new(
            r#"Would you like to:
    (c)ontinue with a new audit file, e(x)it tool w/o changes, (f)orce continue with existing audit [WARNING: crate contents may have changed since last audit]
    "#,
        )
        .with_validator(|x: &str| match x {
            "c" | "x" | "f" => Ok(Validation::Valid),
            _ => Ok(Validation::Invalid("Invalid input".into())),
        })
        .prompt()
        .unwrap();

        match ans.as_str() {
            "c" => {
                // TODO: Prompt user for new audit path
                println!("Generating new audit file");

                audit_file.audit_trees = scan_effects
                    .into_iter()
                    .map(|effect_instance: &EffectInstance| {
                        (
                            effect_instance.clone(),
                            EffectTree::Leaf(
                                EffectInfo::from_instance(effect_instance),
                                SafetyAnnotation::Skipped,
                            ),
                        )
                    })
                    .collect::<HashMap<_, _>>();
                audit_file.hash = hash_dir(audit_file.base_dir.clone())?;

                let mut audit_file_string = audit_file_path
                    .as_path()
                    .to_str()
                    .ok_or_else(|| anyhow!("Couldn't convert OS Path to str"))?
                    .to_string();
                audit_file_string.push_str(".new");
                println!("New audit file name: {}", &audit_file_string);
                *audit_file_path = PathBuf::from(audit_file_string);

                Ok(ContinueStatus::Continue)
            }
            "x" => {
                println!("Exiting audit tool");
                Ok(ContinueStatus::ExitNow)
            }
            "f" => Ok(ContinueStatus::Continue),
            _ => Err(anyhow!("Invalid audit handle selection")),
        }
    }
}

fn audit_crate(args: Args, audit_file: Option<AuditFile>) -> Result<()> {
    let scan_res = {
        let relevant_effects = if let Some(p) = &audit_file {
            &p.scanned_effects
        } else {
            &args.effect_types
        };

        println!("Scanning crate...");
        scanner::scan_crate(&args.crate_path, relevant_effects)?
    };
    let scan_effects = scan_res.effects_set();

    if let Some(callgraph_file) = &args.dump_callgraph {
        let path = Path::new(callgraph_file);
        if !path.exists() {
            let mut file = File::create(callgraph_file)?;
            file.write_all(&format!("{}", Dot::new(&scan_res.call_graph)).into_bytes())?;
        } else {
            println!("Callgraph filepath already exists");
        }
    }

    if args.debug {
        println!("{:?}", scan_res);
        return Ok(());
    }

    let mut audit_file_path = args
        .audit_file_path
        .clone()
        .context("Error: should have created a default audit file path by now")?;
    let mut audit_file = match audit_file {
        Some(mut pf) => {
            if !args.ignore_hash && !is_audit_scan_valid(&pf, args.crate_path.clone())? {
                // TODO: If the audit file diverges from the effects at all, we
                //       should enter incremental mode and detect what's changed
                match handle_invalid_audit_file(
                    &mut pf,
                    &mut audit_file_path,
                    scan_effects,
                    &args,
                ) {
                    Ok(ContinueStatus::Continue) => (),
                    Ok(ContinueStatus::ExitNow) => return Ok(()),
                    Err(e) => return Err(e),
                };
            }
            println!("Loaded audit file");
            pf
        }
        None => {
            // No audit file yet, so make a new one
            println!("Creating new audit file");

            if let Some(parent_dir) = audit_file_path.parent() {
                create_dir_all(parent_dir)?;
            }
            File::create(audit_file_path.clone())?;

            let mut pf = AuditFile::empty(args.crate_path.clone(), args.effect_types)?;
            pf.set_base_audit_trees(scan_effects);
            pf
        }
    };

    if start_audit(&mut audit_file, scan_res, &args.config)?.is_some() {
        // The user marked that they want to audit a child effect, but we aren't
        // able to do so in this mode.
        return Err(anyhow!("Can't audit dependency crate effects in this binary"));
    }

    audit_file.print_audit_stats();

    println!();
    println!("Saving audit to file");
    audit_file.save_to_file(audit_file_path)?;

    Ok(())
}

fn runner(args: Args) -> Result<()> {
    let audit_file_path = args
        .audit_file_path
        .clone()
        .context("Error: should have created a default audit file path already")?;
    let audit_file = AuditFile::read_audit_file(audit_file_path.clone())?;

    if args.preview {
        println!("Previewing crate effects.");
        println!("Scanning crate...");

        let res = scan_crate(&args.crate_path, &args.effect_types)?;
        for effect in res.effects {
            println!("{}", effect.to_csv());
        }
        Ok(())
    } else if args.reset_annotation {
        match audit_file {
            None => Err(anyhow!("Audit file doesn't exist")),
            Some(pf) => reset_annotation(pf, audit_file_path),
        }
    } else if args.review {
        match audit_file {
            None => Err(anyhow!("Audit file to review doesn't exist")),
            Some(af) => {
                match args.review_info {
                    ReviewInfo::All => review_audit(&af, &args.crate_path, &args.config),
                    ReviewInfo::PubFuns => {
                        println!("Public functions marked caller-checked:");
                        for pub_fn in af.pub_caller_checked.keys() {
                            // TODO: Print more info
                            println!("  {}", pub_fn);
                        }
                        Ok(())
                    }
                }
            }
        }
    } else {
        audit_crate(args, audit_file)
    }
}

fn main() {
    cargo_scan::util::init_logging();
    let mut args = Args::parse();
    if args.audit_file_path.is_none() {
        if let Some(mut p) = home_dir() {
            p.push(".cargo_audits");
            if let Ok(crate_id) = load_cargo_toml(&args.crate_path) {
                p.push(format!("{}.audit", crate_id));
            } else {
                println!("Error: Couldn't load the Cargo.toml at the crate path");
                return;
            }
            args.audit_file_path = Some(p);
        }
    } else {
        println!("Error: couldn't find the home directory (required for default audit file path)");
        return;
    }

    match runner(args) {
        Ok(_) => (),
        Err(e) => println!("Error: {:?}", e),
    };
}
