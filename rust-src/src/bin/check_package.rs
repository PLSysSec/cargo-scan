use cargo_scan::effect::Effect;
use cargo_scan::scanner;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use inquire::{validator::Validation, Text};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
struct Config {
    #[clap(long = "lines-before", default_value_t = 4)]
    /// The number of lines before an effect to show
    lines_before_effect: u8,

    #[clap(long = "lines-after", default_value_t = 1)]
    /// The number of lines after an effect to show
    lines_after_effect: u8,
}

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
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
enum SafetyAnnotation {
    Skipped,
    Safe,
    Unsafe,
    CallerChecked,
}

// TODO: Include information about crate/version
// TODO: We should include more information from the ScanResult
#[serde_as]
#[derive(Serialize, Deserialize)]
struct PolicyFile {
    // TODO: Serde doesn't like this hashmap for some reason (?)
    #[serde_as(as = "Vec<(_, _)>")]
    effects: HashMap<Effect, SafetyAnnotation>,
    base_dir: PathBuf,
    hash: [u8; 32],
}

fn hash_dir(p: PathBuf) -> Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    for entry in WalkDir::new(p) {
        let entry = entry?;
        let mut file = File::open(entry.path())?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        hasher.update(buf);
    }

    Ok(hasher.finalize().into())
}

impl PolicyFile {
    fn new(p: PathBuf) -> Result<Self> {
        let hash = hash_dir(p.clone())?;
        Ok(PolicyFile { effects: HashMap::new(), base_dir: p, hash })
    }

    fn save_to_file(&self, p: PathBuf) -> Result<()> {
        let json = serde_json::to_string(self)?;
        let mut f = File::create(p)?;
        f.write_all(json.as_bytes())?;
        Ok(())
    }
}

// Returns Some policy file if it exists, or None if we should create a new one.
// Errors if the policy filepath is invalid or if we can't read an existing
// policy file
fn get_policy_file(policy_filepath: PathBuf) -> Result<Option<PolicyFile>> {
    if policy_filepath.is_dir() {
        return Err(anyhow!("Policy file filepath is a directory"));
    } else if !policy_filepath.is_file() {
        return Ok(None);
    }

    // We found a policy file
    // TODO: Check the hash to see if we've updated versions? (Might have
    //       to happen later)
    // TODO: make this display a message if the file isn't the proper format
    let json = std::fs::read_to_string(policy_filepath)?;
    let policy_file = serde_json::from_str(&json)?;
    Ok(Some(policy_file))
}

fn print_effect_src(effect: &Effect, config: &Config) -> Result<()> {
    let mut full_path = effect.call_loc().dir().clone();
    full_path.push(effect.call_loc().file());

    let src_contents = std::fs::read_to_string(full_path)?;

    // Get the byte ranges for each line of the src file
    let src_lines = src_contents.split('\n');
    let mut src_linenum_ranges = HashMap::new();
    src_lines.fold((0, 0), |(lineno, byte_count), line| {
        src_linenum_ranges.insert(lineno, (byte_count, byte_count + line.len() + 1));
        (lineno + 1, byte_count + line.len() + 1)
    });

    // calculate the byte ranges for the effect
    let effect_line = effect.call_loc().line();
    let bounded_start_line =
        std::cmp::max(effect_line - config.lines_before_effect as usize, 0);
    let bounded_end_line = std::cmp::min(
        effect_line - config.lines_after_effect as usize,
        src_linenum_ranges.len(),
    );

    let surrounding_start = src_linenum_ranges.get(&bounded_start_line).unwrap().0;
    let surrounding_end = src_linenum_ranges.get(&bounded_end_line).unwrap().1;
    let effect_start = src_linenum_ranges.get(&effect_line).unwrap().0;
    let effect_end = src_linenum_ranges.get(&effect_line).unwrap().1;

    // TODO: cache files?
    let mut files = SimpleFiles::new();
    let file_id =
        files.add(format!("{}", effect.call_loc().file().display()), src_contents);

    // construct the codespan diagnostic
    // TODO: make this a better effect message
    // TODO: Don't display "Error" at the start of the message
    let diag = Diagnostic::error()
        .with_message(format!("effect: {:?}", effect.pattern().as_ref()))
        .with_labels(vec![
            Label::primary(file_id, effect_start..effect_end),
            Label::secondary(file_id, surrounding_start..surrounding_end),
        ]);

    let writer = StandardStream::stderr(ColorChoice::Always);
    let config = codespan_reporting::term::Config::default();

    // Print the information to the user
    term::emit(&mut writer.lock(), &config, &files, &diag)?;

    Ok(())
}

fn print_call_stack(_effect: &Effect) -> Result<()> {
    // TODO: Get the call stack from the effect (might need more info)
    Ok(())
}

fn print_effect_info(effect: &Effect, config: &Config) -> Result<()> {
    print_call_stack(effect)?;
    print_effect_src(effect, config)?;
    Ok(())
}

// Returns Some SafetyAnnotation if the user selects one, None if the user
// chooses to exit early, or an Error
fn get_user_annotation() -> Result<Option<SafetyAnnotation>> {
    let ans = Text::new(
        r#"Select how to mark this effect:
  (s)afe, (u)nsafe, (c)aller checked, ask me (l)ater, e(x)it tool
"#,
    )
    .with_validator(|x: &str| match x {
        "s" | "u" | "c" | "l" | "x" => Ok(Validation::Valid),
        _ => Ok(Validation::Invalid("Invalid input".into())),
    })
    .prompt()
    .unwrap();

    match ans.as_str() {
        "s" => Ok(Some(SafetyAnnotation::Safe)),
        "u" => Ok(Some(SafetyAnnotation::Unsafe)),
        "c" => Ok(Some(SafetyAnnotation::CallerChecked)),
        "l" => Ok(Some(SafetyAnnotation::Skipped)),
        "x" => Ok(None),
        _ => Err(anyhow!("Invalid annotation selection")),
    }
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
    scan_effects: &HashSet<&Effect>,
) -> Result<ContinueStatus> {
    println!("Crate has changed from last policy audit");

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

            policy.effects = scan_effects
                .clone()
                .into_iter()
                .map(|x| (x.clone(), SafetyAnnotation::Skipped))
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

fn runner(args: Args) -> Result<()> {
    let mut policy_path = args.policy_path;
    let policy_file = get_policy_file(policy_path.clone())?;

    // TODO: Might want to fold the ScanResults into the policy file/policy
    //       file creation

    // TODO: There's a lot of stuff in the scan right now that isn't included
    //       in the effects. We should make sure we're reporting everything we
    //       care about.
    let scan_res = scanner::scan_crate(&args.crate_path)?;
    let scan_effects =
        scan_res.effects.iter().filter(|x| x.pattern().is_some()).collect::<HashSet<_>>();

    let mut policy_file = match policy_file {
        Some(mut pf) => {
            // Check if we should invalidate the current policy file
            let policy_effects = pf.effects.keys().collect::<HashSet<_>>();
            let hash = hash_dir(args.crate_path.clone())?;
            if policy_effects != scan_effects || hash != pf.hash {
                // TODO: If the policy file diverges from the effects at all, we
                //       should enter incremental mode and detect what's changed
                match handle_invalid_policy(&mut pf, &mut policy_path, &scan_effects) {
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
            let mut pf = PolicyFile::new(args.crate_path.clone())?;
            pf.effects = scan_effects
                .clone()
                .into_iter()
                .map(|x| (x.clone(), SafetyAnnotation::Skipped))
                .collect::<HashMap<_, _>>();
            pf
        }
    };

    let mut continue_vet = true;
    // Iterate through the effects and prompt the user for if they're safe
    for e in scan_effects {
        let a = policy_file.effects.get_mut(e).unwrap();
        if continue_vet && *a == SafetyAnnotation::Skipped {
            // only present effects we haven't audited yet
            if print_effect_info(e, &args.config).is_err() {
                println!("Error printing effect information. Trying to continue...");
            }

            // TODO: If the user annotates with caller-checked, we should add
            //       each callsite as a new effect location to audit
            let status = match get_user_annotation() {
                Ok(Some(s)) => s,
                Ok(None) => {
                    continue_vet = false;
                    SafetyAnnotation::Skipped
                }
                Err(_) => {
                    println!("Error accepting user input. Attempting to continue...");
                    SafetyAnnotation::Skipped
                }
            };
            *a = status;
        }
    }

    policy_file.save_to_file(policy_path)?;

    Ok(())
}

fn main() {
    let args = Args::parse();

    match runner(args) {
        Ok(_) => println!("Finished checking policy"),
        Err(e) => println!("Error: {:?}", e),
    };
}
