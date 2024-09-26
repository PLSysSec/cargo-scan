use anyhow::{anyhow, Context, Result};
use cargo::core::Workspace;
use cargo::ops::{fetch, generate_lockfile, FetchOptions};
use cargo::util::context::GlobalContext;
use cargo_lock::{Dependency, Lockfile, Package};
use cargo_toml::Manifest;
use clap::Args as ClapArgs;
use log::info;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{Dfs, DfsPostOrder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, remove_file, File};
use std::io::Write;
use std::iter::IntoIterator;
use std::mem;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use toml;

use crate::audit_file::{AuditFile, AuditVersion, DefaultAuditType, EffectInfo};
use crate::effect::{EffectInstance, EffectType, DEFAULT_EFFECT_TYPES};
use crate::ident::{replace_hyphens, CanonicalPath, IdentPath};
use crate::util::{load_cargo_toml, CrateId};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuditChain {
    #[serde(skip)]
    manifest_path: PathBuf,
    crate_path: PathBuf,
    crate_policies: HashMap<CrateId, (PathBuf, AuditVersion)>,
    scanned_effects: Vec<EffectType>,
}

impl AuditChain {
    pub fn new(
        manifest_path: PathBuf,
        crate_path: PathBuf,
        scanned_effects: Vec<EffectType>,
    ) -> AuditChain {
        AuditChain {
            manifest_path,
            crate_path,
            crate_policies: HashMap::new(),
            scanned_effects,
        }
    }

    pub fn all_crates(&self) -> Vec<&CrateId> {
        self.crate_policies.keys().collect::<Vec<_>>()
    }

    pub fn matching_crates_no_version(&self, crate_name: &str) -> Vec<CrateId> {
        self.crate_policies
            .keys()
            .filter(|x| x.crate_name == crate_name)
            .cloned()
            .collect::<Vec<_>>()
    }

    pub fn read_audit_chain(path: PathBuf) -> Result<Option<AuditChain>> {
        if path.is_dir() {
            Err(anyhow!("Manifest path is a directory"))
        } else if path.is_file() {
            let toml_string = std::fs::read_to_string(path.as_path())?;
            let mut audit_chain: AuditChain = toml::from_str(&toml_string)?;
            audit_chain.manifest_path = path;
            Ok(Some(audit_chain))
        } else {
            Ok(None)
        }
    }

    pub fn save_to_file(mut self) -> Result<()> {
        let path = mem::take(&mut self.manifest_path);
        let mut f = File::create(path)?;
        let toml = toml::to_string(&self)?;
        f.write_all(toml.as_bytes())?;
        Ok(())
    }

    pub fn add_crate_audit_file(
        &mut self,
        package: &Package,
        audit_file_loc: PathBuf,
        version: AuditVersion,
    ) {
        let crate_id = CrateId::from(package);
        self.crate_policies.insert(crate_id, (audit_file_loc, version));
    }

    pub fn read_audit_file(&mut self, crate_id: &CrateId) -> Result<Option<AuditFile>> {
        let (audit_file_path, expected_version) = self
            .crate_policies
            .get(crate_id)
            .context("Can't find an associated audit for the crate")?
            .clone();
        match AuditFile::read_audit_file(audit_file_path.clone())? {
            Some(audit_file) => {
                if audit_file.version != expected_version {
                    // Update version in chain manifest, so we don't loop infinitely
                    self.crate_policies
                        .get_mut(crate_id)
                        .context("Couldn't find the crate in the chain manifest")?
                        .1 = audit_file.version;

                    // The audit file has been updated in a different audit, so we need to
                    // recalculate the policies for its parents and save the changes
                    let potentially_removed = audit_file.safe_pub_fns();
                    self.remove_cross_crate_effects(potentially_removed, crate_id)?;
                    self.clone().save_to_file()?;

                    // re-read the audit file so changes have taken effect
                    // NOTE: This assumes there aren't concurrent audits modifying policies
                    AuditFile::read_audit_file(audit_file_path)
                } else {
                    Ok(Some(audit_file))
                }
            }
            None => Ok(None),
        }
    }

    pub fn collect_all_safe_sinks(&mut self) -> Result<HashSet<CanonicalPath>> {
        let mut safe_sinks = HashSet::new();
        for (crate_id, (af_path, _)) in &self.crate_policies {
            let audit_file = AuditFile::read_audit_file(af_path.clone())?.context(
                format!("Can't find an associated audit for crate `{}`", crate_id),
            )?;
            safe_sinks.extend(audit_file.safe_pub_fns());
        }

        Ok(safe_sinks)
    }

    /// Returns the full package name with version if there is exactly one
    /// package matching the input, or none otherwise
    pub fn resolve_crate_id(&self, crate_name: &str) -> Option<CrateId> {
        match &self.resolve_all_crates(crate_name)[..] {
            [p] => Some(p.clone()),
            _ => None,
        }
    }

    /// Returns all matching full package names with the version
    pub fn resolve_all_crates(&self, search_name: &str) -> Vec<CrateId> {
        let mut res = Vec::new();
        for (crate_id, _) in self.crate_policies.iter() {
            let mut crate_name = crate_id.crate_name.clone();
            replace_hyphens(&mut crate_name);
            if crate_name == search_name || crate_id.crate_name == search_name {
                res.push(crate_id.clone());
            }
        }
        res
    }

    pub fn read_audit_file_no_version(
        &mut self,
        crate_name: &str,
    ) -> Result<Option<(CrateId, AuditFile)>> {
        if let Some(crate_id) = self.resolve_crate_id(crate_name) {
            if let Some(audit_file) = self.read_audit_file(&crate_id)? {
                return Ok(Some((crate_id, audit_file)));
            }
        }
        Ok(None)
    }

    /// Looks up where the audit file is saved from the full crate name and saves the
    /// given AuditFile to the PathBuf associated with that crate.
    pub fn save_audit_file(
        &mut self,
        crate_id: &CrateId,
        audit_file: &AuditFile,
    ) -> Result<()> {
        let (audit_file_path, audit_version) = self
            .crate_policies
            .get_mut(crate_id)
            .ok_or_else(|| anyhow!("Couldn't find entry for crate: {}", crate_id))?;
        *audit_version = audit_file.version;
        audit_file.save_to_file(audit_file_path.clone())
    }

    /// Loads the lockfile for the given crate path. Will generate a new one
    /// with the default configuration if none exists.
    pub fn load_lockfile(&self) -> Result<Lockfile> {
        let mut crate_path = self.crate_path.clone();
        crate_path = crate_path.canonicalize()?;
        crate_path.push("Cargo.lock");
        if let Ok(l) = Lockfile::load(&crate_path) {
            Ok(l)
        } else {
            info!("Lockfile missing: generating new lockfile");
            let config = GlobalContext::default()?;
            crate_path.pop();
            crate_path.push("Cargo.toml");
            let workspace = Workspace::new(&crate_path, &config)?;
            generate_lockfile(&workspace)?;
            crate_path.pop();
            crate_path.push("Cargo.lock");
            let l = Lockfile::load(&crate_path)?;
            Ok(l)
        }
    }

    // TODO: Write a test for this to make sure it's properly recalculating
    //       dependency policies when they are invalid. It's going to be almost
    //       impossible to tell if something has gone wrong here.
    /// Removes all effects that originate from `removed_fns` for all parent
    /// crates of `updated_crate` in the AuditChain's dependency graph.
    /// `updated_crate should the full crate name with version`. Returns the
    /// set of removed functions if it succeeds.
    pub fn remove_cross_crate_effects(
        &mut self,
        mut removed_fns: HashSet<CanonicalPath>,
        updated_crate: &CrateId,
    ) -> Result<HashSet<CanonicalPath>> {
        let lockfile = self.load_lockfile()?;
        let dep_tree = lockfile.dependency_tree()?;
        let dep_nodes = dep_tree.nodes();
        let dep_graph = dep_tree.graph();

        let start_package = lookup_package_from_name(updated_crate, lockfile.packages)?;

        let start_node = dep_nodes.get(&Dependency::from(&start_package)).context(
            format!("Missing package {:?} in the dependency graph", start_package),
        )?;
        // Visit nodes in dfs post-order so we don't have to recursively add
        // packages whose public caller-checked functions have been updated.
        let mut visit = DfsPostOrder::new(dep_graph, *start_node);
        while let Some(n) = visit.next(dep_graph) {
            // TODO: Only update packages whose dependencies have changed public
            //       caller-checked lists.
            let package = &dep_graph[n];
            let crate_id = CrateId::from(package);
            let mut crate_audit_file = self
                .read_audit_file(&crate_id)?
                .context(format!("Couldn't find audit for {}", crate_id))?;
            let starting_pub_caller_checked = crate_audit_file
                .pub_caller_checked
                .keys()
                .cloned()
                .collect::<HashSet<_>>();

            let removed_effect_instances =
                crate_audit_file.remove_sinks_from_tree(&removed_fns);
            let package_pub_fns = &crate_audit_file
                .pub_caller_checked
                .keys()
                .cloned()
                .collect::<HashSet<_>>();
            let next_removed_fns = starting_pub_caller_checked
                .difference(package_pub_fns)
                .cloned()
                .collect::<Vec<_>>();
            if !next_removed_fns.is_empty() || !removed_effect_instances.is_empty() {
                // If the audit file changes, we need to bump the version so
                // other audit chains know to recalculate their effects
                crate_audit_file.version += 1;
            }
            removed_fns.extend(next_removed_fns);

            // reconstruct invariant
            crate_audit_file.recalc_pub_caller_checked(&starting_pub_caller_checked);
            crate_audit_file.save_to_file(
                self.crate_policies
                    .get(&crate_id)
                    .context(format!("Missing crate {} from chain", &crate_id))?
                    .0
                    .clone(),
            )?;

            self.crate_policies
                .get_mut(&crate_id)
                .context("Couldn't find the crate in the chain manifest")?
                .1 = crate_audit_file.version;
        }

        Ok(removed_fns)
    }

    /// Gets the root crate id
    pub fn root_crate(&self) -> Result<CrateId> {
        let root_package = Manifest::from_path(format!(
            "{}/Cargo.toml",
            self.crate_path.to_string_lossy()
        ))?
        .package
        .ok_or_else(|| anyhow!("Can't load root package for the root crate path"))?;

        CrateId::from_toml_package(&root_package)
    }
}

#[derive(Clone, ClapArgs, Debug, Serialize, Deserialize)]
pub struct Create {
    /// Path to crate
    pub crate_path: String,
    /// Path to manifest
    pub manifest_path: String,

    // TODO: Check to make sure it meets the format (clap supports this?)
    /// Default audit folder
    #[clap(short = 'p', long = "audit-path", default_value = ".audit_files")]
    pub audit_path: String,

    #[clap(short = 'f', long, default_value_t = false)]
    pub force_overwrite: bool,

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
    pub effect_types: Vec<EffectType>,
}

impl Create {
    pub fn new(
        crate_path: String,
        manifest_path: String,
        audit_file_path: String,
        force_overwrite: bool,
        download_root_crate: Option<String>,
        download_version: Option<String>,
        effect_types: Vec<EffectType>,
    ) -> Self {
        Self {
            crate_path,
            manifest_path,
            audit_path: audit_file_path,
            force_overwrite,
            download_root_crate,
            download_version,
            effect_types,
        }
    }
}

impl Default for Create {
    fn default() -> Self {
        let audit_path = home::home_dir()
            .map(|mut dir| {
                dir.push(".cargo_audits");
                dir
            })
            .unwrap_or_else(|| PathBuf::from(".audit_files"))
            .to_string_lossy()
            .to_string();

        Self {
            crate_path: ".".to_string(),
            manifest_path: "./policy.manifest".to_string(),
            audit_path,
            force_overwrite: false,
            download_root_crate: None,
            download_version: None,
            effect_types: DEFAULT_EFFECT_TYPES.to_vec(),
        }
    }
}

fn create_audit_chain_dirs(args: &Create, crate_download_path: &str) -> Result<()> {
    let mut manifest_path = PathBuf::from(&args.manifest_path);
    manifest_path.pop();
    create_dir_all(manifest_path)?;

    let crate_download_path = PathBuf::from(crate_download_path);
    create_dir_all(crate_download_path)?;

    let audit_file_path = PathBuf::from(&args.audit_path);
    create_dir_all(audit_file_path)?;

    Ok(())
}

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

fn collect_dependency_sinks(
    chain: &mut AuditChain,
    deps: &Vec<Dependency>,
) -> Result<HashSet<CanonicalPath>> {
    let mut sinks = HashSet::new();
    for dep in deps {
        let dep_id = CrateId::from(dep);
        let audit_file = chain.read_audit_file(&dep_id)?.context(
            "couldnt read dependency audit file (maybe created it out of order)",
        )?;
        sinks.extend(audit_file.pub_caller_checked.keys().cloned());
    }

    Ok(sinks)
}

/// Creates a new default audit file for the given package and returns the path to
/// the saved audit file
#[allow(clippy::too_many_arguments)]
fn make_new_audit_file(
    chain: &mut AuditChain,
    package: &Package,
    root_name: &str,
    args: &Create,
    crate_path: &Path,
    audit_type: DefaultAuditType,
    relevant_effects: &[EffectType],
    quick_mode: bool,
) -> Result<()> {
    let audit_file_path = PathBuf::from(format!(
        "{}/{}-{}.audit",
        args.audit_path,
        package.name.as_str(),
        package.version
    ));
    // download the new audit
    let full_name = format!("{}-{}", package.name, package.version);
    let package_path = if full_name == root_name {
        // We are creating a audit for the root crate
        PathBuf::from(args.crate_path.clone()).canonicalize()?
    } else {
        PathBuf::from(crate_path).canonicalize()?
    };

    // Try to create a new default audit
    if audit_file_path.is_dir() {
        return Err(anyhow!("Audit path is a directory"));
    }
    if audit_file_path.is_file() {
        if args.force_overwrite {
            remove_file(audit_file_path.clone())?;
        } else {
            info!(
                "Using existing audit for {} v{} ({})",
                package.name,
                package.version,
                audit_file_path.display()
            );
            let audit_file = AuditFile::read_audit_file(audit_file_path.clone())?
                .ok_or_else(|| {
                    anyhow!("Couldn't read audit: {}", audit_file_path.display())
                })?;
            chain.add_crate_audit_file(package, audit_file_path, audit_file.version);

            return Ok(());
        }
    }

    info!("Making default audit for {} v{}", package.name, package.version);
    let sinks = collect_dependency_sinks(chain, &package.dependencies)?;
    let audit_file = AuditFile::new_default_with_sinks(
        &package_path,
        sinks,
        audit_type,
        relevant_effects,
        quick_mode,
    )?;
    audit_file.save_to_file(audit_file_path.clone())?;

    chain.add_crate_audit_file(package, audit_file_path, audit_file.version);

    Ok(())
}

pub fn create_new_audit_chain(
    args: Create,
    crate_download_path: &str,
    quick_mode: bool,
) -> Result<AuditChain> {
    info!("Creating audit chain");
    let mut chain = AuditChain::new(
        PathBuf::from(&args.manifest_path),
        PathBuf::from(&args.crate_path),
        args.effect_types.clone(),
    );

    create_audit_chain_dirs(&args, crate_download_path)?;

    info!("Loading audit package lockfile");
    // If the lockfile doesn't exist, generate it
    let lockfile = chain.load_lockfile()?;

    let mut crate_path_buf = Path::new(&args.crate_path).canonicalize()?;
    let crate_data = load_cargo_toml(&crate_path_buf)?;

    let root_name = format!("{}-{}", crate_data.crate_name, crate_data.version);

    let config = GlobalContext::default()?;
    crate_path_buf.push("Cargo.toml");
    let workspace = Workspace::new(Path::new(&crate_path_buf), &config)?;
    let fetch_options = FetchOptions { gctx: &config, targets: Vec::new() };
    let (_resolve, package_set) = fetch(&workspace, &fetch_options)?;

    let crate_paths: HashMap<CrateId, PathBuf> =
        HashMap::from_iter(package_set.packages().map(|p| {
            let crate_id = CrateId::new(p.name().to_string(), p.version().clone());
            (crate_id, p.root().to_path_buf())
        }));

    info!("Creating dependency graph");
    let (graph, package_map, root_node) =
        make_dependency_graph(&lockfile.packages, &root_name);
    let mut traverse = DfsPostOrder::new(&graph, root_node);
    while let Some(node) = traverse.next(&graph) {
        let package = package_map.get(&node).unwrap();

        let audit_type = if node == root_node {
            DefaultAuditType::Empty
        } else {
            DefaultAuditType::CallerChecked
        };

        let crate_download_path = crate_paths
            .get(&CrateId::from(package))
            .context("Unresolved path for a crate")?;

        make_new_audit_file(
            &mut chain,
            package,
            &root_name,
            &args,
            crate_download_path,
            audit_type,
            &args.effect_types,
            quick_mode,
        )?;
    }

    info!("Finished creating audit chain");
    Ok(chain)
}

/// Collect all the sink calls that are propagated
/// from the dependencies to the top-level package.
pub fn collect_propagated_sinks(
    chain: &mut AuditChain,
) -> Result<HashMap<EffectInstance, Vec<(EffectInfo, String)>>> {
    let mut current_path: Vec<NodeIndex> = Vec::new();
    let mut effects = HashMap::new();

    let root_name = chain.root_crate()?;
    let lockfile = chain.load_lockfile()?;

    let (graph, package_map, root_node) =
        make_dependency_graph(&lockfile.packages, &root_name.to_string());
    let mut traverse = Dfs::new(&graph, root_node);
    while let Some(node) = traverse.next(&graph) {
        let package = package_map.get(&node).unwrap();
        let id = CrateId::new(package.name.to_string(), package.version.clone());
        let af = chain
            .read_audit_file(&id)?
            .context("Couldn't read audit file while collecting dependency sinks")?;

        if node == root_node {
            for (effect_instance, audit_tree) in &af.audit_trees {
                effects.insert(effect_instance.clone(), audit_tree.get_all_annotations());
            }
            continue;
        }

        current_path.push(node);
        check_sink_calls(af, &mut effects)?;

        // If we have already visited a current package's
        // dependency, revisit it now to check if any of
        // its public caller-checked functions are called
        // in the current package.
        for neighbor in graph.neighbors(node) {
            if current_path.contains(&neighbor) {
                let package = package_map.get(&neighbor).unwrap();
                let id = CrateId::new(package.name.to_string(), package.version.clone());
                let af = chain.read_audit_file(&id)?.context(
                    "Couldn't read audit file while collecting dependency sinks",
                )?;
                check_sink_calls(af, &mut effects)?;
            }
        }
    }

    Ok(effects)
}

fn check_sink_calls(
    af: AuditFile,
    effects: &mut HashMap<EffectInstance, Vec<(EffectInfo, String)>>,
) -> Result<()> {
    for (pub_cc_fn, base_effs) in af.pub_caller_checked {
        if effects.keys().any(|i| {
            *i.callee() == pub_cc_fn && i.caller().crate_name() != pub_cc_fn.crate_name()
        }) {
            for inst in base_effs {
                let tree = af
                    .audit_trees
                    .get(&inst)
                    .ok_or_else(|| anyhow!("couldn't find tree for effect instance"))?;

                effects.insert(inst.clone(), tree.get_all_annotations());
            }
        }
    }

    Ok(())
}

// Mirror of the above that returns HashSet of sinks
pub fn create_dependency_sinks(
    _args: Create,
    _crate_download_path: &str,
) -> Result<HashSet<IdentPath>> {
    todo!()
}

/// Gets the package that matches the name and version if one exists in the project.
fn lookup_package_from_name<I>(crate_id: &CrateId, project_packages: I) -> Result<Package>
where
    I: IntoIterator<Item = Package>,
{
    let name = cargo_lock::Name::from_str(&crate_id.crate_name)?;
    let version = cargo_lock::Version::parse(&format!("{}", crate_id.version))?;
    for p in project_packages {
        if p.name == name && p.version == version {
            return Ok(p);
        }
    }

    Err(anyhow!("Couldn't find package in workspace"))
}
