/*
    Rust identifiers, paths, and patterns

    Ident: std, fs, File
    Path: std::fs::File
    Pattern: std::fs, std::fs::*
*/

use std::fmt::{self, Display};

/// An Rust name identifier, without colons
/// E.g.: env
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident(String);
impl Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Ident {
    fn char_ok(c: char) -> bool {
        c.is_ascii_alphanumeric()
    }
    pub fn invariant(&self) -> bool {
        self.0.chars().all(Self::char_ok)
    }
    pub fn new(s: &str) -> Self {
        let result = Self(s.to_string());
        debug_assert!(result.invariant());
        result
    }
}

/// A Rust path identifier, with colons
/// E.g.: std::env::var_os
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path(String);
impl Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Path {
    fn char_ok(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == ':'
    }
    pub fn invariant(&self) -> bool {
        self.0.chars().all(Self::char_ok)
        // TBD check :: are adjacent
    }
    pub fn new(s: &str) -> Self {
        let result = Self(s.to_string());
        debug_assert!(result.invariant());
        result
    }
    pub fn idents(&self) -> impl Iterator<Item = Ident> + '_ {
        self.0.split("::").map(|s| Ident::new(s))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pattern(String);
impl Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Pattern {
    fn char_ok(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == ':' || c == '*'
    }
    pub fn invariant(&self) -> bool {
        self.0.chars().all(Self::char_ok)
        // TBD check :: are adjacent
    }
    pub fn new(s: &str) -> Self {
        let result = Self(s.to_string());
        debug_assert!(result.invariant());
        result
    }
}
