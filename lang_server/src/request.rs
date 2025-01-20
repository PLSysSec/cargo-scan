use std::{
    collections::HashMap,
    fs::{create_dir_all, File},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Error};
use home::home_dir;
use log::info;
use lsp_types::Location;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use cargo_scan::{
    audit_file::{AuditFile, EffectInfo, EffectTree},
    effect::{self, EffectInstance},
    scan_stats::{get_crate_stats_default, CrateStats},
    util::load_cargo_toml,
};
use serde_with::serde_as;

use crate::location::from_src_loc;

#[derive(Serialize, Deserialize, Eq, PartialEq, Hash, Debug, Clone)]
pub struct EffectsResponse {
    pub caller: String,
    pub callee: String,
    pub effect_type: String,
    pub location: Location,
    pub crate_name: String,
}

impl EffectsResponse {
    pub fn new(effect: &EffectInstance) -> Result<Self, Error> {
        let crate_name = effect.caller().crate_name().to_string();
        let location = from_src_loc(effect.call_loc())?;

        Ok(Self {
            caller: effect.caller().to_string(),
            callee: effect.callee().to_string(),
            effect_type: effect.eff_type().to_csv(),
            location,
            crate_name,
        })
    }

    pub fn from_effect_info(
        eff_info: &EffectInfo,
        callee: String,
        effect_type: String,
    ) -> Result<Self, Error> {
        let location = from_src_loc(&eff_info.callee_loc)?;
        let crate_name = eff_info.caller_path.crate_name().to_string();

        Ok(Self {
            caller: eff_info.caller_path.to_string(),
            callee,
            effect_type,
            location,
            crate_name,
        })
    }

    pub fn get_caller(&self) -> String {
        self.caller.to_owned()
    }

    pub fn get_callee(&self) -> String {
        self.callee.to_owned()
    }

    pub fn get_effect_type(&self) -> String {
        self.effect_type.to_owned()
    }

    pub fn from_json_value(e: Value) -> Result<Self, Error> {
        serde_json::from_value(e).map_err(Error::new)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ScanCommandResponse {
    effects: Vec<EffectsResponse>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug)]
pub struct AuditCommandResponse {
    #[serde_as(as = "Vec<(_, _)>")]
    effects: HashMap<EffectsResponse, Vec<(EffectsResponse, String)>>,
}

impl AuditCommandResponse {
    pub fn new(
        effs: &HashMap<EffectInstance, Vec<(EffectInfo, String)>>,
    ) -> Result<Self, Error> {
        let mut effects = HashMap::new();

        for (inst, anns) in effs.iter() {
            let mut callers = vec![];
            for (i, a) in anns {
                let callee = inst.callee().to_string();
                let eff_type = inst.eff_type().to_csv();
                let resp = EffectsResponse::from_effect_info(i, callee, eff_type)?;
                callers.push((resp, a.to_owned()));
            }
            let base_effect = EffectsResponse::new(inst)?;
            effects.insert(base_effect, callers);
        }

        Ok(Self { effects })
    }

    pub fn to_json_value(&self) -> Result<Value, Error> {
        serde_json::to_value(self).map_err(Error::new)
    }
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
        serde_json::to_value(self).map_err(Error::new)
    }
}

/// Scan crate in root path and get crate stats
fn get_simple_scan_results(path: &Path) -> CrateStats {
    let res = get_crate_stats_default(path.to_path_buf(), false, false);
    info!("Finished scanning. Found {} effects.", res.effects.len());

    res
}
pub fn scan_req(crate_path: &Path) -> Result<Value, Error> {
    let stats = get_simple_scan_results(crate_path);
    ScanCommandResponse::new(&stats.effects)?.to_json_value()
}

pub fn audit_req(path: &Path) -> Result<(AuditFile, PathBuf), Error> {
    // The audit file path defaults to "~/.cargo_audits"
    let mut audit_file_path = home_dir().ok_or_else(||
        anyhow!("Error: couldn't find the home directory (required for default audit file path)"))?;

    audit_file_path.push(".cargo_audits");
    let crate_id = load_cargo_toml(path)?;
    audit_file_path.push(format!("{}.audit", crate_id));

    let audit_file = match AuditFile::read_audit_file(audit_file_path.clone())? {
        Some(p) => {
            info!("Loaded audit file `{}`", audit_file_path.display());
            p
        }
        None => {
            // No audit file yet, so make a new one
            if let Some(parent_dir) = audit_file_path.parent() {
                create_dir_all(parent_dir)?;
            }
            File::create(audit_file_path.clone())?;

            let mut pf = AuditFile::empty(
                path.to_path_buf(),
                effect::DEFAULT_EFFECT_TYPES.to_vec(),
            )?;

            // Scan crate and set base effects to the audit file
            let effects = get_simple_scan_results(path).effects;
            pf.set_base_audit_trees(effects.iter());
            pf.save_to_file(audit_file_path.clone())?;
            info!("Created new audit file `{}`", audit_file_path.display());

            pf
        }
    };

    Ok((audit_file, audit_file_path))
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CallerCheckedResponse {
    pub effects: Vec<EffectsResponse>,
}

impl CallerCheckedResponse {
    pub fn new(
        effect: &EffectsResponse,
        new_audit_locs: &[EffectTree],
    ) -> Result<Self, Error> {
        let mut effects = vec![];

        for tree in new_audit_locs.iter() {
            for eff_info in tree.get_effect_infos().iter() {
                let callee = effect.get_callee();
                let effect_type = effect.get_effect_type();
                let caller =
                    EffectsResponse::from_effect_info(eff_info, callee, effect_type)?;
                effects.push(caller);
            }
        }

        Ok(Self { effects })
    }

    pub fn to_json_value(&self) -> Result<Value, Error> {
        serde_json::to_value(self).map_err(Error::new)
    }
}
