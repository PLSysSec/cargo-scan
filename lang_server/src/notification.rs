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
        af.recalc_pub_cc_with_safe(&scan_res.pub_fns);
        // If any public caller-checked functions have been removed,
        // bump audit version to notify any chains this crates belongs to
        if !af.safe_pub_fns().is_empty() {
            af.version += 1;
        }

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

            if let Some(mut af) = chain.read_audit_file(&crate_id)? {
                let trees = find_effect_instance(&mut af, effect.clone())?;
                Self::update_annotation(trees, params.safety_annotation);
                af.recalc_pub_cc_with_safe(
                    &af.pub_caller_checked.keys().cloned().collect(),
                );
                
                // If any public caller-checked functions have been removed,
                // bump audit version to notify any chains this crates belongs to
                if !af.safe_pub_fns().is_empty() {
                    af.version += 1;
                }
                chain.save_audit_file(&crate_id, &af)?;               
            }
        }

        Ok(())
    }
}
