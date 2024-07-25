use std::{collections::HashMap, path::Path};

use anyhow::{anyhow, Error};
use cargo_scan::{
    audit_chain::AuditChain,
    audit_file::{AuditFile, EffectInfo, EffectTree, SafetyAnnotation},
    effect::EffectInstance,
    ident::CanonicalPath,
    scanner::ScanResults,
};

use crate::{location::to_src_loc, request::EffectsResponse};

pub fn find_effect_instance(
    audit_file: &mut AuditFile,
    effect: EffectsResponse,
) -> Result<Option<&mut EffectTree>, Error> {
    let callee_loc = to_src_loc(&effect.location)?;
    let callee = CanonicalPath::new_owned(effect.clone().callee);
    let caller_path = CanonicalPath::new_owned(effect.clone().caller);
    let curr_effect = EffectInfo { caller_path, callee_loc };

    match audit_file.audit_trees.iter_mut().find_map(|(i, t)| {
        let leaf = t.get_leaf_mut(&curr_effect);
        if *i.callee() == callee {
            leaf
        } else {
            None
        }
    }) {
        Some(tree) => Ok(Some(tree)),
        None => Ok(None),
    }
}

pub fn get_new_audit_locs(
    scan_res: &ScanResults,
    caller: &CanonicalPath,
) -> Result<Vec<EffectTree>, Error> {
    let new_locs = scan_res
        .get_callers(caller)?
        .into_iter()
        .map(|e| EffectTree::Leaf(e, SafetyAnnotation::Skipped))
        .collect::<Vec<_>>();

    Ok(new_locs)
}

pub fn add_callers_to_tree(
    new_audit_locs: Vec<EffectTree>,
    tree: &mut EffectTree,
    curr_effect: EffectInfo,
) {
    if new_audit_locs.is_empty() {
        tree.set_annotation(SafetyAnnotation::CallerChecked);
    } else {
        *tree = EffectTree::Branch(curr_effect, new_audit_locs);
    }
}

pub fn get_all_chain_effects(
    chain_manifest: &Path,
) -> Result<HashMap<EffectInstance, Vec<(EffectInfo, String)>>, Error> {
    let mut effects = HashMap::new();
    let mut chain = AuditChain::read_audit_chain(chain_manifest.to_path_buf())?
        .ok_or_else(|| {
            anyhow!("Couldn't find audit chain manifest at {}", chain_manifest.display())
        })?;

    for crate_id in chain.to_owned().all_crates() {
        if let Some(af) = chain.read_audit_file(crate_id)? {
            for (effect_instance, audit_tree) in &af.audit_trees {
                effects.insert(effect_instance.clone(), audit_tree.get_all_annotations());
            }
        }
    }

    Ok(effects)
}
