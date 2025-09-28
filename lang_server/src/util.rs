use std::{collections::HashMap, path::Path};

use anyhow::{anyhow, Error};
use cargo_scan::{
    audit_chain::{collect_propagated_sinks, AuditChain},
    audit_file::{AuditFile, EffectInfo, EffectTree, SafetyAnnotation},
    effect::EffectInstance,
    ident::CanonicalPath,
    scanner::ScanResults,
};

use crate::{
    location::to_src_loc,
    request::{CallerCheckedResponse, EffectsResponse},
};

pub fn find_effect_instance(
    audit_file: &mut AuditFile,
    effect: EffectsResponse,
) -> Result<Vec<&mut EffectTree>, Error> {
    let callee_loc = to_src_loc(&effect.location)?;
    let callee = CanonicalPath::new_owned(effect.clone().callee);
    let caller_path = CanonicalPath::new_owned(effect.clone().caller);
    let curr_effect = EffectInfo { caller_path, callee_loc };
    let mut trees = vec![];

    audit_file.audit_trees.iter_mut().filter(|(i, _)| *i.callee() == callee).for_each(
        |(_, t)| {
            t.get_trees_mut(&curr_effect, &mut trees);
        },
    );

    Ok(trees)
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
    let mut chain = AuditChain::read_audit_chain(chain_manifest.to_path_buf())?
        .ok_or_else(|| {
            anyhow!("Couldn't find audit chain manifest at {}", chain_manifest.display())
        })?;

    // Check if any dependency sinks were removed by other audits
    // and update chain before loading all existing effects
    let removed_sinks = chain.collect_all_safe_sinks()?;
    chain.remove_cross_crate_effects(removed_sinks, &chain.root_crate()?)?;
    collect_propagated_sinks(&mut chain)
}

pub fn get_callers(
    af: &mut AuditFile,
    effect: EffectsResponse,
    scan_res: &ScanResults,
) -> Result<CallerCheckedResponse, Error> {
    let caller_path = CanonicalPath::new_owned(effect.get_caller());
    let callee_loc = to_src_loc(&effect.location)?;

    let new_audit_locs = get_new_audit_locs(scan_res, &caller_path)?;
    let callers = CallerCheckedResponse::new(&effect, &new_audit_locs)?;

    for tree in find_effect_instance(af, effect)? {
        let curr_effect = EffectInfo {
            caller_path: caller_path.clone(),
            callee_loc: callee_loc.clone(),
        };
        add_callers_to_tree(new_audit_locs.clone(), tree, curr_effect);
    }
    af.recalc_pub_caller_checked(&scan_res.pub_fns);

    Ok(callers)
}
