use crate::{
    effect::EffectBlock,
    policy::{EffectTree, PolicyFile, SafetyAnnotation},
};

use anyhow::{anyhow, Result};
use inquire::{validator::Validation, Text};
use std::{self, fs::File, io::BufRead, io::BufReader, path::PathBuf};

/// Returns Some index or None if the user chooses to exit
fn select_reset(num_effects: usize) -> Result<Option<usize>> {
    let ans = Text::new(&format!(
        "Select effect to reset:\n ({}), e(x)it",
        if num_effects == 0 {
            "0".to_string()
        } else {
            format!("0..{}", num_effects - 1)
        }
    ))
    .with_validator(move |x: &str| match x {
        "x" => Ok(Validation::Valid),
        s => match s.parse::<usize>() {
            Ok(i) if i < num_effects => Ok(Validation::Valid),
            _ => Ok(Validation::Invalid("Invalid input".into())),
        },
    })
    .prompt()
    .unwrap();

    match ans.as_str() {
        "x" => Ok(None),
        s => match s.parse::<usize>() {
            Ok(i) if i < num_effects => Ok(Some(i)),
            _ => Err(anyhow!("Got an invalid response for the effect to reset")),
        },
    }
}

fn print_blocks(blocks: &[(&EffectBlock, SafetyAnnotation)]) -> Result<()> {
    for (idx, (block, annotation)) in blocks.iter().enumerate() {
        let src_loc = block.src_loc();
        let mut full_path = PathBuf::from(src_loc.dir());
        full_path.push(src_loc.file());
        let effect_line = if let Ok(f) = File::open(full_path) {
            let reader = BufReader::new(f);
            reader
                .lines()
                .nth(src_loc.start_line())
                .unwrap_or_else(|| Ok("INVALID SrcLoc FOR FILE".to_string()))
                .unwrap_or_else(|_| "INVALID SrcLoc FOR FILE".to_string())
        } else {
            "MISSING FILE".to_string()
        };
        println!(
            "{idx} - ({annotation}) {filename}, {line}:{col} {effect_line:60}",
            filename = src_loc.file().to_string_lossy(),
            line = src_loc.start_line(),
            col = src_loc.start_col(),
        );
    }

    Ok(())
}

pub fn reset_annotation(mut policy: PolicyFile, policy_path: PathBuf) -> Result<()> {
    let mut new_audit_trees = policy.audit_trees.clone();
    let mut annotated_base_effects = policy
        .audit_trees
        .iter()
        .filter_map(|(block, t)| match t {
            EffectTree::Leaf(_, ann @ SafetyAnnotation::CallerChecked)
            | EffectTree::Leaf(_, ann @ SafetyAnnotation::Safe)
            | EffectTree::Leaf(_, ann @ SafetyAnnotation::Unsafe) => Some((block, *ann)),
            EffectTree::Branch(_, _) => Some((block, SafetyAnnotation::CallerChecked)),
            _ => None,
        })
        .collect::<Vec<_>>();

    let mut len = annotated_base_effects.len();
    while len > 0 {
        print_blocks(&annotated_base_effects)?;
        match select_reset(len)? {
            None => {
                break;
            }
            Some(idx) => {
                len -= 1;
                let block = annotated_base_effects.remove(idx);
                let tree = new_audit_trees.get_mut(block.0).unwrap();
                let info = match tree {
                    EffectTree::Branch(info, _) | EffectTree::Leaf(info, _) => {
                        info.clone()
                    }
                };
                *tree = EffectTree::Leaf(info, SafetyAnnotation::Skipped);
            }
        }
    }

    policy.audit_trees = new_audit_trees;
    policy.save_to_file(policy_path)?;

    println!("No more annotated effects to reset");

    Ok(())
}
