use cargo_scan::effect::{Effect, EffectBlock, SrcLoc};
use cargo_scan::ident::{CanonicalPath, Path};
use cargo_scan::policy::*;
use cargo_scan::scanner;
use cargo_scan::scanner::ScanResults;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::Parser;
use codespan_reporting::diagnostic::{Diagnostic, Label};
use codespan_reporting::files::SimpleFiles;
use codespan_reporting::term;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream};
use inquire::{validator::Validation, Text};

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

    /// Review the policy file without performing an audit
    #[clap(long, short, default_value_t = false)]
    review: bool,

    /// For debugging stuff
    #[clap(long, default_value_t = false)]
    debug: bool,
}

fn print_effect_src(
    effect_origin: &EffectBlock,
    effect: &EffectInfo,
    fn_locs: &HashMap<Path, SrcLoc>,
    config: &Config,
) -> Result<()> {
    // NOTE: The codespan lines are 0-indexed, but SrcLocs are 1-indexed
    let effect_loc = &effect.callee_loc.sub1();
    let mut full_path = effect_loc.dir().clone();
    full_path.push(effect_loc.file());

    let src_contents = std::fs::read_to_string(full_path)?;

    // Get the byte ranges for each line of the src file
    let src_lines = src_contents.split('\n');
    let mut src_linenum_ranges = HashMap::new();
    src_lines.fold((0, 0), |(lineno, byte_count), line| {
        src_linenum_ranges.insert(lineno, (byte_count, byte_count + line.len() + 1));
        (lineno + 1, byte_count + line.len() + 1)
    });

    // calculate the byte ranges for the effect
    let start_effect_line = effect_loc.start_line();
    let end_effect_line = effect_loc.end_line();
    let bounded_start_line =
        start_effect_line.saturating_sub(config.lines_before_effect as usize);
    let bounded_end_line = std::cmp::min(
        end_effect_line + config.lines_after_effect as usize,
        src_linenum_ranges.len(),
    );

    let surrounding_start = src_linenum_ranges.get(&bounded_start_line).unwrap().0;
    let surrounding_end = src_linenum_ranges.get(&bounded_end_line).unwrap().1;
    let effect_start = src_linenum_ranges.get(&start_effect_line).unwrap().0;
    let effect_end = src_linenum_ranges.get(&end_effect_line).unwrap().1;

    // TODO: cache files?
    let mut files = SimpleFiles::new();
    let file_id = files.add(format!("{}", effect_loc.file().display()), src_contents);

    // If the labels don't include the function signature, include it as
    // another label
    // NOTE: The codespan lines are 0-indexed, but SrcLocs are 1-indexed
    let mut labels = match fn_locs.get(&effect.caller_path).map(|x| x.sub1()) {
        Some(loc)
            if loc.start_line() < bounded_start_line
                && loc.end_line() < bounded_start_line =>
        {
            // The signature is entirely outside the current label range, so add
            // a new label with the signature
            let sig_start = src_linenum_ranges.get(&loc.start_line()).unwrap().0;
            let sig_end = src_linenum_ranges.get(&loc.end_line()).unwrap().1;
            vec![
                Label::primary(file_id, effect_start..effect_end),
                Label::secondary(file_id, sig_start..sig_end),
                Label::secondary(file_id, surrounding_start..surrounding_end),
            ]
        }
        Some(loc) if loc.start_line() < bounded_start_line => {
            // The start of the signature is outside the current label range, so
            // extend the surrounding range to include the start of the function
            // signature
            let sig_start = src_linenum_ranges.get(&loc.start_line()).unwrap().0;
            vec![
                Label::primary(file_id, effect_start..effect_end),
                Label::secondary(file_id, sig_start..surrounding_end),
            ]
        }
        Some(_) | None => {
            // The function signature is already included in the label range or
            // we couldn't find the function definition
            vec![
                Label::primary(file_id, effect_start..effect_end),
                Label::secondary(file_id, surrounding_start..surrounding_end),
            ]
        }
    };

    let label_msg =
        if effect_origin.containing_fn().fn_name.as_path() == &effect.caller_path {
            // We are in the original function, so print all the effects in the
            // EffectBlock
            effect_origin
                .effects()
                .iter()
                .filter_map(|x| match x.eff_type() {
                    Effect::SinkCall(sink) => Some(format!("sink call: {}", sink)),
                    Effect::FFICall(call) => Some(format!("ffi call: {}", call)),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            "call safety marked as callee-checked".to_string()
        };
    let l = labels.remove(0);
    labels.insert(0, l.with_message(label_msg));

    // construct the codespan diagnostic
    let diag = Diagnostic::help().with_code("Audit location").with_labels(labels);

    let writer = StandardStream::stderr(ColorChoice::Always);
    let config = codespan_reporting::term::Config::default();

    // Print the information to the user
    term::emit(&mut writer.lock(), &config, &files, &diag)?;

    Ok(())
}

struct CallStackInfo {
    fn_string: Option<String>,
    filename: String,
    lineno: usize,
}

impl CallStackInfo {
    fn new(fn_string: Option<String>, filename: String, lineno: usize) -> Self {
        Self { fn_string, filename, lineno }
    }
}

fn print_call_stack_infos(stack: Vec<CallStackInfo>) {
    let missing_fn_str = "Missing fn decl";
    // TODO: Colorize
    for CallStackInfo { fn_string, filename, lineno } in stack {
        println!("{}:{}", filename, lineno + 1);
        match fn_string {
            Some(f) => println!("    {}", f),
            None => println!("    {}", missing_fn_str),
        };
    }
}

fn fn_decl_info(fn_loc: &SrcLoc) -> Result<CallStackInfo> {
    let mut full_path = fn_loc.dir().clone();
    full_path.push(fn_loc.file());

    let src_contents = std::fs::read_to_string(full_path)?;

    // TODO: Print the full definition if it spans multiple lines
    let mut src_lines = src_contents.splitn(fn_loc.start_line() + 1, '\n');
    let src_fn_loc = src_lines
        .nth(fn_loc.start_line() - 1)
        .ok_or_else(|| anyhow!("Source lineno past end of file"))?;

    // TODO: Capture just the function name
    let res = CallStackInfo::new(
        Some(src_fn_loc.split('{').next().unwrap().trim().to_string()),
        format!("{}", fn_loc.dir().to_string_lossy()),
        fn_loc.start_line(),
    );
    Ok(res)
}

fn missing_fn_decl_info(effect_loc: &SrcLoc) -> CallStackInfo {
    let mut path_list = effect_loc.dir().clone();
    path_list.push(effect_loc.file());
    let full_path = path_list.join("/");
    let full_path_str = full_path.to_string_lossy().to_string();

    CallStackInfo::new(None, full_path_str, effect_loc.start_line())
}

fn print_call_stack(
    curr_effect: &EffectInfo,
    effect_history: &[&EffectInfo],
    fn_locs: &HashMap<Path, SrcLoc>,
) -> Result<()> {
    if !effect_history.is_empty() {
        let mut call_stack_infos = vec![];
        // TODO: Colorize
        println!("EffectInstance call stack:");
        let call_info = match fn_locs.get(&curr_effect.caller_path) {
            Some(fn_loc) => fn_decl_info(fn_loc)?,
            None => missing_fn_decl_info(&curr_effect.callee_loc),
        };
        call_stack_infos.push(call_info);

        for e in effect_history.iter().rev() {
            let call_info = match fn_locs.get(&e.caller_path) {
                Some(fn_loc) => fn_decl_info(fn_loc)?,
                None => missing_fn_decl_info(&e.callee_loc),
            };
            call_stack_infos.push(call_info);
        }

        print_call_stack_infos(call_stack_infos);
    }

    Ok(())
}

fn review_effect_tree_info_helper(
    orig_effect: &EffectBlock,
    effect_tree: &EffectTree,
    effect_history: &[&EffectInfo],
    fn_locs: &HashMap<Path, SrcLoc>,
    config: &Config,
) -> Result<()> {
    match effect_tree {
        EffectTree::Leaf(new_e, a) => {
            print_effect_info(orig_effect, new_e, effect_history, fn_locs, config)?;
            // TODO: Colorize
            println!("Policy annotation: {}", a);
        }
        EffectTree::Branch(new_e, es) => {
            // TODO: Colorize
            print_effect_info(orig_effect, new_e, effect_history, fn_locs, config)?;
            println!("Policy annotation: {}", SafetyAnnotation::CallerChecked);
            let mut new_history = effect_history.to_owned();
            new_history.push(new_e);
            for new_tree in es {
                review_effect_tree_info_helper(
                    orig_effect,
                    new_tree,
                    &new_history,
                    fn_locs,
                    config,
                )?
            }
        }
    }
    Ok(())
}

fn review_effect_tree_info(
    effect: &EffectBlock,
    effect_tree: &EffectTree,
    fn_locs: &HashMap<Path, SrcLoc>,
    config: &Config,
) -> Result<()> {
    review_effect_tree_info_helper(effect, effect_tree, &Vec::new(), fn_locs, config)
}

fn print_effect_info(
    orig_effect: &EffectBlock,
    curr_effect: &EffectInfo,
    effect_history: &[&EffectInfo],
    fn_locs: &HashMap<Path, SrcLoc>,
    config: &Config,
) -> Result<()> {
    println!();
    println!("=================================================");
    print_call_stack(curr_effect, effect_history, fn_locs)?;
    println!();
    print_effect_src(orig_effect, curr_effect, fn_locs, config)?;
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

fn is_policy_scan_valid(
    policy: &PolicyFile,
    scan_effect_blocks: &HashSet<&EffectBlock>,
    crate_path: PathBuf,
) -> Result<bool> {
    let policy_effect_blocks = policy.audit_trees.keys().collect::<HashSet<_>>();
    let hash = hash_dir(crate_path)?;
    // NOTE: We're checking the hash in addition to the effect blocks for now
    //       because we might have changed how we scan packages for effects.
    Ok(policy_effect_blocks == *scan_effect_blocks && policy.hash == hash)
}

fn review_policy(args: Args, policy: PolicyFile) -> Result<()> {
    let scan_res = scanner::scan_crate(&args.crate_path)?;
    let scan_effect_blocks = scan_res.unsafe_effect_blocks_set();
    if !is_policy_scan_valid(&policy, &scan_effect_blocks, args.crate_path.clone())? {
        println!("Error: crate has changed since last policy scan.");
        return Err(anyhow!("Invalid policy during review"));
    }

    for (e, a) in policy.audit_trees.iter() {
        review_effect_tree_info(e, a, &scan_res.fn_locs, &args.config)?;
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuditStatus {
    EarlyExit,
    ContinueAudit,
}

fn audit_leaf<'a>(
    orig_effect: &'a EffectBlock,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
    pub_caller_checked: &mut HashSet<Path>,
    config: &Config,
) -> Result<AuditStatus> {
    let curr_effect = match effect_tree {
        EffectTree::Leaf(e, _) => e.clone(),
        _ => {
            return Err(anyhow!("Tried to leaf audit a branch"));
        }
    };

    if print_effect_info(
        orig_effect,
        &curr_effect,
        effect_history,
        &scan_res.fn_locs,
        config,
    )
    .is_err()
    {
        println!("Error printing effect information. Trying to continue...");
    }

    let status = match get_user_annotation() {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Ok(AuditStatus::EarlyExit);
        }
        Err(_) => {
            println!("Error accepting user input. Attempting to continue...");
            SafetyAnnotation::Skipped
        }
    };

    if status == SafetyAnnotation::CallerChecked {
        // If the caller is public, add to set of public caller-checked
        if scan_res
            .pub_fns
            .contains(&CanonicalPath::from_path(curr_effect.caller_path.clone()))
        {
            pub_caller_checked.insert(curr_effect.caller_path.clone());
        }

        // Add all call locations as parents of this effect
        let new_check_locs = scan_res
            .get_callers(&curr_effect.caller_path)
            .into_iter()
            .map(|x| {
                EffectTree::Leaf(
                    EffectInfo::from_instance(&x.clone()),
                    SafetyAnnotation::Skipped,
                )
            })
            .collect::<Vec<_>>();

        if new_check_locs.is_empty() {
            effect_tree.set_annotation(status);
            Ok(AuditStatus::ContinueAudit)
        } else {
            *effect_tree = EffectTree::Branch(curr_effect, new_check_locs);
            audit_branch(
                orig_effect,
                effect_tree,
                effect_history,
                scan_res,
                pub_caller_checked,
                config,
            )
        }
    } else {
        effect_tree.set_annotation(status).ok_or_else(|| {
            anyhow!("Tried to set the EffectTree annotation, but was a branch node")
        })?;
        Ok(AuditStatus::ContinueAudit)
    }
}

fn audit_branch<'a>(
    orig_effect: &'a EffectBlock,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
    pub_caller_checked: &mut HashSet<Path>,
    config: &Config,
) -> Result<AuditStatus> {
    if let EffectTree::Branch(curr_effect, effects) = effect_tree {
        let mut next_history = effect_history.to_owned();
        next_history.push(curr_effect);
        for e in effects {
            // TODO: Early exit
            match e {
                next_e @ EffectTree::Branch(..) => {
                    if audit_branch(
                        orig_effect,
                        next_e,
                        &next_history,
                        scan_res,
                        pub_caller_checked,
                        config,
                    )? == AuditStatus::EarlyExit
                    {
                        return Ok(AuditStatus::EarlyExit);
                    }
                }
                next_e @ EffectTree::Leaf(..) => {
                    if audit_leaf(
                        orig_effect,
                        next_e,
                        &next_history,
                        scan_res,
                        pub_caller_checked,
                        config,
                    )? == AuditStatus::EarlyExit
                    {
                        return Ok(AuditStatus::EarlyExit);
                    }
                }
            };
        }
        Ok(AuditStatus::ContinueAudit)
    } else {
        Err(anyhow!("Tried to audit an EffectTree branch, but was actually a leaf"))
    }
}

fn audit_effect_tree(
    orig_effect: &EffectBlock,
    effect_tree: &mut EffectTree,
    scan_res: &ScanResults,
    pub_caller_checked: &mut HashSet<Path>,
    config: &Config,
) -> Result<AuditStatus> {
    match effect_tree {
        e @ EffectTree::Leaf(..) => {
            audit_leaf(orig_effect, e, &Vec::new(), scan_res, pub_caller_checked, config)
        }
        e @ EffectTree::Branch(..) => audit_branch(
            orig_effect,
            e,
            &Vec::new(),
            scan_res,
            pub_caller_checked,
            config,
        ),
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

    // Iterate through the effects and prompt the user for if they're safe
    for (e, t) in policy_file.audit_trees.iter_mut() {
        match t.get_leaf_annotation() {
            Some(SafetyAnnotation::Skipped) => {
                if audit_effect_tree(
                    e,
                    t,
                    &scan_res,
                    &mut policy_file.pub_caller_checked,
                    &args.config,
                )? == AuditStatus::EarlyExit
                {
                    break;
                }
            }
            Some(_) => (),
            None => {
                if audit_effect_tree(
                    e,
                    t,
                    &scan_res,
                    &mut policy_file.pub_caller_checked,
                    &args.config,
                )? == AuditStatus::EarlyExit
                {
                    break;
                }
            }
        }
    }

    policy_file.save_to_file(policy_path)?;

    Ok(())
}

fn runner(args: Args) -> Result<()> {
    let policy_file = PolicyFile::read_policy(args.policy_path.clone())?;

    if args.review {
        match policy_file {
            None => Err(anyhow!("Policy file to review doesn't exist")),
            Some(pf) => review_policy(args, pf),
        }
    } else {
        audit_crate(args, policy_file)
    }
}

fn main() {
    let args = Args::parse();

    match runner(args) {
        Ok(_) => println!("Finished checking policy"),
        Err(e) => println!("Error: {:?}", e),
    };
}
