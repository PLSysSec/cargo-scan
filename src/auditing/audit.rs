use std::collections::HashSet;

use crate::auditing::info::*;
use crate::effect::EffectBlock;
use crate::ident::CanonicalPath;
use crate::policy::{EffectInfo, EffectTree};
use crate::{
    policy::{PolicyFile, SafetyAnnotation},
    scanner::ScanResults,
};
use anyhow::{anyhow, Result};
use inquire::{validator::Validation, Text};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditStatus {
    EarlyExit,
    ContinueAudit,
    AuditChildEffect,
}

// Returns Some SafetyAnnotation if the user selects one, None if the user
// chooses to exit early, or an Error
fn get_user_annotation() -> Result<(Option<SafetyAnnotation>, AuditStatus)> {
    // TODO: Don't let user audit effect origin if we are at a sink
    let ans = Text::new(
        r#"Select how to mark this effect:
  (s)afe, (u)nsafe, (c)aller checked, audit (e)ffect origin, ask me (l)ater, e(x)it tool
"#,
    )
    .with_validator(|x: &str| match x {
        "s" | "u" | "c" | "e" | "l" | "x" => Ok(Validation::Valid),
        _ => Ok(Validation::Invalid("Invalid input".into())),
    })
    .prompt()
    .unwrap();

    match ans.as_str() {
        "s" => Ok((Some(SafetyAnnotation::Safe), AuditStatus::ContinueAudit)),
        "u" => Ok((Some(SafetyAnnotation::Unsafe), AuditStatus::ContinueAudit)),
        "c" => Ok((Some(SafetyAnnotation::CallerChecked), AuditStatus::ContinueAudit)),
        "l" => Ok((Some(SafetyAnnotation::Skipped), AuditStatus::ContinueAudit)),
        "e" => Ok((None, AuditStatus::AuditChildEffect)),
        "x" => Ok((None, AuditStatus::EarlyExit)),
        _ => Err(anyhow!("Invalid annotation selection")),
    }
}

fn audit_leaf<'a>(
    orig_effect: &'a EffectBlock,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
    pub_caller_checked: &mut HashSet<CanonicalPath>,
    config: &Config,
) -> Result<AuditStatus> {
    let curr_effect = match effect_tree {
        EffectTree::Leaf(e, _) => e.clone(),
        _ => {
            return Err(anyhow!("Tried to leaf audit a branch"));
        }
    };

    if print_effect_info(
        orig_effect,
        &curr_effect,
        effect_history,
        &scan_res.fn_locs,
        config,
    )
    .is_err()
    {
        println!("Error printing effect information. Trying to continue...");
    }

    update_audit_from_input(
        orig_effect,
        scan_res,
        effect_tree,
        effect_history,
        curr_effect,
        pub_caller_checked,
        config,
    )
}

fn audit_branch<'a>(
    orig_effect: &'a EffectBlock,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
    pub_caller_checked: &mut HashSet<CanonicalPath>,
    config: &Config,
) -> Result<AuditStatus> {
    if let EffectTree::Branch(curr_effect, effects) = effect_tree {
        let mut next_history = effect_history.to_owned();
        next_history.push(curr_effect);
        for e in effects {
            // TODO: Early exit
            match e {
                next_e @ EffectTree::Branch(..) => {
                    if audit_branch(
                        orig_effect,
                        next_e,
                        &next_history,
                        scan_res,
                        pub_caller_checked,
                        config,
                    )? == AuditStatus::EarlyExit
                    {
                        return Ok(AuditStatus::EarlyExit);
                    }
                }
                next_e @ EffectTree::Leaf(..) => {
                    if audit_leaf(
                        orig_effect,
                        next_e,
                        &next_history,
                        scan_res,
                        pub_caller_checked,
                        config,
                    )? == AuditStatus::EarlyExit
                    {
                        return Ok(AuditStatus::EarlyExit);
                    }
                }
            };
        }
        Ok(AuditStatus::ContinueAudit)
    } else {
        Err(anyhow!("Tried to audit an EffectTree branch, but was actually a leaf"))
    }
}

fn audit_effect_tree(
    orig_effect: &EffectBlock,
    effect_tree: &mut EffectTree,
    scan_res: &ScanResults,
    pub_caller_checked: &mut HashSet<CanonicalPath>,
    config: &Config,
) -> Result<AuditStatus> {
    match effect_tree {
        e @ EffectTree::Leaf(..) => {
            audit_leaf(orig_effect, e, &Vec::new(), scan_res, pub_caller_checked, config)
        }
        e @ EffectTree::Branch(..) => audit_branch(
            orig_effect,
            e,
            &Vec::new(),
            scan_res,
            pub_caller_checked,
            config,
        ),
    }
}

pub fn audit_policy(
    policy: &mut PolicyFile,
    scan_res: ScanResults,
    config: &Config,
) -> Result<()> {
    // Iterate through the effects and prompt the user for if they're safe
    for (e, t) in policy.audit_trees.iter_mut() {
        match t.get_leaf_annotation() {
            Some(SafetyAnnotation::Skipped) => {
                if audit_effect_tree(
                    e,
                    t,
                    &scan_res,
                    &mut policy.pub_caller_checked,
                    config,
                )? == AuditStatus::EarlyExit
                {
                    break;
                }
            }
            Some(_) => (),
            None => {
                if audit_effect_tree(
                    e,
                    t,
                    &scan_res,
                    &mut policy.pub_caller_checked,
                    config,
                )? == AuditStatus::EarlyExit
                {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn update_audit_from_input(
    orig_effect: &EffectBlock,
    scan_res: &ScanResults,
    effect_tree: &mut EffectTree,
    effect_history: &[&EffectInfo],
    curr_effect: EffectInfo,
    pub_caller_checked: &mut HashSet<CanonicalPath>,
    config: &Config
) -> Result<AuditStatus> {
    let (annotation, status) = match get_user_annotation() {
        Ok(x) => x,
        Err(_) => {
            println!("Error accepting user input. Attempting to continue...");
            (Some(SafetyAnnotation::Skipped), AuditStatus::ContinueAudit)
        }
    };

    if status != AuditStatus::ContinueAudit {
        return Ok(status);
    }
    let annotation = annotation.ok_or_else(|| {
        anyhow!("Should never return ContinueAudit if we don't have an annotation")
    })?;

    match annotation {
        SafetyAnnotation::CallerChecked => {
            // If the caller is public, add to set of public caller-checked
            if scan_res.pub_fns.contains(&curr_effect.caller_path) {
                pub_caller_checked.insert(curr_effect.caller_path.clone());
            }

            // Add all call locations as parents of this effect
            let new_check_locs = scan_res
                .get_callers(&curr_effect.caller_path)
                .into_iter()
                .map(|x| {
                    EffectTree::Leaf(
                        EffectInfo::from_instance(&x.clone()),
                        SafetyAnnotation::Skipped,
                    )
                })
                .collect::<Vec<_>>();

            if new_check_locs.is_empty() {
                effect_tree.set_annotation(annotation);
                Ok(AuditStatus::ContinueAudit)
            } else {
                *effect_tree = EffectTree::Branch(curr_effect, new_check_locs);
                audit_branch(
                    orig_effect,
                    effect_tree,
                    effect_history,
                    scan_res,
                    pub_caller_checked,
                    config,
                )
            }
        }

        s => {
            effect_tree.set_annotation(s).ok_or_else(|| {
                anyhow!("Tried to set the EffectTree annotation, but was a branch node")
            })?;
            Ok(AuditStatus::ContinueAudit)
        }
    }
}

// pub fn audit_pub_fn(
//     policy: &mut PolicyFile,
// )
