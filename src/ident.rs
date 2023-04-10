/*
    Rust identifiers, paths, and patterns

    Ident: std, fs, File
    Path: std, std::fs, fs::File, super::fs::File, std::fs::File
    CanonicalPath: crate::fs::File
    Pattern: std::fs, std::fs::*
*/

use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

use super::util::iter::FreshIter;

fn replace_hyphens(s: &mut String) {
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
        c.is_ascii_alphanumeric() || c == '_' || c == '[' || c == ']'
    }

    pub fn invariant(&self) -> bool {
        self.0.chars().all(Self::char_ok) && !self.0.is_empty()
    }

    pub fn check_invariant(&self) {
        if !self.invariant() {
            eprintln!("Warning: failed invariant! on Ident {}", self);
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
pub struct Path(String);
impl Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Path {
    fn char_ok(c: char) -> bool {
        // Path currently allows [] to specify some cases we don't handle,
        // like [METHOD]::foo, [FIELD]::foo, etc.
        c.is_ascii_alphanumeric() || c == '_' || c == '[' || c == ']' || c == ':'
    }

    pub fn invariant(&self) -> bool {
        self.0.chars().all(Self::char_ok)
        // TBD check :: are adjacent
    }

    pub fn check_invariant(&self) {
        if !self.invariant() {
            eprintln!("Warning: failed invariant! on Path {}", self);
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

impl Default for Path {
    fn default() -> Self {
        Self::new_empty()
    }
}

/// Type representing a *canonical* Path identifier,
/// i.e. from the root
/// Should not be empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CanonicalPath(Path);
impl Display for CanonicalPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl CanonicalPath {
    fn char_ok(c: char) -> bool {
        // CanonicalPath does not allow []
        c.is_ascii_alphanumeric() || c == '_' || c == ':'
    }

    pub fn invariant(&self) -> bool {
        self.0 .0.chars().all(Self::char_ok) && !self.0.is_empty()
    }

    pub fn check_invariant(&self) {
        if !self.invariant() {
            eprintln!("Warning: failed invariant! on CanonicalPath {}", self);
        }
    }

    pub fn new(s: &str) -> Self {
        Self::from_path(Path::new(s))
    }

    pub fn new_owned(s: String) -> Self {
        Self::from_path(Path::new_owned(s))
    }

    pub fn from_path(p: Path) -> Self {
        let result = Self(p);
        result.check_invariant();
        result
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

    pub fn append_path(&mut self, other: &Path) {
        self.0.append(other);
        self.check_invariant();
    }

    pub fn crate_name(&self) -> Ident {
        self.0.idents().next().unwrap()
    }

    pub fn to_path(self) -> Path {
        self.0
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

/// Type representing a pattern over Paths
///
/// Currently supported: only patterns of the form
/// <path>::* (includes <path> itself)
/// The ::* is left implicit and should not be provided
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pattern(Path);
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
            eprintln!("Warning: failed invariant! on Pattern {}", self);
        }
    }

    pub fn new(s: &str) -> Self {
        Self::from_path(Path::new(s))
    }

    pub fn new_owned(s: String) -> Self {
        Self::from_path(Path::new_owned(s))
    }

    pub fn from_ident(i: Ident) -> Self {
        Self::from_path(Path::from_ident(i))
    }

    pub fn from_path(p: Path) -> Self {
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
        let p = Path::new("std::fs");
        let pats: Vec<Pattern> = p.patterns().collect();
        let pat1 = Pattern::new("std");
        let pat2 = Pattern::new("std::fs");
        assert_eq!(pats, vec![pat1, pat2])
    }

    #[test]
    fn test_path_matches() {
        let p = Path::new("std::fs");
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
