use super::effect::{EffectBlock, EffectInstance, SrcLoc};
use super::ident::Path;

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum SafetyAnnotation {
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

pub fn hash_dir(p: PathBuf) -> Result<[u8; 32]> {
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
pub struct EffectInfo {
    pub caller_path: Path,
    pub callee_loc: SrcLoc,
    // TODO: callee_src_span: SrcSpan,
}

impl EffectInfo {
    pub fn new(caller_path: Path, callee_loc: SrcLoc) -> Self {
        EffectInfo { caller_path, callee_loc }
    }

    pub fn from_instance(effect: &EffectInstance) -> Self {
        let caller_src_path = effect.caller().clone().to_path();
        let callee_loc = effect.call_loc().clone();

        EffectInfo::new(caller_src_path, callee_loc)
    }

    pub fn from_block(effect: &EffectBlock) -> Self {
        EffectInfo::new(
            effect.containing_fn().fn_name.clone().to_path(),
            effect.src_loc().clone(),
        )
    }
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum EffectTree {
    Leaf(EffectInfo, SafetyAnnotation),
    Branch(EffectInfo, Vec<EffectTree>),
}

impl EffectTree {
    pub fn get_leaf_annotation(&self) -> Option<SafetyAnnotation> {
        match self {
            EffectTree::Leaf(_, a) => Some(*a),
            EffectTree::Branch(_, _) => None,
        }
    }

    /// Sets the annotation for a leaf node and returns Some previous annotation,
    /// or None if it was a branch node
    pub fn set_annotation(
        &mut self,
        new_a: SafetyAnnotation,
    ) -> Option<SafetyAnnotation> {
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
pub struct PolicyFile {
    // TODO: Serde doesn't like this hashmap for some reason (?)
    #[serde_as(as = "Vec<(_, _)>")]
    pub audit_trees: HashMap<EffectBlock, EffectTree>,
    // TODO: Make the base_dir a crate instead
    pub base_dir: PathBuf,
    pub hash: [u8; 32],
}

impl PolicyFile {
    pub fn new(p: PathBuf) -> Result<Self> {
        let hash = hash_dir(p.clone())?;
        Ok(PolicyFile { audit_trees: HashMap::new(), base_dir: p, hash })
    }

    pub fn save_to_file(&self, p: PathBuf) -> Result<()> {
        let json = serde_json::to_string(self)?;
        let mut f = File::create(p)?;
        f.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Returns Some policy file if it exists, or None if we should create a new one.
    /// Errors if the policy filepath is invalid or if we can't read an existing
    /// policy file
    pub fn read_policy(policy_filepath: PathBuf) -> Result<Option<PolicyFile>> {
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
}
