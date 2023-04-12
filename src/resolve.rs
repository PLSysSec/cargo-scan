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
    fn assert_top_level_invariant(&self);

    /*
        Resolution functions
    */
    fn resolve_ident(&self, i: &'a syn::Ident) -> Path;
    fn resolve_path(&self, p: &'a syn::Path) -> Path;
    fn resolve_def(&self, i: &'a syn::Ident) -> CanonicalPath;
    fn resolve_ffi(&self, p: &syn::Path) -> Option<CanonicalPath>;

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

use super::effect::SrcLoc;
use super::ident::Ident;
use super::name_resolution::Resolver;

use std::path::PathBuf as FilePathBuf;

pub struct RAResolver {
    inner: Resolver,
    filepath: FilePathBuf,
}

impl RAResolver {
    pub fn new(crt: &FilePath) -> Result<Self> {
        // eprintln!("creating with crate: {:?}", crt);
        let inner = Resolver::new(crt)?;
        let filepath = crt.to_owned();
        Ok(Self { inner, filepath })
    }
    pub fn set_file(&mut self, new_file: &FilePath) {
        self.filepath = new_file.to_owned();
    }
    fn resolve_core<'a>(&self, i: &'a syn::Ident) -> CanonicalPath {
        let s = SrcLoc::from_span(&self.filepath, i);
        let i = Ident::from_syn(i);
        self.inner.resolve_ident(s.clone(), i.clone()).unwrap_or_else(|err| {
            panic!("Resolution failed: {:?} {} ({:?})", s, i, err);
        })
    }
}

impl<'a> Resolve<'a> for RAResolver {
    fn assert_top_level_invariant(&self) {}

    /*
        Resolution functions
    */
    fn resolve_ident(&self, i: &'a syn::Ident) -> Path {
        self.resolve_core(i).to_path()
    }
    fn resolve_path(&self, p: &'a syn::Path) -> Path {
        let i = &p.segments.last().unwrap().ident;
        self.resolve_core(i).to_path()
    }
    fn resolve_def(&self, i: &'a syn::Ident) -> CanonicalPath {
        self.resolve_core(i)
    }
    fn resolve_ffi(&self, _p: &syn::Path) -> Option<CanonicalPath> {
        // TODO
        None
    }

    /*
        Optional helper functions to inform the resolver of the scope
    */
    fn push_mod(&mut self, _mod_ident: &'a syn::Ident) {}
    fn pop_mod(&mut self) {}
    fn push_impl(&mut self, _impl_stmt: &'a syn::ItemImpl) {}
    fn pop_impl(&mut self) {}
    fn push_fn(&mut self, _fn_ident: &'a syn::Ident) {}
    fn pop_fn(&mut self) {}
    fn scan_use(&mut self, _use_stmt: &'a syn::ItemUse) {}
    fn scan_foreign_fn(&mut self, _f: &'a syn::ForeignItemFn) {}
}
