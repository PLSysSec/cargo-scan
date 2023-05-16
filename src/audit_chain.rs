use anyhow::{anyhow, Context, Result};
use cargo::{core::Workspace, ops::generate_lockfile, util::config};
use cargo_lock::{Dependency, Lockfile, Package};
use petgraph::visit::DfsPostOrder;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::iter::IntoIterator;
use std::mem;
use std::path::PathBuf;
use toml;

use crate::ident::CanonicalPath;
use crate::policy::PolicyFile;
use crate::util;

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
