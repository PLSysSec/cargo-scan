use anyhow::{anyhow, Result};
use cargo_lock::Package;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::mem;
use std::path::PathBuf;
use toml;

use crate::policy::PolicyFile;

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
}
