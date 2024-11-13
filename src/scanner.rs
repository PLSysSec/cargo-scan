//! Scanner API
//!
//! Parse a Rust crate or source file and collect effect blocks, function calls, and
//! various other information.

use crate::attr_parser::CfgPred;
use crate::audit_file::EffectInfo;
use crate::resolution::hacky_resolver::HackyResolver;
use crate::resolution::name_resolution::Resolver;

use super::effect::{Effect, EffectInstance, EffectType, FnDec, SrcLoc, Visibility};
use super::ident::{CanonicalPath, IdentPath};
use super::loc_tracker::LoCTracker;
use super::sink::Sink;
use super::util;
use crate::resolution::resolve::{FileResolver, Resolve};

use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::{Bfs, EdgeRef};
use petgraph::Direction;
use proc_macro2::{Group, TokenStream, TokenTree};
use quote::ToTokens;
use serde::de::value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::path::Path as FilePath;
use syn::spanned::Spanned;
use syn::ForeignItemFn;
use syn::{
    ExprMacro, ForeignItemMacro, ImplItemMacro, ItemMacro, StmtMacro, TraitItemMacro,
};

/// Results of a scan
///
/// Holds the intermediate state between scans which doesn't hold references
/// to file data
#[derive(Debug, Default)]
pub struct ScanResults {
    pub effects: Vec<EffectInstance>,
    fn_ptr_effects: Vec<EffectInstance>,

    // Saved function declarations
    pub pub_fns: HashSet<CanonicalPath>,
    pub fn_locs: HashMap<CanonicalPath, SrcLoc>,
    pub trait_meths: HashSet<CanonicalPath>,
    fns_with_effects: HashSet<CanonicalPath>,

    pub decl_marco_def: HashMap<proc_macro2::Ident, syn::ItemMacro>,
    decl_marco_invoke: HashMap<proc_macro2::Ident, SynMacro>,
    pub macro_expanded: HashMap<proc_macro2::Ident, syn::File>,

    pub call_graph: DiGraph<CanonicalPath, SrcLoc>,
    pub node_idxs: HashMap<CanonicalPath, NodeIndex>,

    /* Tracking lines of code (LoC) and skipped/unsupported cases */
    pub total_loc: LoCTracker,
    pub skipped_macros: LoCTracker,
    pub skipped_conditional_code: LoCTracker,
    pub skipped_fn_calls: LoCTracker,
    pub skipped_fn_ptrs: LoCTracker,
    pub skipped_other: LoCTracker,
    pub unsafe_traits: LoCTracker,
    pub unsafe_impls: LoCTracker,
    pub fn_loc_tracker: HashMap<CanonicalPath, LoCTracker>,

    // TODO other cases:
    pub _effects_loc: LoCTracker,
    pub _skipped_build_rs: LoCTracker,
}

impl ScanResults {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn effects_set(&self) -> HashSet<&EffectInstance> {
        self.effects.iter().collect::<HashSet<_>>()
    }

    pub fn get_callers(&self, callee: &CanonicalPath) -> Result<HashSet<EffectInfo>> {
        let callee_node = self
            .node_idxs
            .get(callee)
            .context("Missing callee for get_callers in results graph")?;
        let effects = self
            .call_graph
            .edges_directed(*callee_node, Direction::Incoming)
            .map(|e| {
                let caller_node = e.source();
                let caller = self.call_graph[caller_node].clone();
                let src_loc = e.weight();
                EffectInfo::new(caller, src_loc.clone())
            })
            .collect::<HashSet<_>>();
        Ok(effects)
    }

    pub fn add_fn_dec(&mut self, f: FnDec) {
        let fn_name = f.fn_name;

        // Update call graph
        self.update_call_graph(&fn_name);

        // Save function info
        if f.vis == Visibility::Public || fn_name.is_main() {
            self.pub_fns.insert(fn_name.clone());
        }
        self.fn_locs.insert(fn_name, f.src_loc);
    }

    fn update_call_graph(&mut self, method: &CanonicalPath) -> NodeIndex {
        if let Some(node_idx) = self.node_idxs.get(method) {
            return node_idx.to_owned();
        }

        let node_idx = self.call_graph.add_node(method.clone());
        self.node_idxs.insert(method.clone(), node_idx);

        node_idx
    }

    fn add_call(&mut self, caller: &CanonicalPath, callee: &CanonicalPath, loc: SrcLoc) {
        let caller_idx = self.update_call_graph(caller);
        let callee_idx = self.update_call_graph(callee);
        self.call_graph.add_edge(caller_idx, callee_idx, loc);
    }
}

#[derive(Debug)]
enum SynMacro {
    ExprMacro(ExprMacro),
    ItemMacro(ItemMacro),
    StmtMacro(StmtMacro),
    ForeignItemMacro(ForeignItemMacro),
    TraitItemMacro(TraitItemMacro),
    ImplItemMacro(ImplItemMacro),
}

#[derive(Debug)]
pub struct Scanner<'a, R>
where
    R: Resolve<'a>,
{
    /// filepath that the scanner is being run on
    filepath: &'a FilePath,

    /// Name resolution resolver
    resolver: R,

    /// Number of unsafe keywords the current scope is nested inside
    /// (always 0 at top level)
    /// (includes only unsafe blocks and fn decls -- not traits and trait impls)
    scope_unsafe: usize,

    /// Number of effects found in the current unsafe block
    /// Used only for sanity check / debugging purposes
    scope_unsafe_effects: usize,

    /// Whether we are scanning an assignment expression.
    /// Useful to check if a union field is accessed to
    /// read its value, which is unsafe, or to write to it.
    /// Accessing a union field to assign to it is safe.
    scope_assign_lhs: bool,

    /// Functions inside
    scope_fns: Vec<FnDec>,

    /// Target to accumulate scan results
    data: &'a mut ScanResults,

    /// The list of sinks to look for
    sinks: HashSet<IdentPath>,

    /// The set of enabled cfg options for this crate.
    enabled_cfg: &'a HashMap<String, Vec<String>>,
}

impl<'a, R> Scanner<'a, R>
where
    R: Resolve<'a>,
{
    /*
        Main public API
    */

    /// Create a new scanner tied to a crate and file
    pub fn new(
        filepath: &'a FilePath,
        resolver: R,
        data: &'a mut ScanResults,
        enabled_cfg: &'a HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            filepath,
            resolver,
            scope_unsafe: 0,
            scope_unsafe_effects: 0,
            scope_assign_lhs: false,
            scope_fns: Vec::new(),
            data,
            sinks: Sink::default_sinks(),
            enabled_cfg,
        }
    }

    /// Top-level invariant -- called before consuming results
    pub fn assert_top_level_invariant(&self) {
        self.resolver.assert_top_level_invariant();
        debug_assert!(self.scope_fns.is_empty());
        debug_assert_eq!(self.scope_unsafe, 0);
        debug_assert_eq!(self.scope_unsafe_effects, 0);
    }

    pub fn add_sinks(&mut self, new_sinks: HashSet<IdentPath>) {
        self.sinks.extend(new_sinks);
    }

    /*
        Additional top-level items and modules

        These are public, with the caveat that the Scanner currently assumes
        they are all run on syntax within the same file.
    */

    pub fn scan_file(&mut self, f: &'a syn::File) {
        // track lines of code (LoC) at the file level
        self.data.total_loc.add(f);
        // scan the file and return a list of all calls in it
        for i in &f.items {
            self.scan_item(i);
        }
        self.process_macros();
    }

    // pub fn scan_expand_macro(&mut self) {
    //     for (_, expanded_file) in &self.data.macro_expanded {
    //         for i in &expanded_file.items {
    //             self.scan_item(i);
    //         }
    //     }
    // }

    // pub fn scan_macro_expanded(&mut self) {
    //     for (_, expanded_file) in &self.data.macro_expanded {
    //         for i in &expanded_file.items {
    //             self.scan_item(i);
    //         }
    //     }
    // }

    pub fn scan_item(&mut self, i: &'a syn::Item) {
        match i {
            syn::Item::Mod(m) => self.scan_mod(m),
            syn::Item::Use(u) => {
                self.resolver.scan_use(u);
            }
            syn::Item::Impl(imp) => self.scan_impl(imp),
            syn::Item::Fn(fun) => self.scan_fn_decl(fun),
            syn::Item::Trait(t) => self.scan_trait(t),
            syn::Item::ForeignMod(fm) => self.scan_foreign_mod(fm),
            syn::Item::Macro(m) => {
                // println!("Item Macro: {:#?}", m);
                // self.data.decl_marco_def.insert(m.ident.as_ref().unwrap().clone(),m.clone());
                match m.ident.as_ref() {
                    Some(ident) => {
                        self.data.decl_marco_def.insert(ident.clone(), m.clone());
                    }
                    _ => {
                        if let Some(ident) = m.mac.path.get_ident() {
                            self.data
                                .decl_marco_invoke
                                .insert(ident.clone(), SynMacro::ItemMacro(m.clone()));
                        } else {
                            self.data.skipped_macros.add(m);
                        }
                    }
                }
            }
            _ => (),
            // For all syntax elements see
            // https://docs.rs/syn/latest/syn/enum.Item.html
            // Potentially interesting:
            // Const(ItemConst), Static(ItemStatic) -- for information flow
        }
    }

    // Quickfix to decide when to skip a CFG attribute
    pub fn skip_cfg(&self, args: &TokenStream) -> bool {
        let cfg_pred = CfgPred::parse(args);
        !cfg_pred.is_enabled(self.enabled_cfg)
    }

    // Return true if the attributes imply the code should be skipped
    pub fn skip_attr(&self, attr: &'a syn::Attribute) -> bool {
        let path = attr.path();
        // if path.is_ident("cfg_args") || path.is_ident("cfg") {
        if path.is_ident("cfg") {
            let syn::Meta::List(l) = &attr.meta else { return false };
            let args = &l.tokens;
            if self.skip_cfg(args) {
                debug!("Skipping cfg attribute: {}", args);
                return true;
            } else {
                debug!("Scanning cfg attribute: {}", args);
                return false;
            }
        }
        false
    }

    // Return true if the attributes imply the code should be skipped
    pub fn skip_attrs(&self, attrs: &'a [syn::Attribute]) -> bool {
        attrs.iter().any(|x| self.skip_attr(x))
    }

    // pub fn scan_mod(&mut self, m: &'a syn::ItemMod) {
    //     if self.skip_attrs(&m.attrs) {
    //         self.data.skipped_conditional_code.add(m);
    //         return;
    //     }

    //     if let Some((_, items)) = &m.content {
    //         self.resolver.push_mod(&m.ident);
    //         for i in items {
    //             self.scan_item(i);
    //         }
    //         self.resolver.pop_mod();
    //     }
    // }

    pub fn scan_mod(&mut self, m: &'a syn::ItemMod) {
        if self.skip_attrs(&m.attrs) {
            self.data.skipped_conditional_code.add(m);
            return;
        }

        if let Some((_, items)) = &m.content {
            self.resolver.push_mod(&m.ident);
            for i in items {
                self.scan_item(i);
            }
            self.resolver.pop_mod();
        }
    }

    /*
        Reusable loggers
    */

    fn syn_debug<S: Spanned + Debug>(&self, msg: &str, syn_node: S) {
        let loc = SrcLoc::from_span(self.filepath, &syn_node);
        debug!("Scanner: {} ({}) ({:?})", msg, loc, syn_node);
    }

    fn syn_info<S: Spanned + Debug>(&self, msg: &str, syn_node: S) {
        let loc = SrcLoc::from_span(self.filepath, &syn_node);
        info!("Scanner: {} ({}) ({:?})", msg, loc, syn_node);
    }

    fn syn_warning<S: Spanned + Debug>(&self, msg: &str, syn_node: S) {
        let loc = SrcLoc::from_span(self.filepath, &syn_node);
        warn!("Scanner: {} ({}) ({:?})", msg, loc, syn_node);
    }

    /*
        Extern blocks
    */

    fn scan_foreign_mod(&mut self, fm: &'a syn::ItemForeignMod) {
        if self.skip_attrs(&fm.attrs) {
            self.data.skipped_conditional_code.add(fm);
            return;
        }

        for i in &fm.items {
            self.scan_foreign_item(i);
        }
    }

    fn scan_foreign_item(&mut self, i: &'a syn::ForeignItem) {
        match i {
            syn::ForeignItem::Fn(f) => self.scan_foreign_fn(f),
            syn::ForeignItem::Macro(m) => {
                println!("Foreign Item Macro: {:#?}", m);
                if let Some(ident) = m.mac.path.get_ident() {
                    self.data
                        .decl_marco_invoke
                        .insert(ident.clone(), SynMacro::ForeignItemMacro(m.clone()));
                } else {
                    self.data.skipped_macros.add(m);
                }
            }
            other => {
                self.data.skipped_other.add(other);
            }
        }
        // Ignored: Static, Type, Macro, Verbatim
        // https://docs.rs/syn/latest/syn/enum.ForeignItem.html
    }

    fn scan_foreign_fn(&mut self, f: &'a ForeignItemFn) {
        if self.skip_attrs(&f.attrs) {
            self.data.skipped_conditional_code.add(f);
            return;
        }

        // Notify HackyResolver for this declaration
        self.resolver.scan_foreign_fn(f);
        // Resolve FFI declaration
        let Some(cp) = self.resolver.resolve_ffi_ident(&f.sig.ident) else {
            return;
        };
        let ffi_dec = FnDec::new(self.filepath, &f, cp.clone(), &f.vis);

        // If it is not a public FFI declaration
        // do not update ScanResults
        if ffi_dec.vis != Visibility::Public {
            return;
        }

        // Get the total lines of code of this declaration
        let mut ffi_loc = LoCTracker::new();
        ffi_loc.add(f);
        self.data.fn_loc_tracker.insert(cp.clone(), ffi_loc);

        // Notify ScanResults
        self.data.add_fn_dec(ffi_dec);

        self.push_effect(f.span(), cp.clone(), Effect::FFIDecl(cp));
    }

    /*
        Trait declarations and impl blocks
    */

    fn scan_trait(&mut self, t: &'a syn::ItemTrait) {
        if self.skip_attrs(&t.attrs) {
            self.data.skipped_conditional_code.add(t);
            return;
        }

        // let t_name = self.resolver.resolve_def(&t.ident);
        let t_unsafety = t.unsafety;
        if t_unsafety.is_some() {
            // we found an `unsafe trait` declaration
            self.data.unsafe_traits.add(&t.ident);
        }

        let all_impls = self.resolver.resolve_all_impl_methods(&t.ident);
        for item in &t.items {
            match item {
                syn::TraitItem::Fn(m) => {
                    let impls_for_meth = all_impls
                        .iter()
                        .filter(|cp| match cp.as_path().last_ident() {
                            Some(ident) => m.sig.ident == ident.to_string(),
                            _ => false,
                        })
                        .collect::<Vec<&CanonicalPath>>();
                    self.scan_trait_method(m, &t.vis, impls_for_meth);
                }
                syn::TraitItem::Macro(m) => {
                    println!("Trait Item Macro: {:#?}", m);
                    if let Some(ident) = m.mac.path.get_ident() {
                        self.data
                            .decl_marco_invoke
                            .insert(ident.clone(), SynMacro::TraitItemMacro(m.clone()));
                    } else {
                        self.data.skipped_macros.add(m);
                    }
                }
                syn::TraitItem::Verbatim(v) => {
                    self.syn_info("skipping Verbatim expression", v);
                }
                other => {
                    self.data.skipped_other.add(other);
                }
            }
        }
    }

    fn scan_impl(&mut self, imp: &'a syn::ItemImpl) {
        if self.skip_attrs(&imp.attrs) {
            self.data.skipped_conditional_code.add(imp);
            return;
        }

        self.resolver.push_impl(imp);

        if let Some((_, tr, _)) = &imp.trait_ {
            self.scan_impl_trait_path(tr, imp);
        }

        for item in &imp.items {
            match item {
                syn::ImplItem::Fn(m) => {
                    self.scan_method(m);
                }
                syn::ImplItem::Macro(m) => {
                    // println!("Impl Item Macro: {:#?}", m);
                    if let Some(ident) = m.mac.path.get_ident() {
                        self.data
                            .decl_marco_invoke
                            .insert(ident.clone(), SynMacro::ImplItemMacro(m.clone()));
                    } else {
                        self.data.skipped_macros.add(m);
                    }
                }
                syn::ImplItem::Verbatim(v) => {
                    self.syn_info("skipping Verbatim expression", v);
                }
                other => {
                    self.data.skipped_other.add(other);
                }
            }
        }

        self.resolver.pop_impl();
    }

    fn scan_impl_trait_path(&mut self, tr: &'a syn::Path, imp: &'a syn::ItemImpl) {
        if imp.unsafety.is_some() {
            // we found an `unsafe impl` declaration
            // let tr_name = self.resolver.resolve_path(tr);
            // let self_ty = imp
            //     .self_ty
            //     .to_token_stream()
            //     .into_iter()
            //     .filter_map(|token| match token {
            //         TokenTree::Ident(i) => Some(i),
            //         _ => None,
            //     })
            //     .last();
            // // resolve the implementing type of the trait, if there is one
            // let tr_type = match &self_ty {
            //     Some(ident) => Some(self.resolver.resolve_ident(ident)),
            //     _ => None,
            // };

            self.data.unsafe_impls.add(tr);
        }
    }

    /*
        Function and method declarations
    */

    fn scan_fn_decl(&mut self, f: &'a syn::ItemFn) {
        self.syn_debug("scanning function", f);

        if self.skip_attrs(&f.attrs) {
            self.data.skipped_conditional_code.add(f);
            return;
        }

        self.scan_fn(&f.sig, &f.block, &f.vis);
    }

    fn scan_trait_method(
        &mut self,
        m: &'a syn::TraitItemFn,
        vis: &'a syn::Visibility,
        impl_methods: Vec<&CanonicalPath>,
    ) {
        if self.skip_attrs(&m.attrs) {
            self.data.skipped_conditional_code.add(m);
            return;
        }

        // If there is a default implementation, scan the function body as usual.
        // Otherwise, just create a node in the call graph for the abstract trait method.
        let f_name = self.resolver.resolve_def(&m.sig.ident);
        if let Some(body) = &m.default {
            self.scan_fn(&m.sig, body, vis);
        } else {
            // Update call graph
            self.data.update_call_graph(&f_name);
            self.data.trait_meths.insert(f_name.clone());
        }

        // Add edges in the call graph from all impl methods to their corresponding abstract trait method
        for impl_meth in &impl_methods {
            self.data.add_call(
                &f_name,
                impl_meth,
                SrcLoc::from_span(self.filepath, &m.span()),
            );
        }
    }

    fn scan_method(&mut self, m: &'a syn::ImplItemFn) {
        if self.skip_attrs(&m.attrs) {
            self.data.skipped_conditional_code.add(m);
            return;
        }

        // NB: may or may not be a method, if there is no self keyword
        self.scan_fn(&m.sig, &m.block, &m.vis);
    }

    fn scan_fn(
        &mut self,
        f_sig: &'a syn::Signature,
        body: &'a syn::Block,
        vis: &'a syn::Visibility,
    ) {
        // Create fn decl
        let f_ident = &f_sig.ident;
        let f_name = self.resolver.resolve_def(f_ident);
        let fn_dec = FnDec::new(self.filepath, f_sig, f_name.clone(), vis);

        // Get the total lines of code of this function
        let mut fn_loc = LoCTracker::new();
        fn_loc.add(body.span());
        self.data.fn_loc_tracker.insert(f_name.clone(), fn_loc);

        // Always push the new function declaration before scanning the
        // body so we have access to the function its in
        self.scope_fns.push(fn_dec.clone());

        // Notify resolver
        self.resolver.push_fn(f_ident);

        // Notify ScanResults
        self.data.add_fn_dec(fn_dec);

        // Update unsafety
        let f_unsafety: &Option<syn::token::Unsafe> = &f_sig.unsafety;
        if f_unsafety.is_some() {
            self.scope_unsafe += 1;

            // We need to track unsafe functions to properly
            // filter `FnPtrCreation` effect instances at the
            // end of the scan, if the pointer points to an
            // unsafe function
            self.data.fns_with_effects.insert(f_name.clone());
        }

        // Similarly, we need to track local FFI declarations to
        // properly filter `FnPtrCreation` effect instances at the
        // end of the scan, if the pointer points to a foreign function
        if f_sig.abi.is_some() {
            self.data.fns_with_effects.insert(f_name.clone());
        }

        // ***** Scan body *****
        for s in &body.stmts {
            self.scan_fn_statement(s);
        }

        // Reset state
        self.scope_fns.pop();
        self.resolver.pop_fn();

        // Reset unsafety
        if let Some(f_unsafety) = f_unsafety {
            debug_assert!(self.scope_unsafe >= 1);
            self.scope_unsafe -= 1;
            if self.scope_unsafe_effects == 0 {
                self.syn_debug("unsafe block without any unsafe effects", f_unsafety)
            }
            self.scope_unsafe_effects = 0;
        }
    }

    fn scan_fn_statement(&mut self, s: &'a syn::Stmt) {
        match s {
            syn::Stmt::Local(l) => self.scan_fn_local(l),
            syn::Stmt::Expr(e, _semi) => self.scan_expr(e),
            syn::Stmt::Item(i) => self.scan_item_in_fn(i),
            syn::Stmt::Macro(m) => {
                // println!("Stmt Macro: {:#?}", m);
                if let Some(ident) = m.mac.path.get_ident() {
                    self.data
                        .decl_marco_invoke
                        .insert(ident.clone(), SynMacro::StmtMacro(m.clone()));
                } else {
                    self.data.skipped_macros.add(m);
                }
            }
        }
    }

    fn scan_item_in_fn(&mut self, i: &'a syn::Item) {
        // TBD: may need to track additional state here
        self.scan_item(i);
    }

    fn scan_fn_local(&mut self, l: &'a syn::Local) {
        if self.skip_attrs(&l.attrs) {
            self.data.skipped_conditional_code.add(l);
            return;
        }

        if let Some(let_expr) = &l.init {
            self.scan_expr(&let_expr.expr);
            if let Some((_, else_expr)) = &let_expr.diverge {
                self.scan_expr(else_expr);
            }
        }
    }

    /*
        Expressions
        These have the most cases (currently, 40)
    */

    fn scan_expr(&mut self, e: &'a syn::Expr) {
        match e {
            syn::Expr::Array(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                for y in x.elems.iter() {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Assign(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scope_assign_lhs = true;
                self.scan_expr(&x.left);
                self.scope_assign_lhs = false;
                self.scan_expr(&x.right);
            }
            syn::Expr::Async(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                for s in &x.block.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Await(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.base);
            }
            syn::Expr::Binary(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.left);
                self.scan_expr(&x.right);
            }
            syn::Expr::Block(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                for s in &x.block.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Break(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                if let Some(y) = &x.expr {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Call(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }
                // ***** THE FIRST IMPORTANT CASE *****
                // Arguments
                self.scan_expr_call_args(&x.args);
                // Function call
                self.scan_expr_call(&x.func);
            }
            syn::Expr::Cast(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                // If we see a cast to a raw pointer, add the effect
                if let syn::Type::Ptr(_) = *x.ty {
                    let mut tokens: TokenStream = TokenStream::new();
                    (*x.expr).to_tokens(&mut tokens);
                    tokens.into_iter().for_each(|tt| {
                        if let TokenTree::Ident(i) = tt {
                            let p = self.resolver.resolve_field(&i);
                            self.push_effect(x.span(), p, Effect::RawPtrCast)
                        }
                    });
                }
                self.scan_expr(&x.expr);
            }
            syn::Expr::Closure(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                // TBD: closures are a bit weird!
                // Note that the body expression doesn't get evaluated yet,
                // and may be evaluated somewhere else.
                // May need to do something more special here.
                self.scan_closure(x);
            }
            syn::Expr::Continue(_) => (),
            syn::Expr::Field(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.base);
                self.scan_field_access(x);
            }
            syn::Expr::ForLoop(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.expr);
                for s in &x.body.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Group(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.expr);
            }
            syn::Expr::If(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.cond);
                for s in &x.then_branch.stmts {
                    self.scan_fn_statement(s);
                }
                if let Some((_, y)) = &x.else_branch {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Index(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.expr);
                self.scan_expr(&x.index);
            }
            syn::Expr::Let(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.expr);
            }
            syn::Expr::Lit(_) => (),
            syn::Expr::Loop(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                for s in &x.body.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Macro(m) => {
                // println!("Expr Macro: {:#?}", m);
                if let Some(ident) = m.mac.path.get_ident() {
                    self.data
                        .decl_marco_invoke
                        .insert(ident.clone(), SynMacro::ExprMacro(m.clone()));
                } else {
                    self.data.skipped_macros.add(m);
                }
            }
            syn::Expr::Match(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.expr);
                for a in &x.arms {
                    if self.skip_attrs(&a.attrs) {
                        self.data.skipped_conditional_code.add(a);
                        return;
                    }

                    if let Some((_, y)) = &a.guard {
                        self.scan_expr(y);
                    }
                    self.scan_expr(&a.body);
                }
            }
            syn::Expr::MethodCall(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                // ***** THE SECOND IMPORTANT CASE *****
                // Receiver object
                self.scan_expr(&x.receiver);
                // Arguments
                self.scan_expr_call_args(&x.args);
                // Function call
                self.scan_expr_call_method(&x.method);
            }
            syn::Expr::Paren(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.expr);
            }
            syn::Expr::Path(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                // typically a local variable
                self.scan_path(&x.path);
            }
            syn::Expr::Range(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                if let Some(y) = &x.start {
                    self.scan_expr(y);
                }
                if let Some(y) = &x.end {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Reference(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.expr);
            }
            syn::Expr::Repeat(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.expr);
                self.scan_expr(&x.len);
            }
            syn::Expr::Return(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                if let Some(y) = &x.expr {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Struct(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                for y in x.fields.iter() {
                    if self.skip_attrs(&y.attrs) {
                        self.data.skipped_conditional_code.add(y);
                        return;
                    }

                    self.scan_expr(&y.expr);
                }
                if let Some(y) = &x.rest {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Try(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.expr);
            }
            syn::Expr::TryBlock(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.syn_warning("encountered try block (unstable feature)", x);
                for y in &x.block.stmts {
                    self.scan_fn_statement(y);
                }
            }
            syn::Expr::Tuple(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                for y in x.elems.iter() {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Unary(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                if let syn::UnOp::Deref(_) = x.op {
                    self.scan_deref(&x.expr);
                }
                self.scan_expr(&x.expr);
            }
            syn::Expr::Unsafe(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                // ***** THE THIRD IMPORTANT CASE *****
                self.scan_unsafe_block(x);
            }
            syn::Expr::Verbatim(v) => {
                self.syn_info("skipping Verbatim expression", v);
            }
            syn::Expr::While(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.scan_expr(&x.cond);
                for s in &x.body.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Yield(x) => {
                if self.skip_attrs(&x.attrs) {
                    self.data.skipped_conditional_code.add(x);
                    return;
                }

                self.syn_warning("encountered yield expression (unstable feature)", x);
                if let Some(y) = &x.expr {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Const(c) => {
                if self.skip_attrs(&c.attrs) {
                    self.data.skipped_conditional_code.add(c);
                    return;
                }

                for s in &c.block.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Infer(_) => {
                // a single underscore _
                // we can ignore this
            }
            _ => {
                // unexpected case
                self.syn_warning("encountered unknown/unexpected expression", e)
            }
        }
    }

    fn scan_path(&mut self, x: &'a syn::Path) {
        let ty = self.resolver.resolve_path_type(x);
        if ty.is_function() {
            // Skip constant or immutable static function pointers
            if self.resolver.resolve_const_or_static(x) {
                self.syn_info("Skipping const or static item", x);
                self.data.skipped_fn_ptrs.add(x.span());
            } else {
                let cp = self.resolver.resolve_path(x);
                self.push_effect(x.span(), cp, Effect::FnPtrCreation);
            }
        }
        // Accessing a mutable global variable
        if ty.is_mut_static() {
            let cp = self.resolver.resolve_path(x);
            // NOTE: Can only be done in an unsafe block
            self.push_effect(x.span(), cp.clone(), Effect::StaticMut(cp));
        }
        // Accessing an external static variable
        if self.resolver.resolve_ffi(x).is_some() {
            let cp = self.resolver.resolve_path(x);
            // NOTE: Can only be done in an unsafe block
            self.push_effect(x.span(), cp.clone(), Effect::StaticExt(cp));
        }
    }

    fn scan_closure(&mut self, x: &'a syn::ExprClosure) {
        self.syn_debug("scanning closure", x);

        let effects_num = self.data.effects.len();
        // Scan closure's body first. If it does not contain
        // any effects, the closure is not dangerous and
        // we do not create a new effect instance for it.
        self.scan_expr(&x.body);
        if self.data.effects.len() > effects_num {
            let cl_name = self.resolver.resolve_closure(x);
            self.push_effect(x.span(), cl_name, Effect::ClosureCreation);
        }
    }

    fn scan_deref(&mut self, x: &'a syn::Expr) {
        let mut tokens: TokenStream = TokenStream::new();
        x.to_tokens(&mut tokens);
        tokens.into_iter().for_each(|tt| {
            if let TokenTree::Ident(i) = tt {
                let ty = self.resolver.resolve_field_type(&i);
                let p = self.resolver.resolve_field(&i);
                if ty.is_raw_ptr() {
                    // NOTE: Can only be done in an unsafe block
                    self.push_effect(x.span(), p.clone(), Effect::RawPointer(p));
                }
            }
        });
    }

    // Check if the field being accessed is a Union field
    fn scan_field_access(&mut self, x: &'a syn::ExprField) {
        if let syn::Member::Named(i) = &x.member {
            let ty = self.resolver.resolve_field_type(i);
            if !ty.is_union_field() || self.scope_assign_lhs {
                return;
            }
            let cp = self.resolver.resolve_field(i);
            // NOTE: Can only be done in an unsafe block
            self.push_effect(x.span(), cp.clone(), Effect::UnionField(cp));
        }
    }

    fn scan_unsafe_block(&mut self, x: &'a syn::ExprUnsafe) {
        self.scope_unsafe += 1;
        for s in &x.block.stmts {
            self.scan_fn_statement(s);
        }

        // Reset unsafety
        debug_assert!(self.scope_unsafe >= 1);
        self.scope_unsafe -= 1;
        if self.scope_unsafe_effects == 0 {
            self.syn_debug("unsafe block without any unsafe effects", x)
        }
        self.scope_unsafe_effects = 0;
    }

    /*
        Function calls --what we're interested in
    */
    fn scan_expr_call_args(
        &mut self,
        a: &'a syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    ) {
        for y in a.iter() {
            self.scan_expr(y);
        }
    }

    /// Push an effect into the current `EffectBlock`. Should be used when
    /// pushing an effect in an unsafe block so all effects can be captured at
    /// the same time.
    fn push_effect<S>(&mut self, eff_span: S, callee: CanonicalPath, eff_type: Effect)
    where
        S: Debug + Spanned,
    {
        let caller = if eff_type.is_ffi_decl() {
            &callee
        } else {
            let containing_fn = self.scope_fns.last().expect("not inside a function!");
            &containing_fn.fn_name
        };

        let eff = EffectInstance::new_effect(
            self.filepath,
            caller.clone(),
            callee.clone(),
            &eff_span,
            eff_type.clone(),
        );

        if self.scope_unsafe > 0 && eff.is_rust_unsafe() {
            self.scope_unsafe_effects += 1;
        }
        // Do not add effect instance to effects yet,
        // if it's a function pointer. We will check if
        // the function the pointer points to has potential
        // effects itself at the end of the scan
        if matches!(eff_type, Effect::FnPtrCreation) {
            self.data.fn_ptr_effects.push(eff);
        } else {
            self.data.effects.push(eff);
            self.data.fns_with_effects.insert(caller.clone());
        }
    }

    /// push an Effect to the list of results based on this call site.
    fn push_callsite<S>(
        &mut self,
        callee_span: S,
        callee: CanonicalPath,
        ffi: Option<CanonicalPath>,
        is_unsafe: bool,
    ) where
        S: Debug + Spanned,
    {
        let containing_fn = self.scope_fns.last().expect("not inside a function!");
        let caller = &containing_fn.fn_name;
        self.data.add_call(
            caller,
            &callee,
            SrcLoc::from_span(self.filepath, &callee_span.span()),
        );

        let Some(eff) = EffectInstance::new_call(
            self.filepath,
            caller.clone(),
            callee,
            &callee_span,
            is_unsafe,
            ffi,
            &self.sinks,
        ) else {
            return;
        };

        if self.scope_unsafe > 0 && eff.is_rust_unsafe() {
            self.scope_unsafe_effects += 1;
        }
        self.data.effects.push(eff);
        self.data.fns_with_effects.insert(caller.clone());
    }

    // f in a call of the form (f)(args)
    fn scan_expr_call(&mut self, f: &'a syn::Expr) {
        match f {
            syn::Expr::Path(p) => {
                let callee = self.resolver.resolve_path(&p.path);
                let ffi = self.resolver.resolve_ffi(&p.path);
                let is_unsafe =
                    self.resolver.resolve_unsafe_path(&p.path) && self.scope_unsafe > 0;
                self.push_callsite(p, callee, ffi, is_unsafe);
            }
            syn::Expr::Paren(x) => {
                // e.g. (my_struct.f)(x)
                self.scan_expr_call(&x.expr);
            }
            syn::Expr::Field(x) => {
                // e.g. my_struct.f: F where F: Fn(A) -> B
                // Note: not a method call!
                self.scan_expr_call_field(&x.member)
            }
            syn::Expr::Macro(m) => {
                // println!("Expr All Macro: {:#?}", m);
                if let Some(ident) = m.mac.path.get_ident() {
                    self.data
                        .decl_marco_invoke
                        .insert(ident.clone(), SynMacro::ExprMacro(m.clone()));
                } else {
                    self.data.skipped_macros.add(m);
                }
            }
            other => {
                // anything else could be a function, too -- could return a closure
                // or fn pointer. No way to tell w/o type information.
                self.syn_info("Skipped function call", other);
                self.data.skipped_fn_calls.add(other);
            }
        }
    }

    fn scan_expr_call_field(&mut self, m: &'a syn::Member) {
        match m {
            syn::Member::Named(i) => {
                let is_unsafe =
                    self.resolver.resolve_unsafe_ident(i) && self.scope_unsafe > 0;
                self.push_callsite(i, self.resolver.resolve_field(i), None, is_unsafe);
            }
            syn::Member::Unnamed(idx) => {
                self.push_callsite(
                    idx,
                    self.resolver.resolve_field_index(idx),
                    None,
                    self.scope_unsafe > 0,
                );
            }
        }
    }

    fn scan_expr_call_method(&mut self, i: &'a syn::Ident) {
        let is_unsafe = self.resolver.resolve_unsafe_ident(i) && self.scope_unsafe > 0;
        self.push_callsite(i, self.resolver.resolve_method(i), None, is_unsafe);
    }

    pub fn process_macros(&mut self) {
        for (ident, m) in &self.data.decl_marco_invoke {
            let res = self.expand_macro_recursively(&ident, &m);
            match res {
                Ok(expanded_ast) => {
                    self.data.macro_expanded.insert(ident.clone(), expanded_ast);
                }
                Err(_) => {
                    self.syn_warning("Failed to expand macro", ident);
                }
            }
        }
        self.data.decl_marco_invoke.clear();
        // for (_, ast) in &self.data.macro_expanded {
        //     for item in &ast.items {
        //         self.scan_item(item);
        //     }
        // }
    }

    fn expand_macro_recursively(
        &self,
        ident: &proc_macro2::Ident,
        macro_invocation: &SynMacro,
    ) -> Result<syn::File, syn::Error> {
        let expanded_code;

        if self.data.decl_marco_def.contains_key(ident) {
            let macro_decl = self.data.decl_marco_def.get(ident);
            expanded_code = macro_expand(ident, macro_invocation, macro_decl.unwrap());
        } else {
            // todo: if use the imported macro, or macro in prelude, check the dependency.
            expanded_code = "".to_string();
            // self.data.skipped_macros.add(macro_invocation);
        }
        println!("expaanded ident: {:#?}", ident);
        println!("expanded code: {}", expanded_code);
        return syn::parse_file(&expanded_code);
    }
}

fn macro_expand(
    ident: &proc_macro2::Ident,
    macro_invocation: &SynMacro,
    macro_decl: &ItemMacro,
) -> String {
    let macro_tokens = &macro_decl.mac.tokens;
    let tokens_iter = macro_tokens.clone().into_iter();
    let mut pattern = TokenStream::new();
    let mut replacement = TokenStream::new();
    let mut in_replacement = false;

    println!("parsing token for macro decl");
    for token in tokens_iter.clone() {
        println!("token: {}", token.to_string());
        if let proc_macro2::TokenTree::Punct(ref punct) = token {
            if punct.as_char() == '=' {
                in_replacement = true;
                continue;
            }
        }
        if in_replacement {
            replacement.extend(Some(token));
        } else {
            pattern.extend(Some(token));
        }
    }

    let matched_values;
    println!("getting matched value");
    match macro_invocation {
        SynMacro::TraitItemMacro(trait_item_macro) => {
            matched_values =
                match_macro_invocation(pattern, &trait_item_macro.mac.tokens);
        }
        SynMacro::ItemMacro(item_macro) => {
            matched_values = match_macro_invocation(pattern, &item_macro.mac.tokens);
        }
        SynMacro::ExprMacro(expr_macro) => {
            matched_values = match_macro_invocation(pattern, &expr_macro.mac.tokens);
        }
        SynMacro::ImplItemMacro(impl_item_macro) => {
            matched_values = match_macro_invocation(pattern, &impl_item_macro.mac.tokens);
        }
        SynMacro::StmtMacro(stmt_macro) => {
            matched_values = match_macro_invocation(pattern, &stmt_macro.mac.tokens);
        }
        SynMacro::ForeignItemMacro(foreign_item_macro) => {
            matched_values =
                match_macro_invocation(pattern, &foreign_item_macro.mac.tokens);
        }
    }
    println!("got matched value");
    for val in matched_values.clone().unwrap().into_keys() {
        println!("matched value: {}", val);
    }
    let macro_content =
        replacement.into_iter().find(|token| matches!(token, TokenTree::Group(_)));
    let expanded =
        substitute_replacement(macro_content.to_token_stream(), matched_values.unwrap())
            .unwrap()
            .to_string();
    expanded
}

fn match_macro_invocation(
    pattern: TokenStream,
    tokens: &TokenStream,
) -> Result<HashMap<String, Vec<TokenStream>>, String> {
    let mut values = HashMap::new();
    let mut tokens_iter = tokens.clone().into_iter();

    for pattern_token in pattern {
        match pattern_token {
            TokenTree::Group(group) => {
                let inner_tokens = group.stream();
                let mut inner_iter = inner_tokens.into_iter();

                while let Some(inner_token) = inner_iter.next() {
                    match inner_token {
                        TokenTree::Punct(punct) if punct.as_char() == '$' => {
                            // Look for an identifier following `$`
                            if let Some(TokenTree::Ident(ident)) = inner_iter.next() {
                                // Capture the corresponding token from the invocation
                                let mut value_tokens = Vec::new();
                                if let Some(value_token) = tokens_iter.next() {
                                    value_tokens.push(value_token.to_token_stream());
                                    if value_token.to_string() == "mod" {
                                        let value_token2 = tokens_iter.next().unwrap();
                                        value_tokens.push(value_token2.to_token_stream());
                                        values.insert(ident.to_string(), value_tokens);
                                    } else {
                                        values.insert(ident.to_string(), value_tokens);
                                    }
                                } else {
                                    return Err(
                                        "Mismatched invocation for pattern".into()
                                    );
                                }

                                // Skip over pattern specifiers (e.g., `:expr`) if present
                                if let Some(TokenTree::Punct(colon)) = inner_iter.next() {
                                    if colon.as_char() == ':' {
                                        inner_iter.next();
                                    }
                                }
                            }
                        }
                        // Skip non-placeholder tokens (like commas and other delimiters)
                        _ => {
                            tokens_iter.next();
                        }
                    }
                }
            }
            _ => {
                tokens_iter.next();
            }
        }
    }

    Ok(values)
}

fn substitute_replacement(
    replacement: TokenStream,
    values: HashMap<String, Vec<TokenStream>>,
) -> Result<TokenStream, String> {
    let mut expanded = TokenStream::new();
    let mut replacement_iter = replacement.into_iter();

    while let Some(token) = replacement_iter.next() {
        match token {
            TokenTree::Punct(punct) if punct.as_char() == '$' => {
                // Check if `$` is followed by a placeholder identifier
                if let Some(TokenTree::Ident(ident)) = replacement_iter.next() {
                    if let Some(value) = values.get(&ident.to_string()) {
                        expanded.extend(value.clone().into_iter());
                        continue;
                    }
                }
            }
            TokenTree::Group(group) => {
                // Recursively expand within groups
                let inner_stream =
                    substitute_replacement(group.stream(), values.clone())?;
                let mut new_group = Group::new(group.delimiter(), inner_stream);
                new_group.set_span(group.span());
                expanded.extend(Some(TokenTree::Group(new_group)));
            }
            _ => {
                expanded.extend(Some(token));
            }
        }
    }

    Ok(expanded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::{parse_quote, ItemMacro};

    #[test]
    fn test_expand_declarative_macro() {
        // Define a simple declarative macro as `macro_rules! example { ($arg:expr) => { $arg }; }`

        let macro_def: ItemMacro = parse_quote! {
            macro_rules! multiply_add {
                ($a:expr, $b:expr, $c:expr) => {$a * ($b + $c)};
            }
        };

        let invocation_stmt: syn::Stmt = parse_quote! {
            multiply_add!(2,9,1);
        };

        // Extract the macro from the statement
        let invocation_mac = match invocation_stmt {
            syn::Stmt::Macro(stmt_macro) => stmt_macro,
            _ => panic!("Expected a Stmt::Macro"),
        };

        let invocation = SynMacro::StmtMacro(invocation_mac.clone());
        // Expand the macro invocation
        println!("Entering macro_expand");
        let expanded = macro_expand(
            &macro_def.ident.clone().unwrap(),
            &invocation,
            &macro_def.clone(),
        );

        // Assert that the expansion is as expected
        assert_eq!(expanded, "{ 2 * (9 + 1) }");
    }
}

/// Load the Rust file at the filepath and scan it (quick mode)
pub fn scan_file_quick(
    crate_name: &str,
    filepath: &FilePath,
    scan_results: &mut ScanResults,
    sinks: HashSet<IdentPath>,
    enabled_cfg: &HashMap<String, Vec<String>>,
) -> Result<()> {
    let mut file = File::open(filepath)?;
    let mut src = String::new();
    file.read_to_string(&mut src)?;
    let syntax_tree = syn::parse_file(&src)?;

    let hacky_resolver = HackyResolver::new(crate_name, filepath);

    let mut scanner =
        Scanner::new(filepath, hacky_resolver.unwrap(), scan_results, enabled_cfg);
    scanner.add_sinks(sinks);

    scanner.scan_file(&syntax_tree);

    Ok(())
}

/// Load the Rust file at the filepath and scan it
pub fn scan_file(
    crate_name: &str,
    filepath: &FilePath,
    resolver: &Resolver,
    scan_results: &mut ScanResults,
    sinks: HashSet<IdentPath>,
    enabled_cfg: &HashMap<String, Vec<String>>,
) -> Result<()> {
    debug!("Scanning file: {:?}", filepath);

    // Load file contents
    let mut file = File::open(filepath)?;
    let mut src = String::new();
    file.read_to_string(&mut src)?;
    let syntax_tree = syn::parse_file(&src)?;

    // Initialize resolver
    let file_resolver = FileResolver::new(crate_name, resolver, filepath)?;

    // Initialize scanner
    let mut scanner = Scanner::new(filepath, file_resolver, scan_results, enabled_cfg);
    scanner.add_sinks(sinks);

    // Scan file contents
    scanner.scan_file(&syntax_tree);
    let macro_expanded = scanner.data.macro_expanded.clone(); // 克隆一个引用副本
    println!("begin scan macro expanded");
    for (_, macro_file) in &macro_expanded {
        for item in &macro_file.items {
            scanner.scan_item(item);
        }
    }

    Ok(())
}

/// Try to run scan_file, reporting any errors back to the user
pub fn try_scan_file(
    crate_name: &str,
    filepath: &FilePath,
    resolver: &Resolver,
    scan_results: &mut ScanResults,
    sinks: HashSet<IdentPath>,
    enabled_cfg: &HashMap<String, Vec<String>>,
    quick_mode: bool,
) {
    if quick_mode {
        scan_file_quick(crate_name, filepath, scan_results, sinks, enabled_cfg)
            .unwrap_or_else(|err| {
                info!("Failed to scan file {} ({})", filepath.to_string_lossy(), err);
            })
    } else {
        scan_file(crate_name, filepath, resolver, scan_results, sinks, enabled_cfg)
            .unwrap_or_else(|err| {
                info!("Failed to scan file: {} ({})", filepath.to_string_lossy(), err);
            });
    }
}

/// Scan the supplied crate with an additional list of sinks
pub fn scan_crate_with_sinks(
    crate_path: &FilePath,
    sinks: HashSet<IdentPath>,
    relevant_effects: &[EffectType],
    quick_mode: bool,
) -> Result<ScanResults> {
    info!("Scanning crate: {:?}", crate_path);

    // Make sure the path is a crate
    if !crate_path.is_dir() {
        return Err(anyhow!("Path is not a crate; not a directory: {:?}", crate_path));
    }

    let mut cargo_toml_path = crate_path.to_path_buf();
    cargo_toml_path.push("Cargo.toml");
    if !cargo_toml_path.try_exists()? || !cargo_toml_path.is_file() {
        return Err(anyhow!("Path is not a crate; missing Cargo.toml: {:?}", crate_path));
    }

    let crate_name = util::load_cargo_toml(crate_path)?.crate_name;

    // TODO: this should *not* be created in the quick-mode case
    let resolver = Resolver::new(crate_path)?;

    let mut scan_results = ScanResults::new();

    let enabled_cfg = resolver.get_cfg_options_for_crate(&crate_name).unwrap_or_default();

    // TODO: For now, only walking through the src dir, but might want to
    //       include others (e.g. might codegen in other dirs)
    // If there is no src_dir, we walk through all .rs files in the crate.

    let src_dir = crate_path.join(FilePath::new("src"));
    let file_iter = if src_dir.is_dir() {
        util::fs::walk_files_with_extension(&src_dir, "rs")
    } else {
        info!("crate has no src dir; scanning all .rs files instead");
        util::fs::walk_files_with_extension(crate_path, "rs")
    };

    for entry in file_iter {
        try_scan_file(
            &crate_name,
            entry.as_path(),
            &resolver,
            &mut scan_results,
            sinks.clone(),
            &enabled_cfg,
            quick_mode,
        );
    }

    for i in scan_results.decl_marco_def.clone().into_keys() {
        println!("decl_marco_def: {}", i);
    }
    for i in scan_results.decl_marco_invoke.keys() {
        println!("decl_marco_invoke: {}", i);
    }

    filter_fn_ptr_effects(&mut scan_results, crate_name);
    scan_results
        .effects
        .retain(|e| EffectType::matches_effect(relevant_effects, e.eff_type()));

    Ok(scan_results)
}

/// Scan the supplied crate
pub fn scan_crate(
    crate_path: &FilePath,
    relevant_effects: &[EffectType],
    quick_mode: bool,
) -> Result<ScanResults> {
    scan_crate_with_sinks(crate_path, HashSet::new(), relevant_effects, quick_mode)
}

/// Keep only the `FnPtrCreation` effect instances for the pointers that
/// point to functions with effects or functions defined in dependencies
fn filter_fn_ptr_effects(scan_results: &mut ScanResults, crate_name: String) {
    let mut crate_name = crate_name;
    crate::ident::replace_hyphens(&mut crate_name);

    for p in scan_results.fn_ptr_effects.iter() {
        if !p.callee().crate_name().to_string().eq(&crate_name)
            || check_fn_for_effects(scan_results, p.callee())
        {
            scan_results.effects.push(p.clone());
            scan_results.fns_with_effects.insert(p.caller().clone());
        }
    }
}

// We still need to track transitive effects from callees, because the immediate
// function the pointer points to might not have effects, but it might call other
// functions with potentially dangerous behavior.
fn check_fn_for_effects(scan_results: &ScanResults, fn_: &CanonicalPath) -> bool {
    let Some(node) = scan_results.node_idxs.get(fn_) else {
        return true;
    };
    let graph = &scan_results.call_graph;
    let mut bfs = Bfs::new(graph, *node);

    while let Some(node) = bfs.next(graph) {
        if scan_results.fns_with_effects.contains(&graph[node]) {
            return true;
        }
    }

    false
}
