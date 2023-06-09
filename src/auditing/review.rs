use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::Path;

use super::info::Config;
use super::util::is_policy_scan_valid;
use crate::auditing::info::print_effect_info;
use crate::effect::{EffectBlock, SrcLoc};
use crate::ident::CanonicalPath;
use crate::policy::{EffectInfo, EffectTree, PolicyFile, SafetyAnnotation};
use crate::scanner;

fn review_effect_tree_info_helper(
    orig_effect: &EffectBlock,
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
    effect: &EffectBlock,
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
    let scan_res = scanner::scan_crate(crate_path)?;
    let scan_effect_blocks = scan_res.unsafe_effect_blocks_set();
    if !is_policy_scan_valid(policy, &scan_effect_blocks, crate_path)? {
        println!("Error: crate has changed since last policy scan.");
        return Err(anyhow!("Invalid policy during review"));
    }

    for (e, a) in policy.audit_trees.iter() {
        review_effect_tree_info(e, a, &scan_res.fn_locs, config)?;
    }

    Ok(())
}
