use std::{
    fs::{create_dir_all, File},
    path::PathBuf,
};

use anyhow::{anyhow, Error};
use home::home_dir;
use log::info;
use lsp_types::Location;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use cargo_scan::{
    audit_file::AuditFile,
    effect::{self, EffectInstance},
    scan_stats::{get_crate_stats_default, CrateStats},
    util::load_cargo_toml,
};

use crate::location::from_src_loc;

#[derive(Serialize, Deserialize, Debug)]
pub struct EffectsResponse {
    pub caller: String,
    pub callee: String,
    pub effect_type: String,
    pub location: Location,
}

impl EffectsResponse {
    pub fn new(effect: &EffectInstance) -> Result<Self, Error> {
        let location = from_src_loc(effect.call_loc())?;

        Ok(Self {
            caller: effect.caller().to_string(),
            callee: effect.callee().to_string(),
            effect_type: effect.eff_type().to_csv(),
            location,
        })
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ScanCommandResponse {
    effects: Vec<EffectsResponse>,
}

impl ScanCommandResponse {
    pub fn new(effs: &Vec<EffectInstance>) -> Result<Self, Error> {
        let mut effects = vec![];
        for e in effs {
            effects.push(EffectsResponse::new(e)?);
        }

        Ok(Self { effects })
    }

    pub fn to_json_value(&self) -> Result<Value, Error> {
        serde_json::to_value(self).map_err(|e| Error::new(e))
    }
}

/// Scan crate in root path and get crate stats
fn get_simple_scan_results(path: &PathBuf) -> CrateStats {
    let res = get_crate_stats_default(path.to_path_buf(), false);
    info!("Finished scanning. Found {} effects.", res.effects.len());

    res
}
pub fn scan_req(crate_path: &PathBuf) -> Result<Value, Error> {
    let stats = get_simple_scan_results(&crate_path);
    ScanCommandResponse::new(&stats.effects)?.to_json_value()
}

pub fn audit_req(path: &PathBuf) -> Result<(AuditFile, PathBuf), Error> {
    // The audit file path defaults to "~/.cargo_audits"
    let mut audit_file_path = home_dir().ok_or_else(||
        anyhow!("Error: couldn't find the home directory (required for default audit file path)"))?;

    audit_file_path.push(".cargo_audits");
    let crate_id = load_cargo_toml(path)?;
    audit_file_path.push(format!("{}.audit", crate_id));

    let audit_file = match AuditFile::read_audit_file(audit_file_path.clone())? {
        Some(p) => p,
        None => {
            // No audit file yet, so make a new one
            if let Some(parent_dir) = audit_file_path.parent() {
                create_dir_all(parent_dir)?;
            }
            File::create(audit_file_path.clone())?;

            let mut pf =
                AuditFile::empty(path.clone(), effect::DEFAULT_EFFECT_TYPES.to_vec())?;

            // Scan crate and set base effects to the audit file
            let effects = get_simple_scan_results(&path).effects;
            pf.set_base_audit_trees(effects.iter());
            pf.save_to_file(audit_file_path.clone())?;
            info!("Created new audit file `{}`", audit_file_path.display());

            pf
        }
    };

    Ok((audit_file, audit_file_path))
}
