use std::path::{Path, PathBuf};

use anyhow::{Context, Error};
use cargo_scan::{
    audit_chain::AuditChain,
    audit_file::{AuditFile, EffectTree},
    ident::CanonicalPath,
    scanner::ScanResults,
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
    // If we're trying to annotate a branch, then we should
    // remove all its callers from the audit file and make it
    // a leaf before setting the new safety annotation
    fn update_annotation(trees: Vec<&mut EffectTree>, a: String) {
        let ann = convert_annotation(a.clone());

        for tree in trees {
            match tree {
                EffectTree::Leaf(_, _) => {
                    tree.set_annotation(ann);
                }
                EffectTree::Branch(i, _) => {
                    *tree = EffectTree::Leaf(i.to_owned(), ann);
                }
            };
        }
    }

    pub fn annotate_effects_in_single_audit(
        params: AuditNotificationParams,
        af: &mut AuditFile,
        scan_res: &ScanResults,
        audit_file_path: PathBuf,
    ) -> Result<(), Error> {
        let effect = params.effect;
        let trees = find_effect_instance(af, effect)?;

        Self::update_annotation(trees, params.safety_annotation);
        af.recalc_pub_caller_checked(&scan_res.pub_fns);
        af.version += 1;

        af.save_to_file(audit_file_path)?;

        Ok(())
    }

    pub fn annotate_effects_in_chain_audit(
        params: AuditNotificationParams,
        chain_manifest: &Path,
    ) -> Result<(), Error> {
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
                let trees = find_effect_instance(&mut new_af, effect.clone())?;
                Self::update_annotation(trees, params.safety_annotation);
                new_af.recalc_pub_caller_checked(
                    &new_af.pub_caller_checked.keys().cloned().collect(),
                );

                chain.save_audit_file(&crate_id, &new_af)?;
                // update parent crates based off updated effects
                let removed_fns = AuditFile::pub_diff(&prev_af, &new_af);
                chain.remove_cross_crate_effects(removed_fns, &chain.root_crate()?)?;
                chain.save_to_file()?;
            }
        }

        Ok(())
    }
}
