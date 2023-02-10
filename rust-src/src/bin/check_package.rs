use cargo_scan::effect::Effect;
use cargo_scan::scanner;

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
//use colored::Colorize;
use inquire::{validator::Validation, Text};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

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
#[serde_as]
#[derive(Serialize, Deserialize)]
struct PolicyFile {
    // TODO: Serde doesn't like this hashmap for some reason (?)
    #[serde_as(as = "Vec<(_, _)>")]
    effects: HashMap<Effect, SafetyAnnotation>,
    // TODO: The base_dir should be the crate name or something
    base_dir: PathBuf,
    hash: u128,
}

impl PolicyFile {
    fn new(p: PathBuf) -> Self {
        // TODO: hash the file
        PolicyFile { effects: HashMap::new(), base_dir: p, hash: 0 }
    }

    fn save_to_file(&self, p: PathBuf) -> Result<()> {
        let json = serde_json::to_string(self)?;
        let mut f = File::create(p)?;
        f.write_all(json.as_bytes())?;
        Ok(())
    }
}

fn get_policy_file(
    policy_filepath: PathBuf,
    crate_filepath: PathBuf,
) -> Result<PolicyFile> {
    if policy_filepath.is_dir() {
        return Err(anyhow!("Policy file filepath is a directory"));
    } else if !policy_filepath.is_file() {
        File::create(policy_filepath)?;

        // Return an empty PolicyFile, we'll add effects to it later
        return Ok(PolicyFile::new(crate_filepath));
    }

    // We found a policy file
    // TODO: Check the hash to see if we've updated versions? (Might have
    //       to happen later)
    // TODO: make this display a message if the file isn't the proper format
    let json = std::fs::read_to_string(policy_filepath)?;
    let policy_file = serde_json::from_str(&json)?;
    Ok(policy_file)
}

fn get_effects(p: &Path) -> Result<Vec<Effect>> {
    let scanner_res = scanner::load_and_scan(p);
    // TODO: There's a lot of stuff in the scan right now that isn't included
    //       in the effects. We should make sure we're reporting everything we
    //       care about.
    Ok(scanner_res.effects)
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

// Returns the SafetyAnnotation if the user selects one, or returns
// an error indicating if we should exit early
fn get_user_annotation() -> Result<SafetyAnnotation, bool> {
    let ans = Text::new(
        r#"Select how to mark this effect:
  (s)afe, (u)nsafe, (c)aller checked, ask me (l)ater
"#,
    )
    .with_validator(|x: &str| match x {
        "s" | "u" | "c" | "l" | "x" => Ok(Validation::Valid),
        _ => Ok(Validation::Invalid("Invalid input".into())),
    })
    .prompt()
    .unwrap();

    match ans.as_str() {
        "s" => Ok(SafetyAnnotation::Safe),
        "u" => Ok(SafetyAnnotation::Unsafe),
        "c" => Ok(SafetyAnnotation::CallerChecked),
        "l" => Ok(SafetyAnnotation::Skipped),
        "x" => Err(true),
        _ => Err(false),
    }
}

fn main() {
    let args = Args::parse();

    // TODO: If the hash of the policy file is different (or on different version
    //       of crate), invalidate the policy file and start a new one.
    let mut policy_file =
        match get_policy_file(args.policy_path.clone(), args.crate_path.clone()) {
            Ok(c) => c,
            Err(e) => {
                println!("err: {:?}", e);
                return;
            }
        };

    let effects = get_effects(&args.crate_path).unwrap();

    let mut continue_vet = true;
    // Iterate through the effects and prompt the user for if they're safe
    for e in effects {
        let a = policy_file.effects.entry(e.clone()).or_insert(SafetyAnnotation::Skipped);
        if continue_vet && *a == SafetyAnnotation::Skipped {
            // only present effects we haven't audited yet
            if print_effect_info(&e, &args.config).is_err() {
                println!("Error printing effect information. Trying to continue...");
            }
            let status = match get_user_annotation() {
                Ok(s) => s,
                Err(false) => {
                    println!("Error accepting user input. Attempting to continue...");
                    SafetyAnnotation::Skipped
                }
                Err(true) => {
                    continue_vet = false;
                    SafetyAnnotation::Skipped
                }
            };
            *a = status;
        }
    }

    // save the new policy file
    if policy_file.save_to_file(args.policy_path).is_err() {
        println!("Error saving policy file.");
    }
}
