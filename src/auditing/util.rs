use crate::audit_file::AuditFile;

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
    for entry in WalkDir::new(&p).sort_by_file_name() {
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

pub fn is_audit_scan_valid<P>(audit_file: &AuditFile, crate_path: P) -> Result<bool>
where
    P: AsRef<Path>,
{
    let hash = hash_dir(crate_path)?;
    // TODO: Better way to check hash
    Ok(audit_file.hash == hash)
}

/// The maximum size for an effect tree when creating a default caller-checked policy
pub const MAX_CALLER_CHECKED_TREE_SIZE: i32 = 10_000_000;

/// The maximum sum of sizes of effect trees when createing a default caller-checked policy
pub const MAX_AUDIT_FILE_SIZE: i32 = 20_000_000;
