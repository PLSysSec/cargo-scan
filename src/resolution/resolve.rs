//! Interface for name resolution for Rust identifiers.
//!
//! The type FileResolver is a wrapper around ResolverImpl from name_resolution.rs
//! with the needed functionality.

use crate::resolution::name_resolution::{Resolver, ResolverImpl};

use crate::effect::SrcLoc;
use super::hacky_resolver::HackyResolver;
use crate::ident::{CanonicalPath, CanonicalType, Ident};

use anyhow::Result;
use log::{debug, warn};
use std::fmt::Display;
use std::path::Path as FilePath;
use syn::{self, spanned::Spanned};

/*
    Conversion functions from syn to internal ident data model
*/

pub fn ident_from_syn(i: &syn::Ident) -> Ident {
    Ident::new_owned(i.to_string())
}

/// Common interface for FileResolver and HackyResolver
///
/// Abstracts the functionality for resolution that is needed by Scanner.
pub trait Resolve<'a>: Sized {
    /*
        Constructor and invariant
    */
    fn assert_top_level_invariant(&self);

    /*
        Function resolution
    */
    fn resolve_ident(&self, i: &'a syn::Ident) -> CanonicalPath;
    fn resolve_method(&self, i: &'a syn::Ident) -> CanonicalPath;
    fn resolve_path(&self, p: &'a syn::Path) -> CanonicalPath;
    fn resolve_def(&self, i: &'a syn::Ident) -> CanonicalPath;
    fn resolve_ffi(&self, p: &'a syn::Path) -> Option<CanonicalPath>;
    fn resolve_ffi_ident(&self, i: &'a syn::Ident) -> Option<CanonicalPath>;
    fn resolve_unsafe_path(&self, p: &'a syn::Path) -> bool;
    fn resolve_unsafe_ident(&self, p: &'a syn::Ident) -> bool;
    fn resolve_all_impl_methods(&self, i: &'a syn::Ident) -> Vec<CanonicalPath>;

    /*
        Field and expression resolution
    */
    fn resolve_field(&self, i: &syn::Ident) -> CanonicalPath;
    fn resolve_field_index(&self, idx: &'a syn::Index) -> CanonicalPath;
    fn resolve_closure(&self, cl: &'a syn::ExprClosure) -> CanonicalPath;
    fn resolve_const_or_static(&self, p: &'a syn::Path) -> bool;

    /*
        Type resolution
    */
    fn resolve_path_type(&self, i: &'a syn::Path) -> CanonicalType;
    fn resolve_field_type(&self, i: &syn::Ident) -> CanonicalType;

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
    resolver: ResolverImpl<'a>,
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
        let imp = ResolverImpl::new(resolver, filepath)?;
        Ok(Self { filepath, resolver: imp, backup })
    }

    fn resolve_core(&self, i: &syn::Ident) -> Result<CanonicalPath> {
        let mut s = SrcLoc::from_span(self.filepath, i);
        debug!("Resolving: {} ({})", i, s);
        // Add 1 to column to avoid weird off-by-one errors
        s.add1();
        let i = ident_from_syn(i);
        self.resolver.resolve_ident(s, i)
    }

    fn resolve_ffi_core(&self, i: &syn::Ident) -> Result<Option<CanonicalPath>> {
        let mut s = SrcLoc::from_span(self.filepath, i);
        debug!("Resolving FFI: {} ({})", i, s);
        // Add 1 to column to avoid weird off-by-one errors
        s.add1();
        let i_owned = ident_from_syn(i);
        if self.resolver.is_ffi(s, i_owned)? {
            Ok(Some(self.resolve_core(i)?))
        } else {
            Ok(None)
        }
    }

    fn resolve_unsafe_core(&self, i: &syn::Ident) -> Result<bool> {
        let mut s = SrcLoc::from_span(self.filepath, i);
        debug!("Resolving Unsafe Call: {} ({})", i, s);
        // Add 1 to column to avoid weird off-by-one errors
        s.add1();
        let i_owned = ident_from_syn(i);
        if self.resolver.is_unsafe_call(s, i_owned)? {
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn resolve_type_core(&self, i: &syn::Ident) -> Result<CanonicalType> {
        let mut s = SrcLoc::from_span(self.filepath, i);
        debug!("Resolving type: {} ({})", i, s);
        // Add 1 to column to avoid weird off-by-one errors
        s.add1();
        let i = ident_from_syn(i);
        self.resolver.resolve_type(s, i)
    }

    fn resolve_const_or_static_core(&self, i: &syn::Ident) -> Result<bool> {
        let mut s = SrcLoc::from_span(self.filepath, i);
        debug!("Resolving const or immutable static: {} ({})", i, s);
        // Add 1 to column to avoid weird off-by-one errors
        s.add1();
        let i = ident_from_syn(i);
        self.resolver.is_const_or_immutable_static_ident(s, i)
    }

    fn resolve_all_impl_methods_core(
        &self,
        i: &syn::Ident,
    ) -> Result<Vec<CanonicalPath>> {
        let mut s = SrcLoc::from_span(self.filepath, i);
        debug!("Resolving all impl methods for trait: {}", i);
        // Add 1 to column to avoid weird off-by-one errors
        s.add1();
        let i = ident_from_syn(i);
        self.resolver.all_impl_methods_for_trait(s, i)
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
            debug!("Resolution failed (using fallback) for: {} ({}) ({})", i, s, err);
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

    fn resolve_path_type(&self, p: &'a syn::Path) -> CanonicalType {
        let i = &p.segments.last().unwrap().ident;
        self.resolve_type_or_else(i, || self.backup.resolve_path_type(p))
    }

    fn resolve_def(&self, i: &'a syn::Ident) -> CanonicalPath {
        self.resolve_ident_or_else(i, || self.backup.resolve_def(i))
    }

    fn resolve_ffi_ident(&self, i: &syn::Ident) -> Option<CanonicalPath> {
        self.resolve_or_else(
            i,
            || self.resolve_ffi_core(i),
            || self.backup.resolve_ffi_ident(i),
        )
    }

    fn resolve_ffi(&self, p: &syn::Path) -> Option<CanonicalPath> {
        let i = &p.segments.last().unwrap().ident;
        self.resolve_ffi_ident(i)
    }

    fn resolve_unsafe_path(&self, p: &syn::Path) -> bool {
        let i = &p.segments.last().unwrap().ident;
        self.resolve_or_else(
            i,
            || self.resolve_unsafe_core(i),
            || self.backup.resolve_unsafe_path(p),
        )
    }

    fn resolve_unsafe_ident(&self, i: &syn::Ident) -> bool {
        self.resolve_or_else(
            i,
            || self.resolve_unsafe_core(i),
            || self.backup.resolve_unsafe_ident(i),
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

    fn resolve_field(&self, i: &syn::Ident) -> CanonicalPath {
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

    fn resolve_field_type(&self, i: &syn::Ident) -> CanonicalType {
        self.resolve_type_or_else(i, || self.backup.resolve_field_type(i))
    }

    fn resolve_closure(&self, cl: &'a syn::ExprClosure) -> CanonicalPath {
        let s = SrcLoc::from_span(self.filepath, cl);
        debug!("Skipping closure resolution (using fallback) for {:?} ({})", cl, s);
        self.backup.resolve_closure(cl)
    }

    fn resolve_const_or_static(&self, p: &'a syn::Path) -> bool {
        let i = &p.segments.last().unwrap().ident;
        self.resolve_or_else(
            i,
            || self.resolve_const_or_static_core(i),
            || self.backup.resolve_const_or_static(p),
        )
    }

    fn resolve_all_impl_methods(&self, i: &'a syn::Ident) -> Vec<CanonicalPath> {
        self.resolve_or_else(
            i,
            || self.resolve_all_impl_methods_core(i),
            || self.backup.resolve_all_impl_methods(i),
        )
    }
}
