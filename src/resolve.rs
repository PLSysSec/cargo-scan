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
    /*
        Constructor and invariant
    */
    fn new(crt: &FilePath) -> Result<Self>;
    fn assert_top_level_invariant(&self);

    /*
        Resolution functions
    */
    fn resolve_ident(&self, i: &'a syn::Ident) -> Path;
    fn resolve_path(&self, p: &'a syn::Path) -> Path;
    fn resolve_def(&self, p: &'a syn::Ident) -> CanonicalPath;
    fn resolve_ffi(&self, ffi: &syn::Path) -> Option<CanonicalPath>;

    /*
        Optional helper functions to inform the resolver of the scope
    */
    fn push_mod(&mut self, mod_ident: &'a syn::Ident);
    fn pop_mod(&mut self);
    fn push_impl(&mut self, impl_stmt: &'a syn::ItemImpl);
    fn pop_impl(&mut self);
    fn push_fn(&mut self, fn_ident: &'a syn::Ident);
    fn pop_fn(&mut self);
    fn scan_use(&mut self, use_stmt: &'a syn::ItemUse);
    fn scan_foreign_fn(&mut self, f: &'a syn::ForeignItemFn);
}
