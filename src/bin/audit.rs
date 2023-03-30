use cargo_scan::effect::{Effect, EffectBlock, EffectInstance, SrcLoc};
use cargo_scan::ident::Path;
use cargo_scan::scanner;
use cargo_scan::scanner::ScanResults;

use std::collections::{HashMap, HashSet};
use std::fmt;
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

    /// Review the policy file without performing an audit
    #[clap(long, short, default_value_t = false)]
    review: bool,

    /// For debugging stuff
    #[clap(long, default_value_t = false)]
    debug: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
enum SafetyAnnotation {
    Skipped,
    Safe,
    Unsafe,
    CallerChecked,
}

impl fmt::Display for SafetyAnnotation {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SafetyAnnotation::Skipped => write!(f, "Skipped"),
            SafetyAnnotation::Safe => write!(f, "Safe"),
            SafetyAnnotation::Unsafe => write!(f, "Unsafe"),
            SafetyAnnotation::CallerChecked => write!(f, "Caller-checked"),
        }
    }
}

fn hash_dir(p: PathBuf) -> Result<[u8; 32]> {
    let mut hasher = Sha256::new();
    for entry in WalkDir::new(p) {
        match entry {
            Ok(ne) if ne.path().is_file() => {
                let mut file = File::open(ne.path())?;
                let mut buf = Vec::new();
                file.read_to_end(&mut buf)?;
                hasher.update(buf);
            }
            _ => (),
        }
    }

    Ok(hasher.finalize().into())
}

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
struct EffectInfo {
    pub caller_path: Path,
    pub callee_loc: SrcLoc,
    // TODO: callee_src_span: SrcSpan,
}

impl EffectInfo {
    pub fn new(caller_path: Path, callee_loc: SrcLoc) -> Self {
        EffectInfo { caller_path, callee_loc }
    }

    pub fn from_instance(effect: &EffectInstance) -> Self {
        let caller_src_path = effect.caller().clone();
        let callee_loc = effect.call_loc().clone();

        EffectInfo::new(caller_src_path, callee_loc)
    }

    pub fn from_block(effect: &EffectBlock) -> Self {
        match effect.effects().first() {
            Some(e) => EffectInfo::from_instance(e),
            None => EffectInfo::new(Path::new(""), effect.src_loc().clone()),
        }
    }
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
enum EffectTree {
    Leaf(EffectInfo, SafetyAnnotation),
    Branch(EffectInfo, Vec<EffectTree>),
}

impl EffectTree {
    fn get_leaf_annotation(&self) -> Option<SafetyAnnotation> {
        match self {
            EffectTree::Leaf(_, a) => Some(*a),
            EffectTree::Branch(_, _) => None,
        }
    }

    /// Sets the annotation for a leaf node and returns Some previous annotation,
    /// or None if it was a branch node
    fn set_annotation(&mut self, new_a: SafetyAnnotation) -> Option<SafetyAnnotation> {
        match self {
            EffectTree::Leaf(_, a) => {
                let ret = *a;
                *a = new_a;
                Some(ret)
            }
            _ => None,
        }
    }
}

// TODO: Include information about crate/version
// TODO: We should include more information from the ScanResult
#[serde_as]
#[derive(Serialize, Deserialize)]
struct PolicyFile {
    // TODO: Serde doesn't like this hashmap for some reason (?)
    #[serde_as(as = "Vec<(_, _)>")]
    audit_trees: HashMap<EffectBlock, EffectTree>,
    // TODO: Make the base_dir a crate instead
    base_dir: PathBuf,
    hash: [u8; 32],
}

impl PolicyFile {
    fn new(p: PathBuf) -> Result<Self> {
        let hash = hash_dir(p.clone())?;
        Ok(PolicyFile { audit_trees: HashMap::new(), base_dir: p, hash })
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
    // TODO: make this display a message if the file isn't the proper format
    let json = std::fs::read_to_string(policy_filepath)?;

    // If we try to read an empty file, just make a new one
    if json.is_empty() {
        return Ok(None);
    }
    let policy_file = serde_json::from_str(&json)?;
    Ok(Some(policy_file))
}

fn print_effect_src(
    effect_origin: &EffectBlock,
    effect: &EffectInfo,
    config: &Config,
) -> Result<()> {
    let effect_loc = &effect.callee_loc;
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
    // TODO: Off by 1? Might have to change in the effect calculation.
    // TODO: Highlight the entire expression as the main error if it's multi-line
    //       and update the surrounding lines correspondingly
    let effect_line = effect_loc.line() - 1;
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
    let file_id = files.add(format!("{}", effect_loc.file().display()), src_contents);

    // construct the codespan diagnostic
    let mut diag_string = "effects: ".to_string();
    for e in effect_origin.effects().iter().map(|e| match e.eff_type() {
        Effect::SinkCall(sink) => format!("sink - {}", sink),
        Effect::FFICall(path) => format!("ffi call - {}", path),
        Effect::UnsafeOp => "unsafe op".to_string(),
        Effect::OtherCall => todo!(),
    }) {
        diag_string.push_str(&format!("{}\n", e));
    }
    let diag = Diagnostic::error().with_message(diag_string).with_labels(vec![
        Label::primary(file_id, effect_start..effect_end),
        Label::secondary(file_id, surrounding_start..surrounding_end),
    ]);

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
    let mut src_lines = src_contents.splitn(fn_loc.line() + 1, '\n');
    let src_fn_loc = src_lines
        .nth(fn_loc.line() - 1)
        .ok_or_else(|| anyhow!("Source lineno past end of file"))?;

    // TODO: Capture just the function name
    let res = CallStackInfo::new(
        Some(src_fn_loc.split('{').next().unwrap().trim().to_string()),
        format!("{}", fn_loc.dir().to_string_lossy()),
        fn_loc.line(),
    );
    Ok(res)
}

fn missing_fn_decl_info(effect_loc: &SrcLoc) -> CallStackInfo {
    let mut path_list = effect_loc.dir().clone();
    path_list.push(effect_loc.file());
    let full_path = path_list.join("/");
    let full_path_str = full_path.to_string_lossy().to_string();

    CallStackInfo::new(None, full_path_str, effect_loc.line())
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
    print_effect_src(orig_effect, curr_effect, config)?;
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
) -> Result<ContinueStatus> {
    // TODO: Colorize
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

    // TODO: Handle no call sites
    if status == SafetyAnnotation::CallerChecked {
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
        *effect_tree = EffectTree::Branch(curr_effect, new_check_locs);
        audit_branch(orig_effect, effect_tree, effect_history, scan_res, config)
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
    config: &Config,
) -> Result<AuditStatus> {
    if let EffectTree::Branch(curr_effect, effects) = effect_tree {
        let mut next_history = effect_history.to_owned();
        next_history.push(curr_effect);
        for e in effects {
            // TODO: Early exit
            match e {
                next_e @ EffectTree::Branch(..) => {
                    if audit_branch(orig_effect, next_e, &next_history, scan_res, config)?
                        == AuditStatus::EarlyExit
                    {
                        return Ok(AuditStatus::EarlyExit);
                    }
                }
                next_e @ EffectTree::Leaf(..) => {
                    if audit_leaf(orig_effect, next_e, &next_history, scan_res, config)?
                        == AuditStatus::EarlyExit
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
    config: &Config,
) -> Result<AuditStatus> {
    match effect_tree {
        e @ EffectTree::Leaf(..) => {
            audit_leaf(orig_effect, e, &Vec::new(), scan_res, config)
        }
        e @ EffectTree::Branch(..) => {
            audit_branch(orig_effect, e, &Vec::new(), scan_res, config)
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
            let mut pf = PolicyFile::new(args.crate_path.clone())?;
            pf.audit_trees = scan_effect_blocks
                .clone()
                .into_iter()
                .map(|x| {
                    (
                        x.clone(),
                        EffectTree::Leaf(
                            EffectInfo::from_block(x),
                            SafetyAnnotation::Skipped,
                        ),
                    )
                })
                .collect::<HashMap<_, _>>();
            pf
        }
    };

    // Iterate through the effects and prompt the user for if they're safe
    for (e, t) in policy_file.audit_trees.iter_mut() {
        match t.get_leaf_annotation() {
            Some(SafetyAnnotation::Skipped) => {
                if audit_effect_tree(e, t, &scan_res, &args.config)?
                    == AuditStatus::EarlyExit
                {
                    break;
                }
            }
            Some(_) => (),
            None => {
                if audit_effect_tree(e, t, &scan_res, &args.config)?
                    == AuditStatus::EarlyExit
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
    let policy_file = get_policy_file(args.policy_path.clone())?;

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
