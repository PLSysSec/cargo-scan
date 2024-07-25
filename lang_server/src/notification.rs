use std::path::{Path, PathBuf};

use anyhow::{Context, Error};
use cargo_scan::{
    audit_chain::AuditChain,
    audit_file::AuditFile,
    ident::CanonicalPath,
    scanner::{scan_crate, ScanResults},
};
use lsp_types::notification::Notification;
use serde::{Deserialize, Serialize};

use crate::{
    location::convert_annotation, request::EffectsResponse, util::find_effect_instance,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct AuditNotificationParams {
    pub safety_annotation: String,
    pub effect: EffectsResponse,
}

pub struct AuditNotification;

impl Notification for AuditNotification {
    type Params = AuditNotificationParams;
    const METHOD: &'static str = "cargo-scan.set_annotation";
}

impl AuditNotification {
    pub fn annotate_effects_in_single_audit(
        params: AuditNotificationParams,
        af: &mut AuditFile,
        scan_res: &ScanResults,
        audit_file_path: PathBuf,
    ) -> Result<(), Error> {
        let annotation = params.safety_annotation;
        let effect = params.effect;

        if let Some(tree) = find_effect_instance(af, effect)? {
            let new_ann = convert_annotation(annotation);
            tree.set_annotation(new_ann);
            af.recalc_pub_caller_checked(&scan_res.pub_fns);
            af.version += 1;

            af.save_to_file(audit_file_path)?;
        }

        Ok(())
    }

    pub fn annotate_effects_in_chain_audit(
        params: AuditNotificationParams,
        chain_manifest: &Path,
        scan_res: &ScanResults,
        root_crate_path: &PathBuf,
    ) -> Result<(), Error> {
        let annotation = params.safety_annotation;
        let effect = params.effect;

        let crate_name =
            CanonicalPath::new_owned(effect.get_caller()).crate_name().to_string();

        if let Some(mut chain) =
            AuditChain::read_audit_chain(chain_manifest.to_path_buf())?
        {
            let crate_id = chain
                .resolve_crate_id(&crate_name)
                .context(format!("Couldn't resolve crate_name for {}", &crate_name))?;

            if let Some(prev_af) = chain.read_audit_file(&crate_id)? {
                let mut new_af = prev_af.clone();
                if let Some(tree) = find_effect_instance(&mut new_af, effect.clone())? {
                    let new_ann = convert_annotation(annotation);
                    tree.set_annotation(new_ann);

                    if new_af.base_dir != *root_crate_path {
                        let scan_res =
                            scan_crate(&new_af.base_dir, &new_af.scanned_effects, true)?;
                        new_af.recalc_pub_caller_checked(&scan_res.pub_fns);
                    } else {
                        new_af.recalc_pub_caller_checked(&scan_res.pub_fns);
                    };

                    chain.save_audit_file(&crate_id, &new_af)?;

                    // update parent crates based off updated effects
                    let removed_fns = AuditFile::pub_diff(&prev_af, &new_af);
                    chain
                        .remove_cross_crate_effects(removed_fns, &chain.root_crate()?)?;

                    chain.save_to_file()?;
                }
            }
        }

        Ok(())
    }
}
