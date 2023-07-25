use crate::policy::PolicyFile;

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

pub fn is_policy_scan_valid<P>(policy: &PolicyFile, crate_path: P) -> Result<bool>
where
    P: AsRef<Path>,
{
    let hash = hash_dir(crate_path)?;
    // TODO: Better way to check hash
    Ok(policy.hash == hash)
}
