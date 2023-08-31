//! Scanner API
//!
//! Parse a Rust crate or source file and collect effect blocks, function calls, and
//! various other information.

use crate::audit_file::EffectInfo;

use super::effect::{
    Effect, EffectInstance, EffectType, FnDec, SrcLoc, TraitDec, TraitImpl, Visibility,
};
use super::ident::{CanonicalPath, IdentPath};
use super::loc_tracker::LoCTracker;
use super::resolve::{FileResolver, Resolve, Resolver};
use super::sink::Sink;
use super::util;

use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
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

/// Results of a scan
///
/// Holds the intermediate state between scans which doesn't hold references
/// to file data
#[derive(Debug, Default)]
pub struct ScanResults {
    pub effects: Vec<EffectInstance>,

    pub unsafe_traits: Vec<TraitDec>,
    pub unsafe_impls: Vec<TraitImpl>,

    // Saved function declarations
    pub pub_fns: HashSet<CanonicalPath>,
    pub fn_locs: HashMap<CanonicalPath, SrcLoc>,

    pub call_graph: DiGraph<CanonicalPath, SrcLoc>,
    pub node_idxs: HashMap<CanonicalPath, NodeIndex>,

    /* Tracking lines of code (LoC) and skipped/unsupported cases */
    pub total_loc: LoCTracker,
    pub skipped_macros: LoCTracker,
    pub skipped_conditional_code: LoCTracker,
    pub skipped_fn_calls: LoCTracker,
    pub skipped_fn_ptrs: LoCTracker,
    pub skipped_other: LoCTracker,

    // TODO other cases:
    pub _effects_loc: LoCTracker,
    pub _skipped_attributes: LoCTracker,
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
        let node_idx = self.call_graph.add_node(fn_name.clone());
        self.node_idxs.insert(fn_name.clone(), node_idx);

        // Save function info
        if let Visibility::Public = f.vis {
            self.pub_fns.insert(fn_name.clone());
        }
        self.fn_locs.insert(fn_name, f.src_loc);
    }
}

/// Stateful object to scan Rust source code for effects (fn calls of interest)
#[derive(Debug)]
pub struct Scanner<'a> {
    /// filepath that the scanner is being run on
    filepath: &'a FilePath,

    /// Name resolution resolver
    resolver: FileResolver<'a>,

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
}

impl<'a> Scanner<'a> {
    /*
        Main public API
    */

    /// Create a new scanner tied to a crate and file
    pub fn new(
        filepath: &'a FilePath,
        resolver: FileResolver<'a>,
        data: &'a mut ScanResults,
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
            }
            _ => (),
            // For all syntax elements see
            // https://docs.rs/syn/latest/syn/enum.Item.html
            // Potentially interesting:
            // Const(ItemConst), Static(ItemStatic) -- for information flow
        }
    }

    // Quickfix to decide when to skip a CFG attribute
    // TODO: we need to use rust-analyzer or similar to more robustly parse attributes
    pub fn skip_cfg(&self, args: &str) -> bool {
        args.starts_with("target_os = \"linux\"") || args.starts_with("not (feature =")
    }

    // Return true if the attributes imply the code should be skipped
    pub fn skip_attr(&self, attr: &'a syn::Attribute) -> bool {
        let path = attr.path();
        // if path.is_ident("cfg_args") || path.is_ident("cfg") {
        if path.is_ident("cfg") {
            let syn::Meta::List(l) = &attr.meta else { return false };
            let args = &l.tokens;
            if self.skip_cfg(args.to_string().as_str()) {
                info!("Skipping cfg attribute: {}", args);
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
            syn::ForeignItem::Fn(f) => self.resolver.scan_foreign_fn(f),
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

    /*
        Trait declarations and impl blocks
    */

    fn scan_trait(&mut self, t: &'a syn::ItemTrait) {
        if self.skip_attrs(&t.attrs) {
            self.data.skipped_conditional_code.add(t);
            return;
        }

        let t_name = self.resolver.resolve_def(&t.ident);
        let t_unsafety = t.unsafety;
        if t_unsafety.is_some() {
            // we found an `unsafe trait` declaration
            self.data.unsafe_traits.push(TraitDec::new(t, self.filepath, t_name));
        }

        for item in &t.items {
            match item {
                syn::TraitItem::Fn(m) => {
                    self.scan_trait_method(m, &t.vis);
                }
                syn::TraitItem::Macro(m) => {
                    self.data.skipped_macros.add(m);
                }
                syn::TraitItem::Verbatim(v) => {
                    self.syn_warning("skipping Verbatim expression", v);
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
                    self.syn_warning("skipping Verbatim expression", v);
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
            let tr_name = self.resolver.resolve_path(tr);
            let self_ty = imp
                .self_ty
                .to_token_stream()
                .into_iter()
                .filter_map(|token| match token {
                    TokenTree::Ident(i) => Some(i),
                    _ => None,
                })
                .last();
            // resolve the implementing type of the trait, if there is one
            let tr_type = match &self_ty {
                Some(ident) => Some(self.resolver.resolve_ident(ident)),
                _ => None,
            };

            self.data.unsafe_impls.push(TraitImpl::new(
                imp,
                self.filepath,
                tr_name,
                tr_type,
            ));
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

    fn scan_trait_method(&mut self, m: &'a syn::TraitItemFn, vis: &'a syn::Visibility) {
        if self.skip_attrs(&m.attrs) {
            self.data.skipped_conditional_code.add(m);
            return;
        }

        // If there is a default implementation,
        // scan the function body as usual.
        // Otherwise, just create a node in the
        // call graph for the abstract trait method.
        if let Some(body) = &m.default {
            self.scan_fn(&m.sig, body, vis);
        } else {
            let f_name = self.resolver.resolve_def(&m.sig.ident);
            // Update call graph
            let node_idx = self.data.call_graph.add_node(f_name.clone());
            self.data.node_idxs.insert(f_name, node_idx);
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
                for y in x.elems.iter() {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Assign(x) => {
                self.scope_assign_lhs = true;
                self.scan_expr(&x.left);
                self.scope_assign_lhs = false;
                self.scan_expr(&x.right);
            }
            syn::Expr::Async(x) => {
                for s in &x.block.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Await(x) => {
                self.scan_expr(&x.base);
            }
            syn::Expr::Binary(x) => {
                self.scan_expr(&x.left);
                self.scan_expr(&x.right);
            }
            syn::Expr::Block(x) => {
                for s in &x.block.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Break(x) => {
                if let Some(y) = &x.expr {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Call(x) => {
                // ***** THE FIRST IMPORTANT CASE *****
                // Arguments
                self.scan_expr_call_args(&x.args);
                // Function call
                self.scan_expr_call(&x.func);
            }
            syn::Expr::Cast(x) => {
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
                // TBD: closures are a bit weird!
                // Note that the body expression doesn't get evaluated yet,
                // and may be evaluated somewhere else.
                // May need to do something more special here.
                self.scan_closure(x);
            }
            syn::Expr::Continue(_) => (),
            syn::Expr::Field(x) => {
                self.scan_expr(&x.base);
                self.scan_field_access(x);
            }
            syn::Expr::ForLoop(x) => {
                self.scan_expr(&x.expr);
                for s in &x.body.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Group(x) => {
                self.scan_expr(&x.expr);
            }
            syn::Expr::If(x) => {
                self.scan_expr(&x.cond);
                for s in &x.then_branch.stmts {
                    self.scan_fn_statement(s);
                }
                if let Some((_, y)) = &x.else_branch {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Index(x) => {
                self.scan_expr(&x.expr);
                self.scan_expr(&x.index);
            }
            syn::Expr::Let(x) => {
                self.scan_expr(&x.expr);
            }
            syn::Expr::Lit(_) => (),
            syn::Expr::Loop(x) => {
                for s in &x.body.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Macro(m) => {
                self.data.skipped_macros.add(m);
            }
            syn::Expr::Match(x) => {
                self.scan_expr(&x.expr);
                for a in &x.arms {
                    if let Some((_, y)) = &a.guard {
                        self.scan_expr(y);
                    }
                    self.scan_expr(&a.body);
                }
            }
            syn::Expr::MethodCall(x) => {
                // ***** THE SECOND IMPORTANT CASE *****
                // Receiver object
                self.scan_expr(&x.receiver);
                // Arguments
                self.scan_expr_call_args(&x.args);
                // Function call
                self.scan_expr_call_method(&x.method);
            }
            syn::Expr::Paren(x) => {
                self.scan_expr(&x.expr);
            }
            syn::Expr::Path(x) => {
                // typically a local variable
                self.scan_path(&x.path);
            }
            syn::Expr::Range(x) => {
                if let Some(y) = &x.start {
                    self.scan_expr(y);
                }
                if let Some(y) = &x.end {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Reference(x) => {
                self.scan_expr(&x.expr);
            }
            syn::Expr::Repeat(x) => {
                self.scan_expr(&x.expr);
                self.scan_expr(&x.len);
            }
            syn::Expr::Return(x) => {
                if let Some(y) = &x.expr {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Struct(x) => {
                for y in x.fields.iter() {
                    self.scan_expr(&y.expr);
                }
                if let Some(y) = &x.rest {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Try(x) => {
                self.scan_expr(&x.expr);
            }
            syn::Expr::TryBlock(x) => {
                self.syn_warning("encountered try block (unstable feature)", x);
                for y in &x.block.stmts {
                    self.scan_fn_statement(y);
                }
            }
            syn::Expr::Tuple(x) => {
                for y in x.elems.iter() {
                    self.scan_expr(y);
                }
            }
            syn::Expr::Unary(x) => {
                if let syn::UnOp::Deref(_) = x.op {
                    self.scan_deref(&x.expr);
                }
                self.scan_expr(&x.expr);
            }
            syn::Expr::Unsafe(x) => {
                // ***** THE THIRD IMPORTANT CASE *****
                self.scan_unsafe_block(x);
            }
            syn::Expr::Verbatim(v) => {
                self.syn_warning("skipping Verbatim expression", v);
            }
            syn::Expr::While(x) => {
                self.scan_expr(&x.cond);
                for s in &x.body.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Yield(x) => {
                self.syn_warning("encountered yield expression (unstable feature)", x);
                if let Some(y) = &x.expr {
                    self.scan_expr(y);
                }
            }
            _ => self.syn_warning("encountered unknown expression", e),
        }
    }

    fn scan_path(&mut self, x: &'a syn::Path) {
        let ty = self.resolver.resolve_path_type(x);
        // Function pointer creation
        if ty.is_function() || ty.is_fn_ptr() {
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
        let containing_fn = self.scope_fns.last().expect("not inside a function!");
        let caller = &containing_fn.fn_name;

        let eff = EffectInstance::new_effect(
            self.filepath,
            caller.clone(),
            callee,
            &eff_span,
            eff_type,
        );

        if self.scope_unsafe > 0 && eff.is_rust_unsafe() {
            self.scope_unsafe_effects += 1;
        }
        self.data.effects.push(eff);
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

        if let Some(caller_node_idx) = self.data.node_idxs.get(caller) {
            if let Some(callee_node_idx) = self.data.node_idxs.get(&callee) {
                self.data.call_graph.add_edge(
                    *caller_node_idx,
                    *callee_node_idx,
                    SrcLoc::from_span(self.filepath, &callee_span.span()),
                );
            }
        }

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

/// Load the Rust file at the filepath and scan it
pub fn scan_file(
    crate_name: &str,
    filepath: &FilePath,
    resolver: &Resolver,
    scan_results: &mut ScanResults,
    sinks: HashSet<IdentPath>,
) -> Result<()> {
    info!("Scanning file: {:?}", filepath);

    // Load file contents
    let mut file = File::open(filepath)?;
    let mut src = String::new();
    file.read_to_string(&mut src)?;
    let syntax_tree = syn::parse_file(&src)?;

    // Initialize data structures
    let file_resolver = FileResolver::new(crate_name, resolver, filepath)?;
    let mut scanner = Scanner::new(filepath, file_resolver, scan_results);
    scanner.add_sinks(sinks);

    // Scan file contents
    scanner.scan_file(&syntax_tree);

    Ok(())
}

/// Try to run scan_file, reporting any errors back to the user
pub fn try_scan_file(
    crate_name: &str,
    filepath: &FilePath,
    resolver: &Resolver,
    scan_results: &mut ScanResults,
    sinks: HashSet<IdentPath>,
) {
    scan_file(crate_name, filepath, resolver, scan_results, sinks).unwrap_or_else(
        |err| {
            warn!("Failed to scan file: {} ({})", filepath.to_string_lossy(), err);
        },
    );
}

/// Scan the supplied crate with an additional list of sinks
pub fn scan_crate_with_sinks(
    crate_path: &FilePath,
    sinks: HashSet<IdentPath>,
    relevant_effects: &[EffectType],
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

    let resolver = Resolver::new(crate_path)?;

    let mut scan_results = ScanResults::new();

    // TODO: For now, only walking through the src dir, but might want to
    //       include others (e.g. might codegen in other dirs)
    let src_dir = crate_path.join(FilePath::new("src"));
    if src_dir.is_dir() {
        for entry in util::fs::walk_files_with_extension(&src_dir, "rs") {
            try_scan_file(
                &crate_name,
                entry.as_path(),
                &resolver,
                &mut scan_results,
                sinks.clone(),
            );
        }
    } else {
        info!("crate has no src dir; looking for a single lib.rs file instead");
        let lib_file = crate_path.join(FilePath::new("lib.rs"));
        if lib_file.is_file() {
            try_scan_file(
                &crate_name,
                lib_file.as_path(),
                &resolver,
                &mut scan_results,
                sinks,
            );
        } else {
            warn!(
                "unable to find src dir or lib.rs file; \
                no files scanned! In crate {:?}",
                crate_path
            );
        }
    }

    scan_results
        .effects
        .retain(|e| EffectType::matches_effect(relevant_effects, e.eff_type()));

    Ok(scan_results)
}

/// Scan the supplied crate
pub fn scan_crate(
    crate_path: &FilePath,
    relevant_effects: &[EffectType],
) -> Result<ScanResults> {
    scan_crate_with_sinks(crate_path, HashSet::new(), relevant_effects)
}
