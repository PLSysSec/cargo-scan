use anyhow::{anyhow, Result};
use cargo_lock::{Lockfile, Package};
use clap::Parser;
use log::{debug, info};
use petgraph::{
    graph::{DiGraph, NodeIndex},
    visit::DfsPostOrder,
};
use semver::Version;
use std::{
    collections::{HashMap, HashSet},
    fs::{create_dir, create_dir_all},
    io::Write,
    path::{Path, PathBuf},
};

use cargo_scan::{
    audit_file::{AuditFile, EffectTree, SafetyAnnotation},
    download_crate,
    effect::EffectInstance,
    ident::{CanonicalPath, IdentPath},
    scanner::{scan_crate, ScanResults},
    util::{self, fs::walk_files_with_extension, CrateId},
};

// Results
const RESULTS_DIR: &str = "data/results/audits/";
const RESULTS_CC_SUFFIX: &str = "caller_checked.csv";
const RESULTS_SINKS_SUFFIX: &str = "sinks.csv";
const RESULTS_SUMMARY_SUFFIX: &str = "stats_summary.csv";

// Headers
const STATS_CC_HEADER: &str = "avg_call_stack";
const STATS_SUMMARY_HEADER: &str = "crate, total_fns, total_loc, audited_loc, total_caller_checked, total_avg_call_stack, total_sinks, total_pub_fns, pub_fns_cc";

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the audits directory to collectively get statistics
    audits_dir: PathBuf,
    /// Path to get statistics for specific audit file
    #[clap(short = 'f', long = "audit-file")]
    audit_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct AuditingStats {
    // Audited crate
    crate_id: CrateId,
    // Total number of audited functions
    total_fns: usize,
    // Total lines of code in functions we consider in the crate
    total_loc: usize,
    // Total lines of audited code
    audited_loc: usize,
    // Base effects that are marked caller-checked
    // and the size of the corresponding `EffectTree`
    caller_checked_effects: HashMap<EffectInstance, usize>,
    // Total number of base effects in the audit
    total_effects: usize,
    // Set of sink calls that flow from dependencies
    sink_calls: HashSet<EffectInstance>,
    // How many public functions are in the crate
    pub_fns: usize,
    // How many public functions are marked caller-checked
    pub_fns_cc: usize,
}

impl AuditingStats {
    pub fn total_avg_call_stack(&self) -> f32 {
        if self.caller_checked_effects.is_empty() {
            return 0.0;
        }

        let sum = self.caller_checked_effects.values().sum::<usize>() as f32;
        sum / self.caller_checked_effects.len() as f32
    }
}

// Count the average size of the call-stack
// that was audited for this effect.
fn count_tree_size(tree: &EffectTree) -> usize {
    match tree {
        EffectTree::Leaf(_, _) => 1,
        EffectTree::Branch(_, ts) => ts.iter().fold(1, |s, t| s + count_tree_size(t)),
    }
}

fn effects_marked_caller_checked(
    effect: &EffectInstance,
    tree: &EffectTree,
    caller_checked: &mut HashMap<EffectInstance, usize>,
) {
    match tree {
        EffectTree::Leaf(info, SafetyAnnotation::CallerChecked)
            if &info.caller_path == effect.caller() =>
        {
            caller_checked.insert(effect.clone(), 1);
        }
        EffectTree::Branch(info, _) if &info.caller_path == effect.caller() => {
            caller_checked.insert(effect.clone(), count_tree_size(tree));
        }
        _ => (),
    }
}

// Hacky workaround to check equality of `CanonicalPath`s
// because they contain `SrcLoc`s with absolute dirs
fn custom_eq(cp1: &CanonicalPath, cp2: &CanonicalPath) -> bool {
    if cp1.as_path() != cp2.as_path() {
        return false;
    }

    let loc1 = cp1.get_src_loc();
    let loc2 = cp2.get_src_loc();
    let dir1 = loc1.dir().to_string_lossy();
    let dir2 = loc2.dir().to_string_lossy();
    let Some(dir1) = dir1.rsplit_once("cargo-scan") else { return false };
    let Some(dir2) = dir2.rsplit_once("cargo-scan") else { return false };

    dir1.1 == dir2.1
        && loc1.file() == loc2.file()
        && loc1.start_line() == loc2.start_line()
        && loc1.start_col() == loc2.start_col()
        && loc1.end_line() == loc2.end_line()
        && loc1.end_col() == loc2.end_col()
}

fn compute_stats(
    crate_id: CrateId,
    audit: &AuditFile,
    results: &ScanResults,
    sinks: &HashSet<IdentPath>,
) -> AuditingStats {
    let mut audited_loc = 0;
    let mut total_fns = HashSet::new();
    let mut sink_calls = HashSet::new();
    let mut caller_checked_effects = HashMap::new();
    let total_effects = audit.audit_trees.keys().len();

    // Collect effects marked caller-checked and total audited functions
    for (effect, tree) in &audit.audit_trees {
        total_fns.extend(counter(tree));
        effects_marked_caller_checked(effect, tree, &mut caller_checked_effects)
    }

    // Collect total lines of code for audited functions
    for f in &total_fns {
        let mut found = false;

        for (key, tracker) in results.fn_loc_tracker.iter() {
            if custom_eq(key, f) {
                audited_loc += tracker.get_loc_lb();
                found = true;
                break;
            }
        }
        if !found {
            debug!(
                "failed to find tracker node for `{:?}`. possibly it is a trait method.",
                f.as_str()
            );
        }
    }

    // Collect total number of sink calls that flow from dependencies
    let sink_effects = audit.audit_trees.keys().filter(|eff| {
        matches!(eff.eff_type(), cargo_scan::effect::Effect::SinkCall(_))
            && sinks.contains(eff.callee().as_path())
    });
    sink_calls.extend(sink_effects.cloned());

    let total_loc =
        results.fn_loc_tracker.values().fold(0, |acc, x| acc + x.get_loc_lb());

    AuditingStats {
        crate_id,
        total_fns: total_fns.len(),
        total_loc,
        audited_loc,
        caller_checked_effects,
        total_effects,
        sink_calls,
        pub_fns: results.pub_fns.len(),
        pub_fns_cc: audit.pub_caller_checked.len(),
    }
}

fn counter(tree: &EffectTree) -> HashSet<&CanonicalPath> {
    let mut set: HashSet<&CanonicalPath> = HashSet::new();

    match tree {
        EffectTree::Leaf(info, _) => {
            set.insert(&info.caller_path);
        }
        EffectTree::Branch(info, branch) => {
            let s = branch.iter().fold(HashSet::new(), |mut set, tree| {
                set.extend(counter(tree));

                set
            });

            set.insert(&info.caller_path);
            set.extend(s);
        }
    };

    set
}

// Read the audit file from the input path and get the audited crate
fn get_crate_from_audit_file(audit_file_path: &Path) -> Result<(CrateId, AuditFile)> {
    let audit_file = AuditFile::read_audit_file(audit_file_path.to_path_buf())?
        .expect("Could not find audit file");

    let filename = audit_file_path
        .file_name()
        .expect("Should be a correct audit file")
        .to_string_lossy();
    let crate_ = filename.strip_suffix(".audit").unwrap();

    let (name, version) = crate_
        .rsplit_once('-')
        .expect("Could not retrieve crate name and version from audit file");
    let id = CrateId::new(name.to_string(), Version::parse(version)?);

    Ok((id, audit_file))
}

fn download_crate(crate_path: &Path, crate_id: &CrateId) -> Result<bool> {
    let Some(download_dir) = crate_path.parent() else {
        return Err(anyhow!("Incorrect crate directory"));
    };

    // If the crate path does not exist, create the directory
    // and download the crate for scanning.
    create_dir_all(download_dir)?;
    if !crate_path.is_dir() {
        debug!("Downloading crate {} in {:?}", crate_id, download_dir);
        download_crate::download_crate_from_info(
            &crate_id.crate_name,
            &crate_id.version.to_string(),
            download_dir.to_str().unwrap(),
        )?;
        return Ok(true);
    }

    Ok(false)
}

fn get_lockfile(crate_path: &Path) -> Result<Lockfile> {
    let mut lock_path = crate_path.canonicalize()?;
    lock_path.push("Cargo.lock");
    let lockfile = Lockfile::load(lock_path)?;

    Ok(lockfile)
}

// Code from David
fn make_dependency_graph(
    packages: &Vec<Package>,
    root_name: &str,
) -> (DiGraph<String, ()>, HashMap<NodeIndex, Package>, NodeIndex) {
    let mut graph = DiGraph::new();
    let mut node_map = HashMap::new();
    let mut package_map = HashMap::new();

    for p in packages {
        let p_string = format!("{}-{}", p.name.as_str(), p.version);
        if !node_map.contains_key(&p_string) {
            let next_node = graph.add_node(p_string.clone());
            node_map.insert(p_string.clone(), next_node);
        }
        // Clone to avoid multiple mutable borrow
        let p_idx = *node_map.get(&p_string).unwrap();
        package_map.insert(p_idx, p.clone());

        for dep in &p.dependencies {
            let dep_string = format!("{}-{}", dep.name.as_str(), dep.version);
            if !node_map.contains_key(&dep_string) {
                let next_node = graph.add_node(dep_string.clone());
                node_map.insert(dep_string.clone(), next_node);
            }
            let dep_idx = *node_map.get(&dep_string).unwrap();
            graph.add_edge(p_idx, dep_idx, ());
        }
    }

    let root_idx = *node_map.get(root_name).unwrap();
    (graph, package_map, root_idx)
}

fn get_sinks_from_deps(
    audits_dir: &Path,
    lockfile: &Lockfile,
    crate_id: &CrateId,
) -> HashSet<IdentPath> {
    let mut sinks = HashSet::new();
    // Build dependency graph to find public functions from
    // dependencies that flow as effects to the current audit
    let (graph, nodes, root) =
        make_dependency_graph(&lockfile.packages, &crate_id.to_string());

    let mut traverse = DfsPostOrder::new(&graph, root);
    while let Some(node) = traverse.next(&graph) {
        if root == node {
            continue;
        }
        let dep = nodes.get(&node).unwrap();
        let dep_id = CrateId::from(dep);
        let dep_audit_file =
            audits_dir.join(PathBuf::from(dep_id.to_string() + ".audit"));

        // Read dependency's AuditFile, if it exists, to get public caller-checked functions
        match AuditFile::read_audit_file(dep_audit_file.clone()) {
            Ok(Some(dep_audit)) => sinks.extend(
                dep_audit.pub_caller_checked.keys().map(|x| x.as_path()).cloned(),
            ),
            _ => debug!("Could not find dependency audit file: {:?}", dep_audit_file),
        }
    }

    sinks
}

fn get_auditing_stats(
    audits_dir: &Path,
    audit_file_path: &Path,
) -> Result<AuditingStats> {
    let (crate_id, audit_file) = get_crate_from_audit_file(audit_file_path)?;
    let downloaded = download_crate(&audit_file.base_dir, &crate_id)?;
    let crate_path = &audit_file.base_dir;

    // Need to re-run the scan for this package to retrieve information about the functions present in the audit.
    let results = scan_crate(crate_path, &audit_file.scanned_effects, false)?;

    // Load lockfile to get dependencies of audited package
    let lockfile = get_lockfile(crate_path)?;

    // Get sink functions from dependencies that flow into this audit
    let sinks = get_sinks_from_deps(audits_dir, &lockfile, &crate_id);

    // Get audit statistics
    let stats = compute_stats(crate_id, &audit_file, &results, &sinks);

    // Remove crate directory, if it was created only for the statistics
    if downloaded {
        std::fs::remove_dir_all(crate_path)?;
    }

    Ok(stats)
}

fn dump_summary(all_stats: &Vec<AuditingStats>) -> Result<()> {
    let mut path = PathBuf::from(RESULTS_DIR);
    if !path.is_dir() {
        create_dir(&path)?;
    }
    path = path.join(RESULTS_SUMMARY_SUFFIX);
    let mut output = util::fs::path_writer(path);
    writeln!(output, "{}", STATS_SUMMARY_HEADER)?;

    // Dump summary for all audits
    for stats in all_stats {
        writeln!(
            output,
            "{}, {}, {}, {}, {}/{}, {}, {}, {}, {}",
            stats.crate_id,
            stats.total_fns,
            stats.total_loc,
            stats.audited_loc,
            stats.caller_checked_effects.len(),
            stats.total_effects,
            stats.total_avg_call_stack(),
            stats.sink_calls.len(),
            stats.pub_fns,
            stats.pub_fns_cc,
        )?;
    }

    Ok(())
}

fn dump_caller_checked(all_stats: &Vec<AuditingStats>) -> Result<()> {
    let mut path = PathBuf::from(RESULTS_DIR);
    if !path.is_dir() {
        create_dir(&path)?;
    }
    path = path.join(RESULTS_CC_SUFFIX);
    let mut output = util::fs::path_writer(path);
    writeln!(output, "{}, {}", STATS_CC_HEADER, EffectInstance::csv_header())?;

    // Dump caller-checked effects' info
    for stats in all_stats {
        for (eff, s) in &stats.caller_checked_effects {
            writeln!(output, "{}, {}", s, eff.to_csv())?;
        }
    }

    Ok(())
}

fn dump_sinks(all_stats: &Vec<AuditingStats>) -> Result<()> {
    let mut path = PathBuf::from(RESULTS_DIR);
    if !path.is_dir() {
        create_dir(&path)?;
    }
    path = path.join(RESULTS_SINKS_SUFFIX);
    let mut output = util::fs::path_writer(path);
    writeln!(output, "{}", EffectInstance::csv_header())?;

    for stats in all_stats {
        for eff in &stats.sink_calls {
            writeln!(output, "{}", eff.to_csv())?;
        }
    }

    Ok(())
}
fn main() -> Result<()> {
    cargo_scan::util::init_logging();
    let args = Args::parse();
    let mut all_stats = Vec::new();

    let audits_dir = args.audits_dir;
    if !audits_dir.is_dir() {
        return Err(anyhow!("Argument `{:?}` is not a valid directory", audits_dir));
    }

    match args.audit_file {
        Some(file) => {
            // Compute stats for specific audit file only
            let stats = get_auditing_stats(&audits_dir, &audits_dir.join(file))?;
            all_stats.push(stats.clone());
        }
        None => {
            // Compute stats for all audit files in the input directory
            for file in walk_files_with_extension(&audits_dir, "audit") {
                info!("Collecting stats for audit: {:?}", file);
                let stats = get_auditing_stats(&audits_dir, &file)?;
                all_stats.push(stats.clone());
            }
        }
    };

    // Save statistics for audits
    dump_sinks(&all_stats)?;
    dump_summary(&all_stats)?;
    dump_caller_checked(&all_stats)?;

    Ok(())
}
