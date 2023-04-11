//! Defines an interface for a resolver for Rust identifiers.
//!
//! Instantiated by:
//! - Resolver in name_resolution.rs

use super::ident::{CanonicalPath, Ident, Path};

use anyhow::Result;
use std::path::Path as FilePath;
use syn;

pub trait Resolve: Sized {
    fn new(crt: &FilePath) -> Result<Self>;
    fn push_scope(&mut self);
    fn pop_scope(&mut self);
    fn scan_use(&mut self, use_path: &syn::UsePath);
    fn resolve_path(&self, p: &Path) -> CanonicalPath;
    fn resolve_ident(&self, i: &Ident) -> CanonicalPath;
}

#[derive(Debug)]
pub struct HackyResolver();

impl Resolve for HackyResolver {
    fn new(_crt: &FilePath) -> Result<Self> {
        Ok(Self())
    }
    fn push_scope(&mut self) {}
    fn pop_scope(&mut self) {}
    fn scan_use(&mut self, _use_path: &syn::UsePath) {}
    fn resolve_path(&self, p: &Path) -> CanonicalPath {
        CanonicalPath::from_path(p.clone())
    }
    fn resolve_ident(&self, i: &Ident) -> CanonicalPath {
        CanonicalPath::from_path(Path::from_ident(i.clone()))
    }
}
