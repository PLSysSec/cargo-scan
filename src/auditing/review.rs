use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;

use super::info::Config;
use super::util::is_policy_scan_valid;
use crate::auditing::info::print_effect_info;
use crate::effect::{EffectInstance, SrcLoc, EffectType};
use crate::ident::CanonicalPath;
use crate::policy::{EffectInfo, EffectTree, PolicyFile, SafetyAnnotation};
use crate::scanner;

fn review_effect_tree_info_helper(
    orig_effect: &EffectInstance,
    effect_tree: &EffectTree,
    effect_history: &[&EffectInfo],
    fn_locs: &HashMap<CanonicalPath, SrcLoc>,
    config: &Config,
) -> Result<()> {
    match effect_tree {
        EffectTree::Leaf(new_e, a) => {
            print_effect_info(orig_effect, new_e, effect_history, fn_locs, config)?;
            // TODO: Colorize
            println!("Policy annotation: {}", a);
        }
        EffectTree::Branch(new_e, es) => {
            // TODO: Colorize
            print_effect_info(orig_effect, new_e, effect_history, fn_locs, config)?;
            println!("Policy annotation: {}", SafetyAnnotation::CallerChecked);
            let mut new_history = effect_history.to_owned();
            new_history.push(new_e);
            for new_tree in es {
                review_effect_tree_info_helper(
                    orig_effect,
                    new_tree,
                    &new_history,
                    fn_locs,
                    config,
                )?
            }
        }
    }
    Ok(())
}

fn review_effect_tree_info(
    effect: &EffectInstance,
    effect_tree: &EffectTree,
    fn_locs: &HashMap<CanonicalPath, SrcLoc>,
    config: &Config,
) -> Result<()> {
    review_effect_tree_info_helper(effect, effect_tree, &Vec::new(), fn_locs, config)
}

pub fn review_policy(
    policy: &PolicyFile,
    crate_path: &Path,
    config: &Config,
) -> Result<()> {
    // TODO: Change this scan to use the simpler scan when we add it
    // NOTE: The original scan for the policy we're reviewing wasn't necesarilly created
    //       with the same set of effects we're scanning for now. However, we only use
    //       the scan results to get the function locations, so it doesn't matter.
    let scan_res = scanner::scan_crate(crate_path, &EffectType::unsafe_effects())?;
    if !is_policy_scan_valid(policy, crate_path)? {
        println!("Error: crate has changed since last policy scan.");
        return Err(anyhow!("Invalid policy during review"));
    }

    for (e, a) in policy.audit_trees.iter() {
        review_effect_tree_info(e, a, &scan_res.fn_locs, config)?;
    }

    Ok(())
}
