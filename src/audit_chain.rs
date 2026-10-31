use anyhow::{anyhow, Context, Result};
use clap::Args as ClapArgs;
use log::info;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, remove_file, File};
use std::io::Write;
use std::mem;
use std::path::{Path, PathBuf};
use toml;

use crate::audit_file::{AuditFile, AuditVersion, DefaultAuditType, EffectTree};
use crate::cargo_workspace::{self, DepGraph, PackageInfo};
use crate::effect::{EffectInstance, EffectType, DEFAULT_EFFECT_TYPES};
use crate::ident::{replace_hyphens, CanonicalPath};
use crate::util::CrateId;

pub use crate::cargo_workspace::ResolveOptions;

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
        package: &PackageInfo,
        audit_file_loc: PathBuf,
        version: AuditVersion,
    ) {
        self.crate_policies.insert(chain_key(package), (audit_file_loc, version));
    }

    pub fn read_audit_file(&mut self, crate_id: &CrateId) -> Result<Option<AuditFile>> {
        let (audit_file_path, expected_version) = self
            .crate_policies
            .get(crate_id)
            .with_context(|| "Can't find an associated audit for the crate")?
            .clone();
        match AuditFile::read_audit_file(audit_file_path.clone())? {
            Some(audit_file) => {
                if audit_file.version != expected_version {
                    // Update version in chain manifest, so we don't loop infinitely
                    self.crate_policies
                        .get_mut(crate_id)
                        .with_context(|| "Couldn't find the crate in the chain manifest")?
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
            let audit_file =
                AuditFile::read_audit_file(af_path.clone())?.with_context(|| {
                    format!("Can't find an associated audit for crate `{}`", crate_id)
                })?;
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
        for crate_id in self.crate_policies.keys() {
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

    // TODO: Write a test for this to make sure it's properly recalculating
    //       dependency policies when they are invalid. It's going to be almost
    //       impossible to tell if something has gone wrong here.
    /// Removes all effects that originate from `removed_fns` for all parent
    /// crates of `updated_crate` in the AuditChain's dependency graph.
    /// `updated_crate should the full crate name with version`. Returns the
    /// set of removed functions if it succeeds.
    pub fn remove_cross_crate_effects(
        &mut self,
        removed_fns: HashSet<CanonicalPath>,
        updated_crate: &CrateId,
    ) -> Result<HashSet<CanonicalPath>> {
        let graph = chain_graph(&self.crate_path, &ResolveOptions::default())?;
        self.remove_cross_crate_effects_with_graph(removed_fns, updated_crate, &graph)
    }

    fn remove_cross_crate_effects_with_graph(
        &mut self,
        mut removed_fns: HashSet<CanonicalPath>,
        updated_crate: &CrateId,
        graph: &DepGraph,
    ) -> Result<HashSet<CanonicalPath>> {
        let updated = graph
            .packages()
            .find(|&p| chain_key(graph.package(p)) == *updated_crate)
            .ok_or_else(|| anyhow!("No package {} in dependency graph", updated_crate))?;

        let audited: HashSet<CrateId> = self.crate_policies.keys().cloned().collect();
        // Visit the updated crate and its dependents, dependencies first, so we
        // don't have to recursively add packages whose public caller-checked
        // functions have been updated.
        for p in graph.transitive_dependents(updated) {
            let crate_id = chain_key(graph.package(p));
            if !audited.contains(&crate_id) {
                continue;
            }
            let mut crate_audit_file = self
                .read_audit_file(&crate_id)?
                .with_context(|| format!("Couldn't find audit for {}", crate_id))?;
            let starting_pub_caller_checked = crate_audit_file
                .pub_caller_checked
                .keys()
                .cloned()
                .collect::<HashSet<_>>();

            let removed_effect_instances =
                crate_audit_file.remove_sinks_from_tree(&removed_fns);
            // reconstruct invariant before comparing: removing effects only
            // changes the trees, not the public caller-checked functions
            crate_audit_file.recalc_pub_caller_checked(&starting_pub_caller_checked);
            let package_pub_fns = crate_audit_file
                .pub_caller_checked
                .keys()
                .cloned()
                .collect::<HashSet<_>>();
            let next_removed_fns = starting_pub_caller_checked
                .difference(&package_pub_fns)
                .cloned()
                .collect::<Vec<_>>();

            if !next_removed_fns.is_empty() || !removed_effect_instances.is_empty() {
                // If the audit file changes, we need to bump the version so
                // other audit chains know to recalculate their effects
                crate_audit_file.version += 1;
            }
            removed_fns.extend(next_removed_fns);

            crate_audit_file.save_to_file(
                self.crate_policies
                    .get(&crate_id)
                    .context(format!("Missing crate {} from chain", crate_id))?
                    .0
                    .clone(),
            )?;

            self.crate_policies
                .get_mut(&crate_id)
                .with_context(|| "Couldn't find the crate in the chain manifest")?
                .1 = crate_audit_file.version;
        }

        Ok(removed_fns)
    }

    /// Gets the root crate id
    pub fn root_crate(&self) -> Result<CrateId> {
        Ok(cargo_workspace::load_package(&self.crate_path)?.id)
    }
}

/// Key of a package in [`AuditChain::crate_policies`]
fn chain_key(package: &PackageInfo) -> CrateId {
    let v = &package.id.version;
    CrateId::new(package.id.crate_name.clone(), Version::new(v.major, v.minor, v.patch))
}

/// Resolve the dependency graph that chain is built on
fn chain_graph(crate_path: &Path, opts: &ResolveOptions) -> Result<DepGraph> {
    // TODO: decide whether chains should conditionally include dev-dependencies;
    // ignore them for now, even if they are set as true
    let opts = ResolveOptions { include_dev_deps: false, ..opts.clone() };
    DepGraph::resolve(crate_path, &opts)
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

    /// The type of audit file that will be created for the top-level crate.
    #[clap(short = 't', long, default_value = "empty")]
    pub root_audit_type: DefaultAuditType,

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

    /// Config options for Cargo's workspace resolution.
    #[clap(flatten)]
    #[serde(flatten)]
    pub resolve: ResolveOptions,
}

impl Create {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        crate_path: String,
        manifest_path: String,
        audit_file_path: String,
        force_overwrite: bool,
        download_root_crate: Option<String>,
        download_version: Option<String>,
        effect_types: Vec<EffectType>,
        root_audit_type: DefaultAuditType,
        resolve_opts: ResolveOptions,
    ) -> Self {
        Self {
            crate_path,
            manifest_path,
            audit_path: audit_file_path,
            force_overwrite,
            download_root_crate,
            download_version,
            effect_types,
            root_audit_type,
            resolve: resolve_opts,
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
            root_audit_type: DefaultAuditType::Empty,
            resolve: ResolveOptions::default(),
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

fn collect_dependency_sinks(
    chain: &mut AuditChain,
    deps: &[CrateId],
) -> Result<HashSet<CanonicalPath>> {
    let mut sinks = HashSet::new();
    for dep_id in deps {
        let audit_file = chain.read_audit_file(dep_id)?.with_context(|| {
            "couldnt read dependency audit file (maybe created it out of order)"
        })?;
        sinks.extend(audit_file.pub_caller_checked());
    }

    Ok(sinks)
}

/// Creates a new default audit file for the given package and returns the path to
/// the saved audit file
fn make_new_audit_file(
    chain: &mut AuditChain,
    package: &PackageInfo,
    config: &AuditConfig<'_>,
    audit_type: DefaultAuditType,
    dependencies: &[CrateId],
) -> Result<()> {
    let PackageInfo { id: CrateId { crate_name, version }, dir: crate_path, .. } =
        package;
    let audit_file_path = PathBuf::from(format!(
        "{}/{}-{}.audit",
        config.args.audit_path, crate_name, version
    ));
    let package_path = cargo_workspace::normalize_path(crate_path)?;

    if audit_file_path.is_dir() {
        return Err(anyhow!("Audit path is a directory"));
    }
    if audit_file_path.is_file() {
        if config.args.force_overwrite {
            remove_file(audit_file_path.clone())?;
        } else {
            info!(
                "Using existing audit for {} v{} ({}) -- expanding macros: {}",
                crate_name,
                version,
                audit_file_path.display(),
                config.expand_macro,
            );
            let mut audit_file = AuditFile::read_audit_file(audit_file_path.clone())?
                .ok_or_else(|| {
                    anyhow!("Couldn't read audit: {}", audit_file_path.display())
                })?;

            let sinks = collect_dependency_sinks(chain, dependencies)?;
            audit_file = audit_file.update_audit_file(
                crate_path,
                sinks,
                audit_type,
                &config.args.effect_types,
                config.quick_mode,
                config.expand_macro,
            )?;

            audit_file.save_to_file(audit_file_path.clone())?;
            chain.add_crate_audit_file(package, audit_file_path, audit_file.version);

            return Ok(());
        }
    }

    info!("Making new {:?} default audit for {} v{}", audit_type, crate_name, version);

    let sinks = collect_dependency_sinks(chain, dependencies)?;
    let audit_file = AuditFile::new_default_with_sinks(
        &package_path,
        sinks,
        audit_type,
        &config.args.effect_types,
        config.quick_mode,
        config.expand_macro,
    )?;
    audit_file.save_to_file(audit_file_path.clone())?;

    chain.add_crate_audit_file(package, audit_file_path, audit_file.version);

    Ok(())
}

struct AuditConfig<'a> {
    args: &'a Create,
    quick_mode: bool,
    expand_macro: bool,
}

pub fn create_new_audit_chain(
    args: Create,
    crate_download_path: &str,
    quick_mode: bool,
    expand_macro: bool,
) -> Result<AuditChain> {
    info!("Creating audit chain");
    let mut chain = AuditChain::new(
        PathBuf::from(&args.manifest_path),
        PathBuf::from(&args.crate_path),
        args.effect_types.clone(),
    );

    create_audit_chain_dirs(&args, crate_download_path)?;

    let graph = chain_graph(Path::new(&args.crate_path), &args.resolve)?;
    let root = graph.root().ok_or_else(|| {
        anyhow!(
            "{} is a virtual workspace; a chain needs a root package",
            args.crate_path
        )
    })?;

    let config = AuditConfig { args: &args, quick_mode, expand_macro };
    // Dependencies first, so their audits exist when their dependents' audits
    // collect sinks from them
    for pkg in graph.transitive_dependencies(root) {
        let audit_type = if pkg == root {
            config.args.root_audit_type
        } else {
            DefaultAuditType::CallerChecked
        };
        let dependencies: Vec<CrateId> =
            graph.direct_dependencies(pkg).map(|d| chain_key(graph.package(d))).collect();
        make_new_audit_file(
            &mut chain,
            graph.package(pkg),
            &config,
            audit_type,
            &dependencies,
        )?;
    }

    info!("Finished creating audit chain");
    Ok(chain)
}

/// Ranking info for a dependency crate:
/// 1. how many effects transitively propagate to the root
/// 2. its depth in the dependency graph
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DepRank {
    pub crate_name: String,
    pub depth: usize,
    /// The crate's own base effects
    /// that propagate to the root.
    pub base_effects: usize,
    /// Cross-crate calls from parent crates into
    /// the crate's public caller-checked functions.
    /// Auditing this crate first removes them from parent audits.
    pub propagated_effects: usize,
}

/// Collect all the sink calls that are propagated
/// from the dependencies to the top-level package.
pub fn collect_propagated_sinks(
    chain: &mut AuditChain,
) -> Result<HashMap<EffectInstance, EffectTree>> {
    let graph = chain_graph(&chain.crate_path, &ResolveOptions::default())?;
    collect_propagated_sinks_with_graph(chain, &graph)
}

/// The chain's packages, dependents before dependencies (i.e., root is first)
fn chain_order(graph: &DepGraph) -> Result<Vec<&PackageInfo>> {
    let root =
        graph.root().ok_or_else(|| anyhow!("Chain root is a virtual workspace"))?;
    let mut order = graph.transitive_dependencies(root);
    order.reverse();
    Ok(order.into_iter().map(|p| graph.package(p)).collect())
}

fn collect_propagated_sinks_with_graph(
    chain: &mut AuditChain,
    graph: &DepGraph,
) -> Result<HashMap<EffectInstance, EffectTree>> {
    let mut effects = HashMap::new();
    let root_name = chain.root_crate()?;

    let all_crates: HashSet<CrateId> = chain.crate_policies.keys().cloned().collect();

    for pkg in chain_order(graph)? {
        let id = chain_key(pkg);
        if !all_crates.contains(&id) {
            continue;
        }
        let af = chain.read_audit_file(&id)?.with_context(|| {
            "Couldn't read audit file while collecting dependency sinks"
        })?;

        if id == root_name {
            for (effect_instance, audit_tree) in &af.audit_trees {
                effects.insert(effect_instance.clone(), audit_tree.clone());
            }
            continue;
        }

        check_sink_calls(af, &mut effects)?;
    }

    Ok(effects)
}

/// Same as `collect_propagated_sinks` but also computes per-dependency rankings
pub fn collect_propagated_sinks_ranked(
    chain: &mut AuditChain,
) -> Result<(HashMap<EffectInstance, EffectTree>, Vec<DepRank>)> {
    let graph = chain_graph(&chain.crate_path, &ResolveOptions::default())?;
    collect_propagated_sinks_ranked_with_graph(chain, &graph)
}

fn collect_propagated_sinks_ranked_with_graph(
    chain: &mut AuditChain,
    graph: &DepGraph,
) -> Result<(HashMap<EffectInstance, EffectTree>, Vec<DepRank>)> {
    let mut topo_index: HashMap<String, usize> = HashMap::new();
    for (idx, pkg) in chain_order(graph)?.into_iter().rev().enumerate() {
        for target in &pkg.targets {
            topo_index.insert(target.crate_name.clone(), idx);
        }
    }

    let effects = collect_propagated_sinks_with_graph(chain, graph)?;
    let rankings = compute_dep_rankings(&effects, &topo_index);
    Ok((effects, rankings))
}

fn compute_dep_rankings(
    effects: &HashMap<EffectInstance, EffectTree>,
    topo_index: &HashMap<String, usize>,
) -> Vec<DepRank> {
    let dep_crates: HashSet<String> =
        effects.keys().map(|i| i.caller().crate_name().to_string()).collect();

    dep_crates
        .into_iter()
        .map(|dep| {
            let own = effects
                .keys()
                .filter(|i| i.caller().crate_name().to_string() == dep)
                .count();
            let incoming = effects
                .keys()
                .filter(|i| {
                    i.callee().crate_name().to_string() == dep
                        && i.caller().crate_name().to_string() != dep
                })
                .count();
            let topo_idx = topo_index.get(&dep).copied().unwrap_or(usize::MAX);
            DepRank {
                crate_name: dep,
                depth: topo_idx,
                base_effects: own,
                propagated_effects: incoming,
            }
        })
        .collect()
}

fn check_sink_calls(
    af: AuditFile,
    effects: &mut HashMap<EffectInstance, EffectTree>,
) -> Result<()> {
    for (pub_cc_fn, base_effs) in af.pub_caller_checked {
        if effects.keys().any(|i| {
            *i.callee() == pub_cc_fn && i.caller().crate_name() != pub_cc_fn.crate_name()
        }) {
            for inst in base_effs {
                let tree = af.audit_trees.get(&inst).ok_or_else(|| {
                    anyhow!("couldn't find tree for effect instance: {:?}", inst)
                })?;
                effects.insert(inst.clone(), tree.clone());
            }
        }
    }

    Ok(())
}
