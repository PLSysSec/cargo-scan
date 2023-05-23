use anyhow::{anyhow, Context, Result};
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
use std::path::PathBuf;
use toml;

use crate::download_crate;
use crate::ident::CanonicalPath;
use crate::policy::{DefaultPolicyType, PolicyFile};
use crate::util::{self, load_cargo_toml};

#[derive(Serialize, Deserialize, Debug)]
pub struct AuditChain {
    #[serde(skip)]
    manifest_path: PathBuf,
    crate_path: PathBuf,
    crate_policies: HashMap<String, PathBuf>,
}

impl AuditChain {
    pub fn new(manifest_path: PathBuf, crate_path: PathBuf) -> AuditChain {
        AuditChain { manifest_path, crate_path, crate_policies: HashMap::new() }
    }

    pub fn all_crates(&self) -> Vec<&String> {
        self.crate_policies.keys().collect::<Vec<_>>()
    }

    pub fn matching_crates_no_version<'a>(&'a self, crate_name: &str) -> Vec<&'a String> {
        self.crate_policies
            .keys()
            .filter(|x| x.starts_with(crate_name))
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

    pub fn add_crate_policy(&mut self, package: &Package, policy_loc: PathBuf) {
        let package_id = format!("{}-{}", package.name.as_str(), package.version);
        self.crate_policies.insert(package_id, policy_loc);
    }

    pub fn read_policy(&self, package: &str) -> Option<PolicyFile> {
        let policy_path = self.crate_policies.get(package)?;
        PolicyFile::read_policy(policy_path.clone()).ok()?
    }

    pub fn read_policy_no_version(
        &self,
        package: &str,
    ) -> Result<Vec<(String, PolicyFile)>> {
        let mut policies = Vec::new();
        for (full_name, crate_policy_path) in self.crate_policies.iter() {
            // trim the version number off the package and see if they match
            if full_name.starts_with(package) {
                let policy = PolicyFile::read_policy(crate_policy_path.clone())
                    .map_err(|_| anyhow!("Error reading policy for crate {}", full_name))?
                    .ok_or_else(|| {
                        anyhow!("Policy listed for crate {} is missing", full_name)
                    })?;
                policies.push((full_name.to_string(), policy));
            }
        }
        Ok(policies)
    }

    /// Loads the lockfile for the given crate path. Will generate a new one
    /// with the default configuration if none exists.
    pub fn load_lockfile(&self) -> Result<Lockfile> {
        let mut crate_path = self.crate_path.clone();
        crate_path.push("Cargo.lock");
        if let Ok(l) = Lockfile::load(&crate_path) {
            Ok(l)
        } else {
            println!("Lockfile missing: generating new lockfile");
            let config = config::Config::default()?;
            crate_path.pop();
            let workspace = Workspace::new(&crate_path, &config)?;
            generate_lockfile(&workspace)?;
            crate_path.push("Cargo.lock");
            let l = Lockfile::load(&crate_path)?;
            Ok(l)
        }
    }

    /// Removes all effects that originate from `removed_fns` for all parent
    /// crates of `updated_crate` in the AuditChain's dependency graph.
    pub fn remove_cross_crate_effects(
        &mut self,
        mut removed_fns: HashSet<CanonicalPath>,
        updated_crate: &str,
    ) -> Result<()> {
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
            let package_string = format!("{}-{}", package.name, package.version);
            let mut package_policy = self
                .read_policy(&package_string)
                .context(format!("Couldn't find policy for {}", package_string))?;
            let starting_pub_caller_checked = package_policy.pub_caller_checked.clone();

            package_policy.remove_sinks_from_tree(&removed_fns);
            let next_removed_fns = starting_pub_caller_checked
                .difference(&package_policy.pub_caller_checked)
                .cloned();
            removed_fns.extend(next_removed_fns);

            // reconstruct invariant
            package_policy.recalc_pub_caller_checked(&starting_pub_caller_checked);
            package_policy.save_to_file(
                self.crate_policies
                    .get(&package_string)
                    .context(format!("Missing crate {} from chain", &package_string))?
                    .clone(),
            )?;
        }

        Ok(())
    }

    /// Gets the root crate name with version
    pub fn root_crate(&self) -> Result<String> {
        let root_package = Manifest::from_path(format!(
            "{}/Cargo.toml",
            self.crate_path.to_string_lossy()
        ))?
        .package
        .ok_or_else(|| anyhow!("Can't load root package for the root crate path"))?;

        Ok(format!("{}-{}", root_package.name, root_package.version.get()?))
    }
}

// TODO: Add an argument for the default policy type
#[derive(Clone, ClapArgs, Debug)]
pub struct Create {
    /// Path to crate
    crate_path: String,
    /// Path to manifest
    manifest_path: String,

    // TODO: Check to make sure it meets the format (clap supports this?)
    /// Default policy folder
    #[clap(short = 'p', long = "policy-path", default_value = ".audit_policies")]
    policy_path: String,

    #[clap(short = 'f', long, default_value_t = false)]
    force_overwrite: bool,
}

impl Create {
    pub fn new(
        crate_path: String,
        manifest_path: String,
        policy_path: String,
        force_overwrite: bool,
    ) -> Self {
        Self { crate_path, manifest_path, policy_path, force_overwrite }
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
    chain: &AuditChain,
    deps: &Vec<Dependency>,
) -> Result<HashSet<CanonicalPath>> {
    let mut sinks = HashSet::new();
    for dep in deps {
        let dep_string = format!("{}-{}", dep.name, dep.version);
        let policy = chain.read_policy(&dep_string).context(
            "couldnt read dependency policy file (maybe created it out of order)",
        )?;
        sinks.extend(policy.pub_caller_checked.iter().cloned());
    }

    Ok(sinks)
}

// TODO: Different default policies
/// Creates a new default policy for the given package and returns the path to
/// the saved policy file
fn make_new_policy(
    chain: &AuditChain,
    package: &Package,
    root_name: &str,
    args: &Create,
    crate_download_path: &str,
    policy_type: DefaultPolicyType,
) -> Result<PathBuf> {
    let policy_path = PathBuf::from(format!(
        "{}/{}-{}.policy",
        args.policy_path,
        package.name.as_str(),
        package.version
    ));

    // download the new policy
    let package_path = if format!("{}-{}", package.name, package.version) == root_name {
        // We are creating a policy for the root crate
        PathBuf::from(args.crate_path.clone())
    } else {
        // TODO: Handle the case where we have a crate source not from crates.io
        download_crate::download_crate(package, crate_download_path)?
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
        PolicyFile::new_default_with_sinks(package_path.as_path(), sinks, policy_type)?;
    policy.save_to_file(policy_path.clone())?;

    Ok(policy_path)
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

    let crate_data = load_cargo_toml(&PathBuf::from(&args.crate_path))?;

    let root_name = format!("{}-{}", crate_data.name, crate_data.version);

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

        let res = make_new_policy(
            &chain,
            package,
            &root_name,
            &args,
            crate_download_path,
            policy_type,
        );
        match res {
            Ok(policy_path) => {
                chain.add_crate_policy(package, policy_path);
            }
            Err(e) => return Err(anyhow!("Audit chain creation failed: {}", e)),
        };
    }

    println!("Finished creating policy chain");
    Ok(chain)
}

/// Gets the package that matches the name and version if one exists in the project.
fn lookup_package_from_name<I>(full_name: &str, project_packages: I) -> Result<Package>
where
    I: IntoIterator<Item = Package>,
{
    let (name, version) = util::package_info_from_string(full_name)?;
    for p in project_packages {
        if p.name == name && p.version == version {
            return Ok(p);
        }
    }

    Err(anyhow!("Couldn't find package in workspace"))
}
