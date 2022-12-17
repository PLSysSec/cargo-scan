/*
    Rust identifiers, paths, and patterns

    Ident: std, fs, File
    Path: std::fs::File
    Pattern: std::fs, std::fs::*

    TBD:
    - This file doesn't allow - in identifiers. Consider allowing and
      converting to _
*/

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{self, Display};
use std::str::FromStr;

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
        c.is_ascii_alphanumeric() || c == '_' || c == '[' || c == ']'
    }
    pub fn invariant(&self) -> bool {
        self.0.chars().all(Self::char_ok)
    }

    pub fn new(s: &str) -> Self {
        Self::new_owned(s.to_string())
    }
    pub fn new_owned(s: String) -> Self {
        let result = Self(s);
        debug_assert!(result.invariant());
        result
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A Rust path identifier, with colons
/// E.g.: std::env::var_os
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Path(pub String);
impl Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Path {
    fn char_ok(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_' || c == '[' || c == ']' || c == ':'
    }
    pub fn invariant(&self) -> bool {
        self.0.chars().all(Self::char_ok)
        // TBD check :: are adjacent
    }

    pub fn new(s: &str) -> Self {
        Self::new_owned(s.to_string())
    }
    pub fn new_owned(s: String) -> Self {
        let result = Self(s);
        debug_assert!(result.invariant());
        result
    }
    pub fn from_ident(i: Ident) -> Self {
        let result = Self(i.0);
        debug_assert!(result.invariant());
        result
    }

    pub fn idents(&self) -> impl Iterator<Item = Ident> + '_ {
        self.0.split("::").map(Ident::new)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Type representing a pattern over Paths
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pattern(String);
impl Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Pattern {
    fn char_ok(c: char) -> bool {
        c.is_ascii_alphanumeric()
            || c == '_'
            || c == '['
            || c == ']'
            || c == ':'
            || c == '*'
    }
    pub fn invariant(&self) -> bool {
        self.0.chars().all(Self::char_ok)
        // TBD check :: are adjacent
    }

    pub fn new(s: &str) -> Self {
        Self::new_owned(s.to_string())
    }
    pub fn new_owned(s: String) -> Self {
        let result = Self(s);
        debug_assert!(result.invariant());
        result
    }
    pub fn from_ident(i: Ident) -> Self {
        let result = Self(i.0);
        debug_assert!(result.invariant());
        result
    }
    pub fn from_path(p: Path) -> Self {
        let result = Self(p.0);
        debug_assert!(result.invariant());
        result
    }

    pub fn into_path(&self) -> Path {
        // TBD: generalize this
        Path::new(&self.0.replace("::*", ""))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Type representing an Argument pattern like (x: String, y: usize)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Args(pub String);
impl Display for Args {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl Args {
    pub fn new(s: &str) -> Self {
        Self::new_owned(s.to_string())
    }
    pub fn new_owned(s: String) -> Self {
        Self(s)
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Type representing a function call pattern
/// Used for Regions and Effects
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FnCall {
    pub fn_pattern: Pattern,
    pub arg_pattern: Args,
}
impl Display for FnCall {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({})", self.fn_pattern.0, self.arg_pattern.0)
    }
}
impl Serialize for FnCall {
    fn serialize<S>(&self, ser: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ser.collect_str(self)
    }
}
impl FromStr for FnCall {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (s1, s23) = s.split_once('(').ok_or("expected ( in FnCall")?;
        let (s2, s3) = s23.split_once(')').ok_or("expected ) in FnCall")?;
        if !s3.is_empty() {
            Err("expected empty string after )")
        } else if s1.is_empty() {
            Err("expected nonempty fn name")
        } else {
            Ok(Self::new(s1, s2))
        }
    }
}
impl<'de> Deserialize<'de> for FnCall {
    fn deserialize<D>(des: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(des)?.parse().map_err(de::Error::custom)
    }
}
impl FnCall {
    pub fn new(fn_pattern: &str, arg_pattern: &str) -> Self {
        Self::new_owned(fn_pattern.to_string(), arg_pattern.to_string())
    }
    pub fn new_owned(fn_pattern: String, arg_pattern: String) -> Self {
        let fn_pattern = Pattern::new_owned(fn_pattern);
        let arg_pattern = Args::new_owned(arg_pattern);
        Self { fn_pattern, arg_pattern }
    }
    pub fn new_all(s1: &str) -> Self {
        Self::new(s1, "*")
    }
    pub fn fn_pattern(&self) -> &Pattern {
        &self.fn_pattern
    }
    pub fn fn_path(&self) -> Path {
        self.fn_pattern.into_path()
    }
}
