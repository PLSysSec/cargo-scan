use super::effect::{EffectInstance, SrcLoc};
use crate::auditing::util::hash_dir;
use crate::effect::{Effect, EffectType};
use crate::ident::CanonicalPath;
use crate::scanner;
use crate::scanner::ScanResults;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::Path as FilePath;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

/// SafetyAnnotation is really a lattice with `Skipped` as the top element, and
/// `Unsafe` as the bottom element.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum SafetyAnnotation {
    Skipped,
    Safe,
    Unsafe,
    CallerChecked,
}

impl fmt::Display for SafetyAnnotation {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SafetyAnnotation::Skipped => write!(f, "Skipped"),
            SafetyAnnotation::Safe => write!(f, "Safe"),
            SafetyAnnotation::Unsafe => write!(f, "Unsafe"),
            SafetyAnnotation::CallerChecked => write!(f, "Caller-checked"),
        }
    }
}

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone, Hash, Eq)]
pub struct EffectInfo {
    pub caller_path: CanonicalPath,
    pub callee_loc: SrcLoc,
}

impl EffectInfo {
    pub fn new(caller_path: CanonicalPath, callee_loc: SrcLoc) -> Self {
        EffectInfo { caller_path, callee_loc }
    }

    pub fn from_instance(effect: &EffectInstance) -> Self {
        let caller_src_path = effect.caller().clone();
        let callee_loc = effect.call_loc().clone();

        EffectInfo::new(caller_src_path, callee_loc)
    }
}

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub enum EffectTree {
    Leaf(EffectInfo, SafetyAnnotation),
    Branch(EffectInfo, Vec<EffectTree>),
}

impl EffectTree {
    pub fn get_leaf_annotation(&self) -> Option<SafetyAnnotation> {
        match self {
            EffectTree::Leaf(_, a) => Some(*a),
            EffectTree::Branch(_, _) => None,
        }
    }

    /// Sets the annotation for a leaf node and returns Some previous annotation,
    /// or None if it was a branch node
    pub fn set_annotation(
        &mut self,
        new_a: SafetyAnnotation,
    ) -> Option<SafetyAnnotation> {
        match self {
            EffectTree::Leaf(_, a) => {
                let ret = *a;
                *a = new_a;
                Some(ret)
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Copy)]
pub enum DefaultAuditType {
    Empty,
    Safe,
    CallerChecked,
}

pub type AuditVersion = u32;

// TODO: Include information about crate/version
// TODO: We should include more information from the ScanResult
#[serde_as]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuditFile {
    #[serde_as(as = "Vec<(_, _)>")]
    pub audit_trees: HashMap<EffectInstance, EffectTree>,
    /// Contains a map from public functions marked caller-checked to a set of
    /// all base EffectInstances that flow into that function
    pub pub_caller_checked: HashMap<CanonicalPath, HashSet<EffectInstance>>,
    // TODO: Make the base_dir a crate instead
    pub base_dir: PathBuf,
    pub hash: [u8; 32],
    pub version: AuditVersion,
    pub scanned_effects: Vec<EffectType>,
}

impl AuditFile {
    pub fn empty(p: PathBuf, relevant_effects: Vec<EffectType>) -> Result<Self> {
        let hash = hash_dir(p.clone())?;
        Ok(AuditFile {
            audit_trees: HashMap::new(),
            pub_caller_checked: HashMap::new(),
            base_dir: p,
            hash,
            version: 0,
            scanned_effects: relevant_effects,
        })
    }

    pub fn set_base_audit_trees<'a, I>(&mut self, effect_blocks: I)
    where
        I: IntoIterator<Item = &'a EffectInstance>,
    {
        self.audit_trees = effect_blocks
            .into_iter()
            .map(|x| {
                (
                    x.clone(),
                    EffectTree::Leaf(
                        EffectInfo::from_instance(x),
                        SafetyAnnotation::Skipped,
                    ),
                )
            })
            .collect::<HashMap<_, _>>();
    }

    pub fn save_to_file(&self, p: PathBuf) -> Result<()> {
        let json = serde_json::to_string(self)?;
        let mut f = File::create(p)?;
        f.write_all(json.as_bytes())?;
        Ok(())
    }

    /// Returns Some audit file if it exists, or None if we should create a new one.
    /// Errors if the audit filepath is invalid or if we can't read an existing
    /// audit file
    pub fn read_audit_file(path: PathBuf) -> Result<Option<AuditFile>> {
        if path.is_dir() {
            Err(anyhow!("Audit path is a directory"))
        } else if path.is_file() {
            let json_string = std::fs::read_to_string(path.as_path())?;
            let audit_file = serde_json::from_str(&json_string)?;
            Ok(Some(audit_file))
        } else {
            Ok(None)
        }
    }

    /// Mark caller-checked functions but don't add a caller to the tree more
    /// than once (so we don't get an infinite cycle).
    fn mark_caller_checked_recurse(
        base_effect: &EffectInstance,
        tree: &mut EffectTree,
        pub_caller_checked: &mut HashMap<CanonicalPath, HashSet<EffectInstance>>,
        scan_res: &ScanResults,
        prev_callers: Vec<CanonicalPath>,
    ) -> Result<()> {
        if let EffectTree::Leaf(effect_info, annotation) = tree {
            // Add the function to the list of sinks if it is public
            if scan_res.pub_fns.contains(&effect_info.caller_path) {
                pub_caller_checked
                    .entry(effect_info.caller_path.clone())
                    .or_insert_with(HashSet::new)
                    .insert(base_effect.clone());
            }

            let mut callers = scan_res
                .get_callers(&effect_info.caller_path)?
                .into_iter()
                .filter_map(|e| {
                    if prev_callers.contains(&e.caller_path) {
                        None
                    } else {
                        Some(EffectTree::Leaf(e, SafetyAnnotation::Skipped))
                    }
                })
                .collect::<Vec<_>>();
            if callers.is_empty() {
                *annotation = SafetyAnnotation::CallerChecked;
            } else {
                for eff in callers.iter_mut() {
                    let mut next_callers = prev_callers.clone();
                    // NOTE: This will always be a leaf since it is only created
                    //       from the map above
                    if let EffectTree::Leaf(i, _) = eff {
                        next_callers.push(i.caller_path.clone());
                    }
                    AuditFile::mark_caller_checked_recurse(
                        base_effect,
                        eff,
                        pub_caller_checked,
                        scan_res,
                        next_callers,
                    )?;
                }
                *tree = EffectTree::Branch(effect_info.clone(), callers);
            }
        }

        Ok(())
    }

    /// Mark all callers of functions in the effect tree to be caller-checked.
    fn mark_caller_checked(
        base_effect: &EffectInstance,
        tree: &mut EffectTree,
        pub_caller_checked: &mut HashMap<CanonicalPath, HashSet<EffectInstance>>,
        scan_res: &ScanResults,
    ) -> Result<()> {
        let callers = vec![base_effect.caller().clone()];
        Self::mark_caller_checked_recurse(
            base_effect,
            tree,
            pub_caller_checked,
            scan_res,
            callers,
        )
    }

    fn recalc_pub_caller_checked_tree(
        base_effect: &EffectInstance,
        tree: &EffectTree,
        pub_caller_checked: &mut HashMap<CanonicalPath, HashSet<EffectInstance>>,
        pub_fns: &HashSet<CanonicalPath>,
    ) {
        match tree {
            EffectTree::Leaf(info, SafetyAnnotation::CallerChecked)
            | EffectTree::Leaf(info, SafetyAnnotation::Unsafe)
            | EffectTree::Leaf(info, SafetyAnnotation::Skipped) => {
                if pub_fns.contains(&info.caller_path) {
                    pub_caller_checked
                        .get_mut(&info.caller_path)
                        .unwrap()
                        .insert(base_effect.clone());
                }
            }
            EffectTree::Leaf(_, SafetyAnnotation::Safe) => (),
            EffectTree::Branch(info, next_trees) => {
                if pub_fns.contains(&info.caller_path) {
                    pub_caller_checked
                        .get_mut(&info.caller_path)
                        .unwrap()
                        .insert(base_effect.clone());
                }
                for t in next_trees {
                    AuditFile::recalc_pub_caller_checked_tree(
                        base_effect,
                        t,
                        pub_caller_checked,
                        pub_fns,
                    );
                }
            }
        }
    }

    /// Recalculate the list of public functions that should be marked caller-
    /// checked. This should always be done before a `AuditFile` is saved to
    /// disk, because it assumes the invariant that the list in
    /// `pub_caller_checked` aligns with those in the effect tree.
    pub fn recalc_pub_caller_checked(&mut self, pub_fns: &HashSet<CanonicalPath>) {
        let mut pub_caller_checked =
            HashMap::from_iter(pub_fns.iter().map(|p| (p.clone(), HashSet::new())));
        for (effect, tree) in self.audit_trees.iter() {
            AuditFile::recalc_pub_caller_checked_tree(
                effect,
                tree,
                &mut pub_caller_checked,
                pub_fns,
            );
        }

        self.pub_caller_checked = pub_caller_checked;
    }

    /// Returns the list of all safe public functions (these include all the
    /// public functions which have been removed since the last audit update).
    pub fn safe_pub_fns(&self) -> HashSet<CanonicalPath> {
        self.pub_caller_checked
            .iter()
            .filter_map(
                |(path, set)| {
                    if set.is_empty() {
                        Some(path.clone())
                    } else {
                        None
                    }
                },
            )
            .collect()
    }

    /// Removes any effect trees which have the given sink as the root. Returns
    /// the removed effects.
    pub fn remove_sinks_from_tree(
        &mut self,
        sinks_to_remove: &HashSet<CanonicalPath>,
    ) -> Vec<EffectInstance> {
        // Replace the audit tree with a temporary value so we can use a filter
        // map to drop effects
        let audit_trees = std::mem::take(&mut self.audit_trees);
        #[allow(clippy::type_complexity)]
        let (new_trees, removed_effects): (
            Vec<Option<(EffectInstance, EffectTree)>>,
            Vec<Vec<EffectInstance>>,
        ) = audit_trees
            .into_iter()
            .map(|(e, tree)| {
                // Remove effects that match our sinks to remove
                if let Effect::SinkCall(s) = e.eff_type() {
                    if sinks_to_remove.contains(&CanonicalPath::new(s.as_str())) {
                        (None, vec![e])
                    } else {
                        (Some((e, tree)), vec![])
                    }
                } else {
                    (Some((e, tree)), vec![])
                }
            })
            .unzip();
        let new_trees = new_trees.into_iter().flatten();
        self.audit_trees = new_trees.collect::<HashMap<_, _>>();
        removed_effects.into_iter().flatten().collect::<Vec<_>>()
    }

    pub fn new_caller_checked_default(
        crate_path: &FilePath,
        relevant_effects: &[EffectType],
    ) -> Result<AuditFile> {
        Self::new_caller_checked_default_with_sinks(
            crate_path,
            HashSet::new(),
            relevant_effects,
        )
    }

    pub fn new_caller_checked_default_with_sinks(
        crate_path: &FilePath,
        sinks: HashSet<CanonicalPath>,
        relevant_effects: &[EffectType],
    ) -> Result<AuditFile> {
        let mut audit_file = AuditFile::empty(crate_path.to_path_buf(), relevant_effects.to_vec())?;
        let ident_sinks =
            sinks.iter().map(|x| x.clone().to_path()).collect::<HashSet<_>>();
        let scan_res =
            scanner::scan_crate_with_sinks(crate_path, ident_sinks, relevant_effects)?;
        let mut pub_caller_checked = HashMap::new();
        audit_file.set_base_audit_trees(scan_res.effects_set());

        for (e, t) in audit_file.audit_trees.iter_mut() {
            AuditFile::mark_caller_checked(e, t, &mut pub_caller_checked, &scan_res)?;
        }

        audit_file.pub_caller_checked = pub_caller_checked;

        Ok(audit_file)
    }

    pub fn new_empty_default_with_sinks(
        crate_path: &FilePath,
        sinks: HashSet<CanonicalPath>,
        relevant_effects: &[EffectType],
    ) -> Result<AuditFile> {
        let mut audit_file = AuditFile::empty(crate_path.to_path_buf(), relevant_effects.to_vec())?;
        let ident_sinks =
            sinks.iter().map(|x| x.clone().to_path()).collect::<HashSet<_>>();
        let scan_res =
            scanner::scan_crate_with_sinks(crate_path, ident_sinks, relevant_effects)?;
        audit_file.set_base_audit_trees(scan_res.effects_set());

        Ok(audit_file)
    }

    pub fn new_default_with_sinks(
        crate_path: &FilePath,
        sinks: HashSet<CanonicalPath>,
        audit_type: DefaultAuditType,
        relevant_effects: &[EffectType],
    ) -> Result<AuditFile> {
        match audit_type {
            DefaultAuditType::CallerChecked => {
                Self::new_caller_checked_default_with_sinks(
                    crate_path,
                    sinks,
                    relevant_effects,
                )
            }
            DefaultAuditType::Empty => {
                Self::new_empty_default_with_sinks(crate_path, sinks, relevant_effects)
            }
            // TODO: belo
            DefaultAuditType::Safe => unimplemented!(),
        }
    }

    /// Gets the difference between the public functions marked caller-checked
    /// in `p1` and `p2`
    pub fn pub_diff(p1: &AuditFile, p2: &AuditFile) -> HashSet<CanonicalPath> {
        p1.pub_caller_checked
            .keys()
            .cloned()
            .collect::<HashSet<_>>()
            .difference(&p2.pub_caller_checked.keys().cloned().collect::<HashSet<_>>())
            .cloned()
            .collect::<HashSet<CanonicalPath>>()
    }
}
