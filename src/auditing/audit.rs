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
    AuditParentEffect,
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

fn print_and_update_audit<'a>(
    orig_effect: &'a EffectBlock,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
    pub_caller_checked: &mut HashSet<CanonicalPath>,
    config: &Config,
) -> Result<AuditStatus> {
    let curr_effect = match effect_tree {
        EffectTree::Leaf(e, _) | EffectTree::Branch(e, _) => e.clone(),
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

fn audit_leaf<'a>(
    orig_effect: &'a EffectBlock,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
    pub_caller_checked: &mut HashSet<CanonicalPath>,
    config: &Config,
) -> Result<AuditStatus> {
    print_and_update_audit(
        orig_effect,
        effect_tree,
        effect_history,
        scan_res,
        pub_caller_checked,
        config,
    )
}

fn update_audit_child<'a>(
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

        // Set this to true if we should audit the child of an effect (the
        // Effect we are iterating over)
        let mut audit_child = false;
        for e in effects {
            // TODO: Early exit
            match e {
                next_e @ EffectTree::Branch(..) => {
                    match audit_branch(
                        orig_effect,
                        next_e,
                        &next_history,
                        scan_res,
                        pub_caller_checked,
                        config,
                    )? {
                        AuditStatus::EarlyExit => {
                            return Ok(AuditStatus::EarlyExit);
                        }
                        AuditStatus::AuditChildEffect => {
                            audit_child = true;
                            break;
                        }
                        _ => (),
                    }
                }
                next_e @ EffectTree::Leaf(..) => {
                    match audit_leaf(
                        orig_effect,
                        next_e,
                        &next_history,
                        scan_res,
                        pub_caller_checked,
                        config,
                    )? {
                        AuditStatus::EarlyExit => {
                            return Ok(AuditStatus::EarlyExit);
                        }
                        AuditStatus::AuditChildEffect => {
                            audit_child = true;
                            break;
                        }
                        _ => (),
                    }
                }
            };
        }

        if audit_child {
            update_audit_child(
                orig_effect,
                effect_tree,
                effect_history,
                scan_res,
                pub_caller_checked,
                config,
            )
        } else {
            Ok(AuditStatus::ContinueAudit)
        }
    } else {
        Err(anyhow!("Tried to audit an EffectTree branch, but was actually a leaf"))
    }
}

// TODO: Now that our auditing for branches and leaves are very similar, we might
//       want to combine them into one function so we don't have to check to make
//       sure we are in the right variante very time
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

// TODO: When we exit early, we have no way of knowing which effects the user
//       has already gone through in this audit and marked "skipped" and so we
//       will re-prompt the user once we resume auditing the policy. We would
//       like to remember that they have already seen these effects during this
//       audit
/// Iterate through all the skipped annotaitons in the policy file and perform
/// the auditing process on those effect trees. Will exit early if the user
/// audits one of the root effects as needing to check its child effects, in
/// which case we will return Ok with Some EffectBlock which contains the effect
/// in the dependency crates that need to be audited.
pub fn audit_policy(
    policy: &mut PolicyFile,
    scan_res: ScanResults,
    config: &Config,
) -> Result<Option<EffectBlock>> {
    // We will set this to the root effect we need to audit if we audit an
    // effect tree and need to now traverse into the dependency packages.
    let mut dependency_audit_effect: Option<EffectBlock> = None;

    // Iterate through the effects and prompt the user for if they're safe
    for (e, t) in policy.audit_trees.iter_mut() {
        match t.get_leaf_annotation() {
            Some(SafetyAnnotation::Skipped) => {
                match audit_effect_tree(
                    e,
                    t,
                    &scan_res,
                    &mut policy.pub_caller_checked,
                    config,
                )? {
                    AuditStatus::EarlyExit => {
                        break;
                    }
                    AuditStatus::AuditChildEffect => {
                        dependency_audit_effect = Some(e.clone());
                        break;
                    }
                    AuditStatus::AuditParentEffect => {
                        return Err(anyhow!("We should never return this status here"));
                    }
                    _ => (),
                }
            }

            Some(_) => (),

            None => {
                match audit_effect_tree(
                    e,
                    t,
                    &scan_res,
                    &mut policy.pub_caller_checked,
                    config,
                )? {
                    AuditStatus::EarlyExit => {
                        break;
                    }
                    AuditStatus::AuditChildEffect => {
                        dependency_audit_effect = Some(e.clone());
                        break;
                    }
                    AuditStatus::AuditParentEffect => {
                        return Err(anyhow!("We should never return this status here"));
                    }
                    _ => (),
                }
            }
        }
    }

    Ok(dependency_audit_effect)
}

fn update_audit_annotation(
    annotation: SafetyAnnotation,
    scan_res: &ScanResults,
    effect_tree: &mut EffectTree,
    curr_effect: EffectInfo,
    pub_caller_checked: &mut HashSet<CanonicalPath>,
) -> Result<AuditStatus> {
    match annotation {
        SafetyAnnotation::CallerChecked => {
            // If we are already in a branch, this indicates we have marked this
            // level as caller-checked already, and we don't need to update
            // anything
            if let EffectTree::Branch(_, _) = effect_tree {
                return Ok(AuditStatus::ContinueAudit);
            }

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
                Ok(AuditStatus::AuditParentEffect)
            }
        }

        s => {
            // TODO: If we aren't at a leaf node and we mark it skipped, don't
            //       update anything
            effect_tree.set_annotation(s).ok_or_else(|| {
                anyhow!("Tried to set the EffectTree annotation, but was a branch node")
            })?;
            Ok(AuditStatus::ContinueAudit)
        }
    }
}

fn update_audit_from_input(
    orig_effect: &EffectBlock,
    scan_res: &ScanResults,
    effect_tree: &mut EffectTree,
    effect_history: &[&EffectInfo],
    curr_effect: EffectInfo,
    pub_caller_checked: &mut HashSet<CanonicalPath>,
    config: &Config,
) -> Result<AuditStatus> {
    match get_user_annotation() {
        Ok((Some(a), AuditStatus::ContinueAudit)) => {
            let update_status = update_audit_annotation(
                a,
                scan_res,
                effect_tree,
                curr_effect,
                pub_caller_checked,
            )?;
            if update_status == AuditStatus::AuditParentEffect {
                audit_branch(
                    orig_effect,
                    effect_tree,
                    effect_history,
                    scan_res,
                    pub_caller_checked,
                    config,
                )
            } else {
                Ok(AuditStatus::ContinueAudit)
            }
        }
        Ok((_, s @ AuditStatus::AuditChildEffect)) => Ok(s),
        Ok((None, AuditStatus::ContinueAudit)) => Err(anyhow!(
            "Should never return ContinueAudit if we don't have an annotation"
        )),
        Ok((_, s @ AuditStatus::EarlyExit)) => Ok(s),
        Ok((_, AuditStatus::AuditParentEffect)) => {
            // TODO: This is for the case where we are walking down the effect
            //       stack for auditing child effects and the user decides they
            //       want to back out to a parent effect. We don't yet support
            //       this functionality, but this is where it will go.
            unimplemented!();
        }
        Err(_) => {
            println!("Error accepting user input. Attempting to continue...");
            Ok(AuditStatus::ContinueAudit)
        }
    }
}

// pub fn audit_pub_fn(
//     policy: &mut PolicyFile,
// )
