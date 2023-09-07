use std::collections::HashSet;

use crate::audit_chain::AuditChain;
use crate::audit_file::{EffectInfo, EffectTree};
use crate::auditing::info::*;
use crate::effect::{Effect, EffectInstance};
use crate::ident::CanonicalPath;
use crate::scanner::scan_crate;
use crate::sink::Sink;
use crate::{
    audit_file::{AuditFile, SafetyAnnotation},
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
    orig_effect: &'a EffectInstance,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
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
        config,
    )
}

fn audit_leaf<'a>(
    orig_effect: &'a EffectInstance,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
    config: &Config,
) -> Result<AuditStatus> {
    print_and_update_audit(orig_effect, effect_tree, effect_history, scan_res, config)
}

fn update_audit_child<'a>(
    orig_effect: &'a EffectInstance,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
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
        config,
    )
}

fn audit_branch<'a>(
    orig_effect: &'a EffectInstance,
    effect_tree: &mut EffectTree,
    effect_history: &[&'a EffectInfo],
    scan_res: &ScanResults,
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
            update_audit_child(orig_effect, effect_tree, effect_history, scan_res, config)
        } else {
            Ok(AuditStatus::ContinueAudit)
        }
    } else {
        Err(anyhow!("Tried to audit an EffectTree branch, but was actually a leaf"))
    }
}

// TODO: Now that our auditing for branches and leaves are very similar, we might
//       want to combine them into one function so we don't have to check to make
//       sure we are in the right variant very time
fn audit_effect_tree(
    orig_effect: &EffectInstance,
    effect_tree: &mut EffectTree,
    scan_res: &ScanResults,
    config: &Config,
) -> Result<AuditStatus> {
    match effect_tree {
        e @ EffectTree::Leaf(..) => {
            audit_leaf(orig_effect, e, &Vec::new(), scan_res, config)
        }
        e @ EffectTree::Branch(..) => {
            audit_branch(orig_effect, e, &Vec::new(), scan_res, config)
        }
    }
}

// TODO: When we exit early, we have no way of knowing which effects the user
//       has already gone through in this audit and marked "skipped" and so we
//       will re-prompt the user once we resume auditing the audit file. We would
//       like to remember that they have already seen these effects during this
//       audit
/// Iterate through all the skipped annotations in the audit file and perform
/// the auditing process on those effect trees. Will exit early if the user
/// audits one of the root effects as needing to check its child effects, in
/// which case we will return Ok with Some EffectInstance which contains the effect
/// in the dependency crates that need to be audited.
pub fn start_audit(
    audit_file: &mut AuditFile,
    scan_res: ScanResults,
    config: &Config,
) -> Result<Option<EffectInstance>> {
    // We will set this to the root effect we need to audit if we audit an
    // effect tree and need to now traverse into the dependency packages.
    let mut dependency_audit_effect: Option<EffectInstance> = None;

    let (unaudited_base, unaudited_total) = audit_file.unaudited_effects();
    if unaudited_base > 0 {
        println!("Total unaudited effects: {}", unaudited_base);
        println!("Total unaudited locations: {}", unaudited_total);
    }

    if audit_file.has_unsafe_effect() {
        println!("WARNING: package has been marked as unsafe");
    }

    // Iterate through the effects and prompt the user for if they're safe
    for (e, t) in audit_file.audit_trees.iter_mut() {
        match t.get_leaf_annotation() {
            Some(SafetyAnnotation::Skipped) => {
                match audit_effect_tree(e, t, &scan_res, config)? {
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

            None => match audit_effect_tree(e, t, &scan_res, config)? {
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
            },
        }
    }

    println!("No more effects to audit");

    // NOTE: We recalculate the public functions here so we don't have to keep
    //       track of them during the audit. This is a bit slower, but simplifies
    //       the code dramatically.
    audit_file.recalc_pub_caller_checked(&scan_res.pub_fns);

    Ok(dependency_audit_effect)
}

fn update_audit_annotation(
    annotation: SafetyAnnotation,
    scan_res: &ScanResults,
    effect_tree: &mut EffectTree,
    curr_effect: EffectInfo,
) -> Result<AuditStatus> {
    match annotation {
        SafetyAnnotation::CallerChecked => {
            // If we are already in a branch, this indicates we have marked this
            // level as caller-checked already, and we don't need to update
            // anything
            if let EffectTree::Branch(_, _) = effect_tree {
                return Ok(AuditStatus::ContinueAudit);
            }

            // Add all call locations as parents of this effect
            let new_check_locs = scan_res
                .get_callers(&curr_effect.caller_path)?
                .into_iter()
                .map(|e| EffectTree::Leaf(e, SafetyAnnotation::Skipped))
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
    orig_effect: &EffectInstance,
    scan_res: &ScanResults,
    effect_tree: &mut EffectTree,
    effect_history: &[&EffectInfo],
    curr_effect: EffectInfo,
    config: &Config,
) -> Result<AuditStatus> {
    match get_user_annotation() {
        Ok((Some(a), AuditStatus::ContinueAudit)) => {
            let update_status =
                update_audit_annotation(a, scan_res, effect_tree, curr_effect)?;
            if update_status == AuditStatus::AuditParentEffect {
                audit_branch(orig_effect, effect_tree, effect_history, scan_res, config)
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

/// Looks up the audit associated with the crate from `sink_ident` and audit
/// the sink public function. This function is responsible for updating the
/// chain and any audit files on the filesystem from the audit. Returns the set
/// of removed functions if it succeeds.
pub fn audit_pub_fn(
    chain: &mut AuditChain,
    sink_ident: &Sink,
) -> Result<HashSet<CanonicalPath>> {
    let sink_crate = sink_ident
        .first_ident()
        .ok_or_else(|| anyhow!("Missing leading identifier for pattern"))?;
    // TODO: The sink crate we get here may include the version
    let (sink_crate_id, mut prev_audit_file) =
        chain.read_audit_file_no_version(sink_crate.as_str())?.ok_or_else(|| {
            anyhow!("Couldn't find audit file for the sink: {}", sink_crate)
        })?;
    let mut new_audit_file = prev_audit_file.clone();

    // Find the public function associated with the sink
    let scan_res =
        scan_crate(&new_audit_file.base_dir, &prev_audit_file.scanned_effects)?;
    let sink_fn = CanonicalPath::new(sink_ident.as_str());
    loop {
        // Keep looping until we are done with auditing children
        match audit_pub_fn_effect(&mut new_audit_file, &sink_fn, &scan_res)? {
            (AuditStatus::ContinueAudit | AuditStatus::EarlyExit, _) => {
                // We are done auditing this crate, so break out to clean up
                break;
            }
            (AuditStatus::AuditChildEffect, Some(child_effect)) => {
                // Save the current audit,
                new_audit_file.recalc_pub_caller_checked(&scan_res.pub_fns);
                chain.save_audit_file(&sink_crate_id, &new_audit_file)?;
                let removed_fns = AuditFile::pub_diff(&prev_audit_file, &new_audit_file);
                chain.remove_cross_crate_effects(removed_fns, &sink_crate_id)?;
                prev_audit_file = new_audit_file;

                let child_sink = match child_effect.eff_type() {
                    Effect::SinkCall(s) => s,
                    _ => {
                        return Err(anyhow!(
                            "Can only audit the children of Sink effects"
                        ))
                    }
                };
                audit_pub_fn(chain, child_sink)?;
                // We have to reload the new audit file because auditing child
                // effects may have removed some base effects from the current
                // crate
                new_audit_file =
                    chain.read_audit_file(&sink_crate_id)?.ok_or_else(|| {
                        anyhow!(
                            "Couldn't find audit file for the sink: {}",
                            sink_crate_id
                        )
                    })?;
                // After we audit the child function, we will recurse until the
                // user marks everything, or we run out of child functions to
                // audit.
            }
            (AuditStatus::AuditChildEffect, None) => {
                return Err(anyhow!(
                "Should never try to audit the child effect without an associated effect"
            ))
            }
            (AuditStatus::AuditParentEffect, _) => {
                return Err(anyhow!("Cannot audit parent effect in this context"));
            }
        }
    }

    // Save the new audit fiel
    new_audit_file.recalc_pub_caller_checked(&scan_res.pub_fns);
    chain.save_audit_file(&sink_crate_id, &new_audit_file)?;

    // update parent crates based off updated effects
    let removed_fns = AuditFile::pub_diff(&prev_audit_file, &new_audit_file);
    let removed_fns = chain.remove_cross_crate_effects(removed_fns, &sink_crate_id)?;

    Ok(removed_fns)
}

fn audit_pub_fn_effect(
    audit_file: &mut AuditFile,
    sink_fn: &CanonicalPath,
    scan_res: &ScanResults,
) -> Result<(AuditStatus, Option<EffectInstance>)> {
    for base_effect in audit_file.pub_caller_checked.get(sink_fn).ok_or_else(|| {
        anyhow!("Couldn't find public function from sink: {:?}", &sink_fn)
    })? {
        let effect_tree =
            audit_file.audit_trees.get_mut(base_effect).ok_or_else(|| {
                anyhow!(
                "Couldn't find tree when auditing public function for effect block: {:?}",
                base_effect
            )
            })?;
        let config = Config::default();
        match audit_effect_tree(base_effect, effect_tree, scan_res, &config)? {
            AuditStatus::ContinueAudit => (),
            s @ AuditStatus::EarlyExit => {
                return Ok((s, None));
            }
            s @ AuditStatus::AuditChildEffect => {
                return Ok((s, Some(base_effect.clone())));
            }
            AuditStatus::AuditParentEffect => {
                return Err(anyhow!("Cannot audit parent effect in this context"));
            }
        }
    }
    Ok((AuditStatus::ContinueAudit, None))
}
