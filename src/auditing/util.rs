use crate::effect::EffectBlock;
use crate::policy::PolicyFile;

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

pub fn hash_dir<P>(p: P) -> Result<[u8; 32]>
where
    P: AsRef<Path>,
{
    let mut hasher = Sha256::new();
    for entry in WalkDir::new(&p) {
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

pub fn is_policy_scan_valid<P>(
    policy: &PolicyFile,
    scan_effect_blocks: &HashSet<&EffectBlock>,
    crate_path: P,
) -> Result<bool>
where
    P: AsRef<Path>,
{
    let policy_effect_blocks = policy.audit_trees.keys().collect::<HashSet<_>>();
    let hash = hash_dir(crate_path)?;
    // NOTE: We're checking the hash in addition to the effect blocks for now
    //       because we might have changed how we scan packages for effects.
    Ok(policy_effect_blocks == *scan_effect_blocks && policy.hash == hash)
}
