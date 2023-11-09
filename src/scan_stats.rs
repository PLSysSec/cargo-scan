//! Extract statistics from a scan.
//!
//! Calls into
//! - scanner for ScanResults (list of effects)
//! - audit_file for AuditFile (caller-checked results)

use crate::ident::CanonicalPath;

use super::audit_file::{AuditFile, EffectTree};
use super::effect::{EffectInstance, EffectType, DEFAULT_EFFECT_TYPES};
use super::loc_tracker::LoCTracker;
use super::scanner::ScanResults;

use anyhow::Result;
use log::{debug, info, warn};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct CrateStats {
    pub crate_path: PathBuf,

    // List of effects
    pub effects: Vec<EffectInstance>,

    // Scan metadata
    pub total_loc: LoCTracker,
    pub skipped_macros: LoCTracker,
    pub skipped_conditional_code: LoCTracker,
    pub skipped_fn_calls: LoCTracker,
    pub skipped_fn_ptrs: LoCTracker,
    pub skipped_other: LoCTracker,
    pub unsafe_traits: LoCTracker,
    pub unsafe_impls: LoCTracker,
    pub pub_fns: usize,

    // AuditFile metadata
    pub pub_fns_with_effects: usize,
    pub pub_total_effects: usize,
    pub audited_fns: usize,
    pub audited_loc: usize,
}

impl CrateStats {
    pub fn metadata_csv_header() -> &'static str {
        "\
        num_effects, \
        total, loc_lb, loc_ub, \
        macros, loc_lb, loc_ub, \
        conditional_code, loc_lb, loc_ub, \
        skipped_calls, loc_lb, loc_ub, \
        skipped_fn_ptrs, loc_lb, loc_ub, \
        skipped_other, loc_lb, loc_ub, \
        unsafe_trait, loc_lb, loc_ub, \
        unsafe_impl, loc_lb, loc_ub, \
        pub_fns, pub_fns_with_effects, pub_total_effects, \
        audited_fns, audited_loc\
        "
    }
    pub fn metadata_csv(&self) -> String {
        format!(
            "{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}",
            self.effects.len(),
            self.total_loc.as_csv(),
            self.skipped_macros.as_csv(),
            self.skipped_conditional_code.as_csv(),
            self.skipped_fn_calls.as_csv(),
            self.skipped_fn_ptrs.as_csv(),
            self.skipped_other.as_csv(),
            self.unsafe_traits.as_csv(),
            self.unsafe_impls.as_csv(),
            self.pub_fns,
            self.pub_fns_with_effects,
            self.pub_total_effects,
            self.audited_fns,
            self.audited_loc,
        )
    }
}

pub fn get_crate_stats_default(crate_path: PathBuf, quick_mode: bool) -> CrateStats {
    get_crate_stats(crate_path.clone(), DEFAULT_EFFECT_TYPES, quick_mode).unwrap_or_else(
        |_| {
            warn!("Scan crashed, skipping crate: {}", crate_path.to_string_lossy());
            CrateStats { crate_path, ..Default::default() }
        },
    )
}

pub fn get_crate_stats(
    crate_path: PathBuf,
    effect_types: &[EffectType],
    quick_mode: bool,
) -> Result<CrateStats> {
    let (audit, results) = AuditFile::new_caller_checked_default_with_results(
        &crate_path,
        effect_types,
        quick_mode,
    )?;

    let pub_fns = results.pub_fns.len();
    let mut pub_fns_with_effects = 0;
    let mut pub_total_effects = 0;
    for v in audit.pub_caller_checked.values() {
        // println!("found public function {} with {} effects", k, v.len());
        if !v.is_empty() {
            pub_fns_with_effects += 1;
            pub_total_effects += v.len();
        }
    }

    let (audited_fns, audited_loc) = get_auditing_metrics(&audit, &results);

    let result = CrateStats {
        crate_path,
        effects: results.effects,
        total_loc: results.total_loc,
        skipped_macros: results.skipped_macros,
        skipped_conditional_code: results.skipped_conditional_code,
        skipped_fn_calls: results.skipped_fn_calls,
        skipped_fn_ptrs: results.skipped_fn_ptrs,
        skipped_other: results.skipped_other,
        unsafe_traits: results.unsafe_traits,
        unsafe_impls: results.unsafe_impls,
        pub_fns,
        pub_fns_with_effects,
        pub_total_effects,
        audited_fns,
        audited_loc,
    };

    Ok(result)
}

// Calculates the total number of functions and the total lines of code that will be audited.
fn get_auditing_metrics(audit: &AuditFile, results: &ScanResults) -> (usize, usize) {
    let mut total_loc = 0;
    let mut total_fns: HashSet<&CanonicalPath> = HashSet::new();

    for tree in audit.audit_trees.values() {
        total_fns.extend(counter(tree));
    }

    for f in &total_fns {
        if let Some(tracker) = results.fn_loc_tracker.get(f) {
            total_loc += tracker.get_loc_lb();
        } else {
            // This case happens in the case of abstract trait method nodes
            debug!("no tracker found for a method -- possibly an abstract trait method");
        }
    }

    (total_fns.len(), total_loc)
}

fn counter(tree: &EffectTree) -> HashSet<&CanonicalPath> {
    let mut set: HashSet<&CanonicalPath> = HashSet::new();

    match tree {
        EffectTree::Leaf(info, _) => {
            set.insert(&info.caller_path);
        }
        EffectTree::Branch(info, branch) => {
            let s = branch.iter().fold(HashSet::new(), |mut set, tree| {
                set.extend(counter(tree));

                set
            });

            set.insert(&info.caller_path);
            set.extend(s);
        }
    };

    set
}
