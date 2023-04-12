//! Defines an interface for a resolver for Rust identifiers.
//!
//! Instantiated by:
//! - Resolver in name_resolution.rs
//! - HackyResolver in hacky_resolver.rs

use super::ident::{CanonicalPath, Path};

use anyhow::Result;
use std::path::Path as FilePath;
use syn;

pub trait Resolve<'a>: Sized {
    fn new(crt: &FilePath) -> Result<Self>;
    fn assert_invariant(&self);
    fn push_scope(&mut self);
    fn pop_scope(&mut self);
    fn scan_use(&mut self, use_stmt: &'a syn::ItemUse);
    fn resolve_ident(&self, i: &'a syn::Ident) -> Path;
    fn resolve_path(&self, p: &'a syn::Path) -> Path;
    fn resolve_ident_canonical(&self, i: &'a syn::Ident) -> CanonicalPath;
    fn resolve_path_canonical(&self, i: &'a syn::Path) -> CanonicalPath;
    fn get_modpath(&self) -> CanonicalPath;
    fn lookup_path_vec(&self, p: &'a syn::Path) -> Vec<&'a syn::Ident>;
}
