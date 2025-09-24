//! Scanner API
//!
//! Parse a Rust crate or source file and collect effect blocks, function calls, and
//! various other information.

use crate::attr_parser::CfgPred;
use crate::audit_file::EffectInfo;
use crate::macro_expand::handle_macro_expansion;
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
use proc_macro2::{TokenStream, TokenTree};
use quote::ToTokens;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::path::Path as FilePath;
use syn::spanned::Spanned;
use syn::ForeignItemFn;

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

    pub call_graph: DiGraph<CanonicalPath, SrcLoc>,
    pub node_idxs: HashMap<CanonicalPath, NodeIndex>,
    // To store call information for macro, Putting all call info in the could make some effect unrecognizable
    pub macro_call_graph: DiGraph<CanonicalPath, SrcLoc>,
    pub macro_node_idxs: HashMap<CanonicalPath, NodeIndex>,

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
        // First, attempt to find the callee node in `node_idxs`
        let callee_node = if let Some(node) = self.node_idxs.get(callee) {
            *node
        } else {
            // If not found in `node_idxs`, try `macro_node_idxs`
            self.macro_node_idxs
                .get(callee)
                .context(format!(
                    "Missing callee in both results graph and macro results graph: {:?}, {:?}",
                    callee,
                    self.macro_node_idxs
                ))?
                .to_owned()
        };

        // Try to collect edges from the primary `call_graph`
        let effects = if let Some(_edges) =
            self.call_graph.edges_directed(callee_node, Direction::Incoming).next()
        {
            self.call_graph
                .edges_directed(callee_node, Direction::Incoming)
                .map(|e| {
                    let caller_node = e.source();
                    let caller = self.call_graph[caller_node].clone();
                    let src_loc = e.weight();
                    EffectInfo::new(caller, src_loc.clone())
                })
                .collect::<HashSet<_>>()
        } else {
            // If no edges found in `call_graph`, try `macro_call_graph`
            self.macro_call_graph
                .edges_directed(callee_node, Direction::Incoming)
                .map(|e| {
                    let caller_node = e.source();
                    let caller = self.macro_call_graph[caller_node].clone();
                    let src_loc = e.weight();
                    EffectInfo::new(caller, src_loc.clone())
                })
                .collect::<HashSet<_>>()
        };

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

    pub fn combine_scan_results(&mut self, other: ScanResults) {
        // Merge `Vec` fields
        self.effects.extend(other.effects);
        self.fn_ptr_effects.extend(other.fn_ptr_effects);

        // Merge `HashSet` fields
        self.pub_fns.extend(other.pub_fns);
        self.trait_meths.extend(other.trait_meths);
        self.fns_with_effects.extend(other.fns_with_effects);

        // Merge `HashMap` fields
        self.fn_locs.extend(other.fn_locs);
        self.fn_loc_tracker.extend(other.fn_loc_tracker);

        // Merge `DiGraph` fields
        self.macro_call_graph = other.call_graph.clone();
        self.macro_node_idxs = other.node_idxs.clone();
        // Merge `LoCTracker` fields
        self.total_loc.merge(other.total_loc);
        self.skipped_macros.merge(other.skipped_macros);
        self.skipped_conditional_code.merge(other.skipped_conditional_code);
        self.skipped_fn_calls.merge(other.skipped_fn_calls);
        self.skipped_fn_ptrs.merge(other.skipped_fn_ptrs);
        self.skipped_other.merge(other.skipped_other);
        self.unsafe_traits.merge(other.unsafe_traits);
        self.unsafe_impls.merge(other.unsafe_impls);
        self._effects_loc.merge(other._effects_loc);
        self._skipped_build_rs.merge(other._skipped_build_rs);
    }
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

    current_macro_context: Option<String>,

    current_macro_call_loc: Option<SrcLoc>,
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
            current_macro_context: None,
            current_macro_call_loc: None,
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

    pub fn set_current_macro_context(&mut self, macro_context: Option<String>) {
        self.current_macro_context = macro_context;
    }

    pub fn set_current_macro_call_loc(&mut self, macro_loc: Option<SrcLoc>) {
        self.current_macro_call_loc = macro_loc;
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
        // self.process_macros();
    }

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
                self.data.skipped_macros.add(m);
                if let Some(ident) = m.ident.as_ref() {
                    let cp = self.resolver.resolve_def(ident);
                    self.data.update_call_graph(&cp);
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
                self.data.skipped_macros.add(m);
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
                    self.data.skipped_macros.add(m);
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
                    self.data.skipped_macros.add(m);
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
                self.data.skipped_macros.add(m);
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
                self.data.skipped_macros.add(m);
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
        let caller: CanonicalPath = if let Some(ref macro_context) =
            self.current_macro_context
        {
            CanonicalPath::new(&macro_context.clone())
        } else if eff_type.is_ffi_decl() {
            callee.clone()
        } else {
            let containing_fn = self.scope_fns.last().expect("not inside a function!");
            containing_fn.fn_name.clone()
        };

        let eff = EffectInstance::new_effect(
            self.filepath,
            caller.clone(),
            callee.clone(),
            &eff_span,
            eff_type.clone(),
            self.current_macro_call_loc.clone(),
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
        let caller: CanonicalPath = if let Some(ref macro_context) =
            self.current_macro_context
        {
            CanonicalPath::new(&macro_context.clone())
        } else {
            let containing_fn = self.scope_fns.last().expect("not inside a function!");
            containing_fn.fn_name.clone()
        };

        let eff_opt = if let Some(macro_loc) = self.current_macro_call_loc.clone() {
            self.data.add_call(&caller, &callee, macro_loc.clone());

            EffectInstance::new_call_macro(
                caller.clone(),
                callee,
                macro_loc,
                is_unsafe,
                ffi,
                &self.sinks,
            )
        } else {
            let src_loc = SrcLoc::from_span(self.filepath, &callee_span);
            self.data.add_call(&caller, &callee, src_loc.clone());

            EffectInstance::new_call_normal(
                self.filepath,
                caller.clone(),
                callee,
                &callee_span,
                is_unsafe,
                ffi,
                &self.sinks,
            )
        };
        let Some(eff) = eff_opt else {
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
                self.data.skipped_macros.add(m);
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

pub struct ScanConfig<'a> {
    enabled_cfg: &'a HashMap<String, Vec<String>>,
    expand_macro: bool,
    quick_mode: bool,
}

/// Load the Rust file at the filepath and scan it
pub fn scan_file(
    crate_name: &str,
    filepath: &FilePath,
    resolver: &Resolver,
    scan_results: &mut ScanResults,
    macro_scan_results: &mut ScanResults,
    sinks: HashSet<IdentPath>,
    scan_config: &ScanConfig,
) -> Result<()> {
    let enabled_cfg = scan_config.enabled_cfg;
    let expand_macro = scan_config.expand_macro;
    debug!("Scanning file: {:?}", filepath);
    // Load file contents
    let mut file = File::open(filepath)?;
    let mut src = String::new();
    file.read_to_string(&mut src)?;
    let syntax_tree = syn::parse_file(&src)?;
    // Initialize resolver
    let file_resolver = FileResolver::new(crate_name, resolver, filepath, None)?;
    // Initialize scanner
    let mut scanner = Scanner::new(filepath, file_resolver, scan_results, enabled_cfg);
    scanner.add_sinks(sinks.clone());
    scanner.scan_file(&syntax_tree);
    if expand_macro {
        handle_macro_expansion(
            crate_name,
            filepath,
            resolver,
            macro_scan_results,
            sinks,
            enabled_cfg,
        )
        .unwrap_or_else(|err| {
            info!("Failed to scan file: {} ({})", filepath.to_string_lossy(), err);
        });
    }
    Ok(())
}

/// Try to run scan_file, reporting any errors back to the user
pub fn try_scan_file(
    crate_name: &str,
    filepath: &FilePath,
    resolver: &Resolver,
    scan_results: &mut ScanResults,
    macro_scan_results: &mut ScanResults,
    sinks: HashSet<IdentPath>,
    scan_config: &ScanConfig,
) {
    let enabled_cfg = scan_config.enabled_cfg;
    let quick_mode = scan_config.quick_mode;
    if quick_mode {
        scan_file_quick(crate_name, filepath, scan_results, sinks, enabled_cfg)
            .unwrap_or_else(|err| {
                info!("Failed to scan file {} ({})", filepath.to_string_lossy(), err);
            })
    } else {
        scan_file(
            crate_name,
            filepath,
            resolver,
            scan_results,
            macro_scan_results,
            sinks,
            scan_config,
        )
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
    expand_macro: bool,
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
    let mut macro_scan_results = ScanResults::new();

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
    let scan_config = ScanConfig { enabled_cfg: &enabled_cfg, expand_macro, quick_mode };
    // scan every file
    for entry in file_iter {
        try_scan_file(
            &crate_name,
            entry.as_path(),
            &resolver,
            &mut scan_results,
            &mut macro_scan_results,
            sinks.clone(),
            &scan_config,
        );
    }
    filter_fn_ptr_effects(&mut scan_results, crate_name.clone());
    filter_fn_ptr_effects(&mut macro_scan_results, crate_name);
    scan_results
        .effects
        .retain(|e| EffectType::matches_effect(relevant_effects, e.eff_type()));
    macro_scan_results
        .effects
        .retain(|e| EffectType::matches_effect(relevant_effects, e.eff_type()));
    scan_results.combine_scan_results(macro_scan_results);
    Ok(scan_results)
}

/// Scan the supplied crate
pub fn scan_crate(
    crate_path: &FilePath,
    relevant_effects: &[EffectType],
    quick_mode: bool,
    expand_macro: bool,
) -> Result<ScanResults> {
    scan_crate_with_sinks(
        crate_path,
        HashSet::new(),
        relevant_effects,
        quick_mode,
        expand_macro,
    )
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
    let Some(node) =
        scan_results.node_idxs.get(fn_).or_else(|| scan_results.macro_node_idxs.get(fn_))
    else {
        return true;
    };

    // Determine the appropriate graph based on where the node was found
    let graph = if scan_results.node_idxs.contains_key(fn_) {
        &scan_results.call_graph
    } else {
        &scan_results.macro_call_graph
    };

    let mut bfs = Bfs::new(graph, *node);

    // Traverse the graph using BFS
    while let Some(node) = bfs.next(graph) {
        let path = &graph[node];
        if scan_results.fns_with_effects.contains(path) {
            return true;
        }
    }

    false
}
