use anyhow::{anyhow, Context, Result};
use cargo::core::source::MaybePackage;
use cargo::ops::{fetch, FetchOptions};
use cargo::{core::Workspace, ops::generate_lockfile, util::config};
use cargo_lock::{Dependency, Lockfile, Package};
use cargo_toml::Manifest;
use clap::Args as ClapArgs;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::DfsPostOrder;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{create_dir_all, remove_file, File};
use std::io::Write;
use std::iter::IntoIterator;
use std::mem;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use toml;

use crate::ident::{CanonicalPath, IdentPath};
use crate::policy::{DefaultPolicyType, PolicyFile, PolicyVersion};
use crate::util::{load_cargo_toml, CrateId};

#[derive(Serialize, Deserialize, Debug)]
pub struct AuditChain {
    #[serde(skip)]
    manifest_path: PathBuf,
    crate_path: PathBuf,
    crate_policies: HashMap<CrateId, (PathBuf, PolicyVersion)>,
}

impl AuditChain {
    pub fn new(manifest_path: PathBuf, crate_path: PathBuf) -> AuditChain {
        AuditChain { manifest_path, crate_path, crate_policies: HashMap::new() }
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

    pub fn add_crate_policy(
        &mut self,
        package: &Package,
        policy_loc: PathBuf,
        version: PolicyVersion,
    ) {
        let crate_id = CrateId::from(package);
        self.crate_policies.insert(crate_id, (policy_loc, version));
    }

    pub fn read_policy(&mut self, crate_id: &CrateId) -> Result<Option<PolicyFile>> {
        let (policy_path, expected_version) = self
            .crate_policies
            .get(crate_id)
            .context("Can't find an associated policy for the crate")?
            .clone();
        match PolicyFile::read_policy(policy_path.clone())? {
            Some(policy) => {
                if policy.version != expected_version {
                    // The policy has been updated in a different audit, so we need to
                    // recalculate the policies for its parents
                    let potentially_removed = policy.safe_pub_fns();
                    self.remove_cross_crate_effects(potentially_removed, crate_id)?;

                    // re-read the policy so changes have taken effect
                    // NOTE: This assumes there aren't concurrent audits modifying policies
                    PolicyFile::read_policy(policy_path)
                } else {
                    Ok(Some(policy))
                }
            }
            None => Ok(None),
        }
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
            if crate_id.crate_name == search_name {
                res.push(crate_id.clone());
            }
        }
        res
    }

    pub fn read_policy_no_version(
        &mut self,
        crate_name: &str,
    ) -> Result<Option<(CrateId, PolicyFile)>> {
        if let Some(crate_id) = self.resolve_crate_id(crate_name) {
            if let Some(policy) = self.read_policy(&crate_id)? {
                return Ok(Some((crate_id, policy)));
            }
        }
        Ok(None)
    }

    /// Looks up where the policy is saved from the full crate name and saves the
    /// given PolicyFile to the PathBuf associated with that crate.
    pub fn save_policy(&mut self, crate_id: &CrateId, policy: &PolicyFile) -> Result<()> {
        let (policy_path, policy_version) = self
            .crate_policies
            .get_mut(crate_id)
            .ok_or_else(|| anyhow!("Couldn't find entry for crate: {}", crate_id))?;
        *policy_version = policy.version;
        policy.save_to_file(policy_path.clone())
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
            println!("Lockfile missing: generating new lockfile");
            let config = config::Config::default()?;
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
            let mut crate_policy = self
                .read_policy(&crate_id)?
                .context(format!("Couldn't find policy for {}", crate_id))?;
            let starting_pub_caller_checked =
                crate_policy.pub_caller_checked.keys().cloned().collect::<HashSet<_>>();

            let removed_effect_instances =
                crate_policy.remove_sinks_from_tree(&removed_fns);
            let package_pub_fns =
                &crate_policy.pub_caller_checked.keys().cloned().collect::<HashSet<_>>();
            let next_removed_fns = starting_pub_caller_checked
                .difference(package_pub_fns)
                .cloned()
                .collect::<Vec<_>>();
            if !next_removed_fns.is_empty() || !removed_effect_instances.is_empty() {
                // If the policy file changes, we need to bump the version so
                // other audit chains know to recalculate their effects
                crate_policy.version += 1;
            }
            removed_fns.extend(next_removed_fns);

            // reconstruct invariant
            crate_policy.recalc_pub_caller_checked(&starting_pub_caller_checked);
            crate_policy.save_to_file(
                self.crate_policies
                    .get(&crate_id)
                    .context(format!("Missing crate {} from chain", &crate_id))?
                    .0
                    .clone(),
            )?;

            self.crate_policies
                .get_mut(&crate_id)
                .context("Couldn't find the crate in the chain manifest")?
                .1 = crate_policy.version;
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

#[derive(Clone, ClapArgs, Debug)]
pub struct Create {
    /// Path to crate
    pub crate_path: String,
    /// Path to manifest
    pub manifest_path: String,

    // TODO: Check to make sure it meets the format (clap supports this?)
    /// Default policy folder
    #[clap(short = 'p', long = "policy-path", default_value = ".audit_policies")]
    pub policy_path: String,

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
}

impl Create {
    pub fn new(
        crate_path: String,
        manifest_path: String,
        policy_path: String,
        force_overwrite: bool,
        download_root_crate: Option<String>,
        download_version: Option<String>,
    ) -> Self {
        Self {
            crate_path,
            manifest_path,
            policy_path,
            force_overwrite,
            download_root_crate,
            download_version,
        }
    }
}

fn create_audit_chain_dirs(args: &Create, crate_download_path: &str) -> Result<()> {
    let mut manifest_path = PathBuf::from(&args.manifest_path);
    manifest_path.pop();
    create_dir_all(manifest_path)?;

    let crate_download_path = PathBuf::from(crate_download_path);
    create_dir_all(crate_download_path)?;

    let policy_path = PathBuf::from(&args.policy_path);
    create_dir_all(policy_path)?;

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
        let policy = chain.read_policy(&dep_id)?.context(
            "couldnt read dependency policy file (maybe created it out of order)",
        )?;
        sinks.extend(policy.pub_caller_checked.keys().cloned());
    }

    Ok(sinks)
}

/// Creates a new default policy for the given package and returns the path to
/// the saved policy file
fn make_new_policy(
    chain: &mut AuditChain,
    package: &Package,
    root_name: &str,
    args: &Create,
    crate_path: &Path,
    policy_type: DefaultPolicyType,
) -> Result<()> {
    let policy_path = PathBuf::from(format!(
        "{}/{}-{}.policy",
        args.policy_path,
        package.name.as_str(),
        package.version
    ));
    // download the new policy
    let full_name = format!("{}-{}", package.name, package.version);
    let package_path = if full_name == root_name {
        // We are creating a policy for the root crate
        PathBuf::from(args.crate_path.clone()).canonicalize()?
    } else {
        PathBuf::from(crate_path).canonicalize()?
    };

    // Try to create a new default policy
    if policy_path.is_dir() {
        return Err(anyhow!("Policy path is a directory"));
    }
    if policy_path.is_file() {
        if args.force_overwrite {
            remove_file(policy_path.clone())?;
        } else {
            return Err(anyhow!("Policy file already exists"));
        }
    }

    let sinks = collect_dependency_sinks(chain, &package.dependencies)?;
    let policy =
        PolicyFile::new_default_with_sinks(&package_path, sinks, policy_type)?;
    policy.save_to_file(policy_path.clone())?;

    chain.add_crate_policy(package, policy_path, policy.version);

    Ok(())
}

pub fn create_new_audit_chain(
    args: Create,
    crate_download_path: &str,
) -> Result<AuditChain> {
    println!("Creating audit chain");
    let mut chain = AuditChain::new(
        PathBuf::from(&args.manifest_path),
        PathBuf::from(&args.crate_path),
    );

    create_audit_chain_dirs(&args, crate_download_path)?;

    println!("Loading audit package lockfile");
    // If the lockfile doesn't exist, generate it
    let lockfile = chain.load_lockfile()?;

    let mut crate_path_buf = Path::new(&args.crate_path).canonicalize()?;
    let crate_data = load_cargo_toml(&crate_path_buf)?;

    let root_name = format!("{}-{}", crate_data.crate_name, crate_data.version);

    let config = config::Config::default()?;
    let _lock = config.acquire_package_cache_lock();
    let set = HashSet::new();
    crate_path_buf.push("Cargo.toml");
    let workspace = Workspace::new(Path::new(&crate_path_buf), &config)?;
    let fetch_options = FetchOptions { config: &config, targets: Vec::new() };
    let (resolve, _package_set) = fetch(&workspace, &fetch_options)?;
    let crate_paths: HashMap<CrateId, PathBuf> =
        HashMap::from_iter(resolve.iter().filter_map(|p| {
            // NOTE: We should return Some for every element here
            let source_id = p.source_id();
            let Ok(mut source) = source_id.load(&config, &set) else {
                return None;
            };
            match source.download(p) {
                Ok(MaybePackage::Ready(pkg)) => {
                    let crate_id =
                        CrateId::new(p.name().to_string(), p.version().clone());
                    Some((crate_id, pkg.root().to_path_buf()))
                }
                _ => None,
            }
        }));

    println!("Creating dependency graph");
    let (graph, package_map, root_node) =
        make_dependency_graph(&lockfile.packages, &root_name);
    let mut traverse = DfsPostOrder::new(&graph, root_node);
    while let Some(node) = traverse.next(&graph) {
        let package = package_map.get(&node).unwrap();
        println!("Making default policy for {} v{}", package.name, package.version);

        let policy_type = if node == root_node {
            DefaultPolicyType::Empty
        } else {
            DefaultPolicyType::CallerChecked
        };

        let crate_download_path = crate_paths
            .get(&CrateId::from(package))
            .context("Unresolved path for a crate")?;

        make_new_policy(
            &mut chain,
            package,
            &root_name,
            &args,
            crate_download_path,
            policy_type,
        )?;
    }

    println!("Finished creating policy chain");
    Ok(chain)
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
