//! Interface for name resolution for Rust identifiers.
//!
//! The type FileResolver is a wrapper around Resolver from name_resolution.rs
//! with the needed functionality.

pub use super::name_resolution::Resolver;

use super::effect::SrcLoc;
use super::hacky_resolver::HackyResolver;
use super::ident::{CanonicalPath, CanonicalType, Ident};

use anyhow::Result;
use log::{debug, info, warn};
use std::fmt::Display;
use std::path::Path as FilePath;
use syn::{self, spanned::Spanned};

/// Common interface for FileResolver and HackyResolver
///
/// Abstracts the functionality for resolution that is needed by Scanner.
pub trait Resolve<'a>: Sized {
    /*
        Constructor and invariant
    */
    fn assert_top_level_invariant(&self);

    /*
        Function resolution functions
    */
    fn resolve_ident(&self, i: &'a syn::Ident) -> CanonicalPath;
    fn resolve_method(&self, i: &'a syn::Ident) -> CanonicalPath;
    fn resolve_path(&self, p: &'a syn::Path) -> CanonicalPath;
    fn resolve_def(&self, i: &'a syn::Ident) -> CanonicalPath;
    fn resolve_ffi(&self, p: &'a syn::Path) -> Option<CanonicalPath>;

    /*
        Type resolution functions
    */
    fn resolve_field(&self, i: &'a syn::Ident) -> CanonicalPath;
    fn resolve_field_type(&self, i: &'a syn::Ident) -> CanonicalType;
    fn resolve_field_index(&self, idx: &'a syn::Index) -> CanonicalPath;

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

#[derive(Debug)]
pub struct FileResolver<'a> {
    filepath: &'a FilePath,
    resolver: &'a Resolver,
    backup: HackyResolver<'a>,
}

impl<'a> FileResolver<'a> {
    pub fn new(
        crate_name: &'a str,
        resolver: &'a Resolver,
        filepath: &'a FilePath,
    ) -> Result<Self> {
        debug!("Creating FileResolver for file: {:?}", filepath);
        let backup = HackyResolver::new(crate_name, filepath)?;
        Ok(Self { filepath, resolver, backup })
    }

    fn resolve_core(&self, i: &syn::Ident) -> Result<CanonicalPath> {
        let mut s = SrcLoc::from_span(self.filepath, i);
        debug!("Resolving: {} ({})", i, s);
        // Add 1 to column to avoid weird off-by-one errors
        s.add1();
        let i = Ident::from_syn(i);
        self.resolver.resolve_ident(s, i)
    }

    fn resolve_ffi_core(&self, i: &syn::Ident) -> Result<Option<CanonicalPath>> {
        let mut s = SrcLoc::from_span(self.filepath, i);
        debug!("Resolving FFI: {} ({})", i, s);
        // Add 1 to column to avoid weird off-by-one errors
        s.add1();
        let i_owned = Ident::from_syn(i);
        if self.resolver.is_ffi(s, i_owned)? {
            Ok(Some(self.resolve_core(i)?))
        } else {
            Ok(None)
        }
    }

    fn resolve_type_core(&self, i: &syn::Ident) -> Result<CanonicalType> {
        let mut s = SrcLoc::from_span(self.filepath, i);
        debug!("Resolving type: {} ({})", i, s);
        // Add 1 to column to avoid weird off-by-one errors
        s.add1();
        let i = Ident::from_syn(i);
        self.resolver.resolve_type(s, i)
    }

    fn resolve_or_else<S, R, F, T>(&self, i: &S, try_resolve: R, fallback: F) -> T
    where
        S: Display + Spanned,
        R: FnOnce() -> Result<T>,
        F: FnOnce() -> T,
    {
        try_resolve().unwrap_or_else(|err| {
            let s = SrcLoc::from_span(self.filepath, i);
            // Temporarily suppressing this warning.
            // TODO: Bump this back up to warn! once a fix is pushed
            info!("Resolution failed (using fallback) for: {} ({}) ({})", i, s, err);
            fallback()
        })
    }

    fn resolve_ident_or_else<F>(&self, i: &syn::Ident, fallback: F) -> CanonicalPath
    where
        F: FnOnce() -> CanonicalPath,
    {
        self.resolve_or_else(i, || self.resolve_core(i), fallback)
    }

    fn resolve_type_or_else<F>(&self, i: &syn::Ident, fallback: F) -> CanonicalType
    where
        F: FnOnce() -> CanonicalType,
    {
        self.resolve_or_else(i, || self.resolve_type_core(i), fallback)
    }
}

impl<'a> Resolve<'a> for FileResolver<'a> {
    fn assert_top_level_invariant(&self) {
        self.backup.assert_top_level_invariant();
    }

    fn resolve_ident(&self, i: &'a syn::Ident) -> CanonicalPath {
        self.resolve_ident_or_else(i, || self.backup.resolve_ident(i))
    }

    fn resolve_path(&self, p: &'a syn::Path) -> CanonicalPath {
        let i = &p.segments.last().unwrap().ident;
        self.resolve_ident_or_else(i, || self.backup.resolve_path(p))
    }

    fn resolve_def(&self, i: &'a syn::Ident) -> CanonicalPath {
        self.resolve_ident_or_else(i, || self.backup.resolve_def(i))
    }

    fn resolve_ffi(&self, p: &syn::Path) -> Option<CanonicalPath> {
        let i = &p.segments.last().unwrap().ident;
        self.resolve_or_else(
            i,
            || self.resolve_ffi_core(i),
            || self.backup.resolve_ffi(p),
        )
    }

    fn push_mod(&mut self, mod_ident: &'a syn::Ident) {
        self.backup.push_mod(mod_ident);
    }

    fn pop_mod(&mut self) {
        self.backup.pop_mod();
    }

    fn push_impl(&mut self, impl_stmt: &'a syn::ItemImpl) {
        self.backup.push_impl(impl_stmt);
    }

    fn pop_impl(&mut self) {
        self.backup.pop_impl();
    }

    fn push_fn(&mut self, fn_ident: &'a syn::Ident) {
        self.backup.push_fn(fn_ident);
    }

    fn pop_fn(&mut self) {
        self.backup.pop_fn();
    }

    fn scan_use(&mut self, use_stmt: &'a syn::ItemUse) {
        self.backup.scan_use(use_stmt);
    }

    fn scan_foreign_fn(&mut self, f: &'a syn::ForeignItemFn) {
        self.backup.scan_foreign_fn(f)
    }

    fn resolve_method(&self, i: &'a syn::Ident) -> CanonicalPath {
        self.resolve_ident_or_else(i, || self.backup.resolve_method(i))
    }

    fn resolve_field(&self, i: &'a syn::Ident) -> CanonicalPath {
        self.resolve_ident_or_else(i, || self.backup.resolve_field(i))
    }

    fn resolve_field_index(&self, idx: &'a syn::Index) -> CanonicalPath {
        let s = SrcLoc::from_span(self.filepath, idx);
        warn!(
            "Skipping function call on a field index (using fallback) for {:?} ({})",
            idx, s
        );
        self.backup.resolve_field_index(idx)
    }

    fn resolve_field_type(&self, i: &'a syn::Ident) -> CanonicalType {
        self.resolve_type_or_else(i, || self.backup.resolve_field_type(i))
    }
}
