//! Rust identifiers, paths, and patterns.
//!
//! Ident: std, fs, File
//! IdentPath: std, std::fs, fs::File, super::fs::File, std::fs::File
//! CanonicalPath: crate::fs::File
//! Pattern: std::fs, std::fs::*

use log::warn;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

use super::util::iter::FreshIter;

pub fn replace_hyphens(s: &mut String) {
    while let Some(i) = s.find('-') {
        s.replace_range(i..(i + 1), "_");
    }
}

#[test]
fn test_replace_hyphens() {
    let mut s1 = "abcd_efgh_ijkl".to_string();
    let mut s2 = "abcd-efgh_ijkl".to_string();
    let mut s3 = "abcd-efgh-ijkl".to_string();
    let s = s1.clone();
    replace_hyphens(&mut s1);
    replace_hyphens(&mut s2);
    replace_hyphens(&mut s3);
    assert_eq!(s1, s);
    assert_eq!(s2, s);
    assert_eq!(s3, s);
}

/// An Rust name identifier, without colons
/// E.g.: env
/// Should be a nonempty string
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ident(String);
impl Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Ident {
    fn char_ok(c: char) -> bool {
        c.is_ascii_alphanumeric() || "_.'&,:;*!+={}[]()<> ".contains(c)
    }

    fn str_ok(s: &str) -> bool {
        let skips = if s.starts_with("r#") {
            // raw ident, e.g. r#try
            2
        } else if s.starts_with('\'') {
            // Lifetime ident, e.g. 'a
            1
        } else {
            0
        };
        s.chars().skip(skips).all(Self::char_ok) && !s.is_empty()
    }

    pub fn invariant(&self) -> bool {
        Self::str_ok(&self.0)
    }

    pub fn check_invariant(&self) {
        if !self.invariant() {
            warn!("failed invariant! on Ident {}", self);
        }
    }

    pub fn new(s: &str) -> Self {
        Self::new_owned(s.to_string())
    }

    pub fn new_owned(s: String) -> Self {
        let mut result = Self(s);
        replace_hyphens(&mut result.0);
        result.check_invariant();
        result
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A Rust path identifier, with colons
/// E.g.: std::env::var_os
/// Semantically a (possibly empty) sequence of Idents
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdentPath(String);
impl Display for IdentPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl IdentPath {
    pub fn invariant(&self) -> bool {
        self.0.is_empty() || self.0.split("::").all(Ident::str_ok)
    }

    pub fn check_invariant(&self) {
        if !self.invariant() {
            warn!("failed invariant! on IdentPath {}", self);
        }
    }

    pub fn new(s: &str) -> Self {
        Self::new_owned(s.to_string())
    }

    pub fn new_owned(s: String) -> Self {
        let mut result = Self(s);
        replace_hyphens(&mut result.0);
        result.check_invariant();
        result
    }

    pub fn new_empty() -> Self {
        Self::new("")
    }

    pub fn from_ident(i: Ident) -> Self {
        let result = Self(i.0);
        result.check_invariant();
        result
    }

    pub fn from_idents(is: impl Iterator<Item = Ident>) -> Self {
        let mut result = Self::new_empty();
        for i in is {
            result.push_ident(&i)
        }
        result.check_invariant();
        result
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push_ident(&mut self, i: &Ident) {
        if !self.is_empty() {
            self.0.push_str("::");
        }
        self.0.push_str(i.as_str());
        self.check_invariant();
    }

    pub fn pop_ident(&mut self) -> Option<Ident> {
        let (s1, s2) = self.0.rsplit_once("::")?;
        let result = Ident::new(s2);
        self.0 = s1.to_string();
        self.check_invariant();
        Some(result)
    }

    pub fn last_ident(&self) -> Option<Ident> {
        let (_, i) = self.0.rsplit_once("::")?;
        Some(Ident::new(i))
    }

    pub fn first_ident(&self) -> Option<Ident> {
        let (i, _) = self.0.split_once("::")?;
        Some(Ident::new(i))
    }

    pub fn append(&mut self, other: &Self) {
        if !other.is_empty() {
            if !self.is_empty() {
                self.0.push_str("::");
            }
            self.0.push_str(other.as_str());
            self.check_invariant();
        }
    }

    /// Iterator over identifiers in the path
    pub fn idents(&self) -> impl Iterator<Item = Ident> + '_ {
        self.0.split("::").map(Ident::new)
    }

    /// O(n) length check
    pub fn len(&self) -> usize {
        self.idents().count()
    }

    /// Iterator over patterns *which match* the path
    /// Current implementation using FreshIter
    pub fn patterns(&self) -> impl Iterator<Item = Pattern> {
        let mut result = String::new();
        let mut results = Vec::new();
        let mut first = true;
        for id in self.idents() {
            if first {
                first = false;
            } else {
                result.push_str("::");
            }
            result.push_str(&id.0);
            results.push(Pattern::new(&result));
        }
        results.drain(..).fresh_iter()
    }

    pub fn matches(&self, pattern: &Pattern) -> bool {
        self.0.starts_with(pattern.as_str())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for IdentPath {
    fn default() -> Self {
        Self::new_empty()
    }
}

/// Type representing a *canonical* path of Rust idents.
/// i.e. from the root
/// Should not be empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CanonicalPath(IdentPath);

impl Display for CanonicalPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl CanonicalPath {
    pub fn invariant(&self) -> bool {
        self.0.invariant() && !self.0.is_empty()
    }

    pub fn check_invariant(&self) {
        if !self.invariant() {
            warn!("failed invariant! on CanonicalPath {}", self);
        }
    }

    pub fn new(s: &str) -> Self {
        Self::from_path(IdentPath::new(s))
    }

    pub fn new_owned(s: String) -> Self {
        Self::from_path(IdentPath::new_owned(s))
    }

    pub fn from_path(p: IdentPath) -> Self {
        let result = Self(p);
        result.check_invariant();
        result
    }

    /// Returns true if the CanonicalPath is the top-level `main` function
    pub fn is_main(&self) -> bool {
        let Some((s1, s2)) = self.0 .0.rsplit_once("::") else {
            return false;
        };
        s2 == "main" && s1 == self.crate_name().as_str()
    }

    pub fn push_ident(&mut self, i: &Ident) {
        self.0.push_ident(i);
        self.check_invariant();
    }

    pub fn pop_ident(&mut self) -> Option<Ident> {
        let result = self.0.pop_ident();
        self.check_invariant();
        result
    }

    pub fn append_path(&mut self, other: &IdentPath) {
        self.0.append(other);
        self.check_invariant();
    }

    pub fn crate_name(&self) -> Ident {
        self.0.idents().next().unwrap()
    }

    pub fn to_path(self) -> IdentPath {
        self.0
    }

    pub fn as_path(&self) -> &IdentPath {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    // NOTE: The matches definition should align with whatever definition format
    //       we use for our default sinks
    pub fn matches(&self, pattern: &Pattern) -> bool {
        self.0.matches(pattern)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TypeKind {
    RawPointer,
    UnionFld,
    StaticMut,
    Function,
    #[default]
    // Default case for types that we
    // don't need extra information about.
    Plain,
}

impl Display for TypeKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            TypeKind::RawPointer => "raw pointer",
            TypeKind::UnionFld => "union field",
            TypeKind::StaticMut => "mutable static",
            TypeKind::Function => "function",
            TypeKind::Plain => "plain",
        };
        write!(f, "{}", s)
    }
}

/// Type representing a type identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct CanonicalType(TypeKind);

impl Display for CanonicalType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl CanonicalType {
    pub fn new(k: TypeKind) -> Self {
        Self(k)
    }

    pub fn is_raw_ptr(&self) -> bool {
        matches!(self.0, TypeKind::RawPointer)
    }

    pub fn is_union_field(&self) -> bool {
        matches!(self.0, TypeKind::UnionFld)
    }

    pub fn is_mut_static(&self) -> bool {
        matches!(self.0, TypeKind::StaticMut)
    }

    pub fn is_function(&self) -> bool {
        matches!(self.0, TypeKind::Function)
    }
}

/// Type representing a pattern over paths
///
/// Currently supported: only patterns of the form
/// <path>::* (includes <path> itself)
/// The ::* is left implicit and should not be provided
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pattern(IdentPath);
impl Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Pattern {
    pub fn invariant(&self) -> bool {
        self.0.invariant()
    }

    pub fn check_invariant(&self) {
        if !self.invariant() {
            warn!("failed invariant! on Pattern {}", self);
        }
    }

    pub fn new(s: &str) -> Self {
        Self::from_path(IdentPath::new(s))
    }

    pub fn new_owned(s: String) -> Self {
        Self::from_path(IdentPath::new_owned(s))
    }

    pub fn from_ident(i: Ident) -> Self {
        Self::from_path(IdentPath::from_ident(i))
    }

    pub fn first_ident(&self) -> Option<Ident> {
        self.0.first_ident()
    }

    pub fn from_path(p: IdentPath) -> Self {
        let result = Self(p);
        result.check_invariant();
        result
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Return true if the set of paths denoted by self is
    /// a subset of those denoted by other
    pub fn subset(&self, other: &Self) -> bool {
        self.0.matches(other)
    }

    /// Return true if the set of paths denoted by self is
    /// a superset of those denoted by other
    pub fn superset(&self, other: &Self) -> bool {
        other.subset(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_patterns() {
        let p = IdentPath::new("std::fs");
        let pats: Vec<Pattern> = p.patterns().collect();
        let pat1 = Pattern::new("std");
        let pat2 = Pattern::new("std::fs");
        assert_eq!(pats, vec![pat1, pat2])
    }

    #[test]
    fn test_path_matches() {
        let p = IdentPath::new("std::fs");
        let pat1 = Pattern::new("std");
        let pat2 = Pattern::new("std::fs");
        let pat3 = Pattern::new("std::fs::File");
        let pat4 = Pattern::new("std::os");
        assert!(p.matches(&pat1));
        assert!(p.matches(&pat2));
        assert!(!p.matches(&pat3));
        assert!(!p.matches(&pat4));
    }

    #[test]
    fn test_pattern_subset_superset() {
        let pat1 = Pattern::new("std");
        let pat2 = Pattern::new("std::fs");
        let pat3 = Pattern::new("std::fs::File");
        let pat4 = Pattern::new("std::os");

        assert!(pat1.superset(&pat2));
        assert!(pat1.superset(&pat3));
        assert!(pat2.superset(&pat3));

        assert!(pat2.subset(&pat1));
        assert!(pat3.subset(&pat1));
        assert!(pat3.subset(&pat2));

        assert!(pat1.subset(&pat1));
        assert!(pat2.subset(&pat2));
        assert!(pat4.subset(&pat4));

        assert!(!pat1.subset(&pat2));
        assert!(!pat2.subset(&pat4));
        assert!(!pat4.subset(&pat2));
    }
}
