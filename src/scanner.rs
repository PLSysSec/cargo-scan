/*
    Scanner to parse a Rust source file and find all function call locations.
*/

use crate::effect::BlockType;

use super::effect::{
    EffectBlock, EffectInstance, FnDec, SrcLoc, TraitDec, TraitImpl, Visibility,
};
use super::ident;
use super::util::infer;

use anyhow::{anyhow, Result};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use syn::spanned::Spanned;
use walkdir::WalkDir;

/// Results of a scan
#[derive(Debug)]
pub struct ScanResults {
    pub effects: Vec<EffectInstance>,
    pub effect_blocks: Vec<EffectBlock>,

    pub unsafe_traits: Vec<TraitDec>,
    pub unsafe_impls: Vec<TraitImpl>,

    pub fn_locs: HashMap<ident::Path, SrcLoc>,
    pub pub_fns: Vec<ident::Path>,

    pub call_graph: DiGraph<ident::Path, SrcLoc>,

    pub skipped_macros: usize,
    pub skipped_fn_calls: usize,
}

impl ScanResults {
    fn _new() -> Self {
        ScanResults {
            effects: Vec::new(),
            effect_blocks: Vec::new(),
            unsafe_traits: Vec::new(),
            unsafe_impls: Vec::new(),
            fn_locs: HashMap::new(),
            pub_fns: Vec::new(),
            call_graph: DiGraph::new(),
            skipped_macros: 0,
            skipped_fn_calls: 0,
        }
    }

    fn _combine_results(&mut self, other: &mut Self) {
        let ScanResults {
            effects: o_effects,
            effect_blocks: o_effect_blocks,
            unsafe_traits: o_unsafe_traits,
            unsafe_impls: o_unsafe_impls,
            fn_locs: o_fn_locs,
            pub_fns: o_pub_fns,
            call_graph: _o_call_graph,
            skipped_macros: o_skipped_macros,
            skipped_fn_calls: o_skipped_fn_calls,
        } = other;

        self.effects.append(o_effects);
        self.effect_blocks.append(o_effect_blocks);
        self.unsafe_traits.append(o_unsafe_traits);
        self.unsafe_impls.append(o_unsafe_impls);
        self.fn_locs.extend(o_fn_locs.drain());
        self.pub_fns.append(o_pub_fns);
        self.skipped_macros += *o_skipped_macros;
        self.skipped_fn_calls += *o_skipped_fn_calls;
    }

    pub fn dangerous_effects(&self) -> HashSet<&EffectInstance> {
        self.effects.iter().filter(|x| x.pattern().is_some()).collect::<HashSet<_>>()
    }

    pub fn unsafe_effect_blocks_set(&self) -> HashSet<&EffectBlock> {
        self.effect_blocks
            .iter()
            .filter(|x| match x.block_type() {
                BlockType::UnsafeExpr | BlockType::UnsafeFn => true,
                BlockType::NormalFn => false,
            })
            .collect::<HashSet<_>>()
    }

    pub fn get_callers<'a>(
        &'a self,
        callee: &ident::Path,
    ) -> HashSet<&'a EffectInstance> {
        let mut callers = HashSet::new();
        for e in &self.effects {
            // TODO: Update this when we get more intelligent name resolution
            let effect_callee = e.callee();
            if effect_callee == callee {
                callers.insert(e);
            }
        }

        callers
    }
}

impl From<ScanData> for ScanResults {
    fn from(data: ScanData) -> Self {
        ScanResults {
            effects: data.effects,
            effect_blocks: data.effect_blocks,
            unsafe_traits: data.unsafe_traits,
            unsafe_impls: data.unsafe_impls,
            pub_fns: data
                .fn_decls
                .iter()
                .filter_map(|fun| match fun.vis {
                    Visibility::Public => Some(fun.fn_name.clone()),
                    Visibility::Private => None,
                })
                .collect(),
            fn_locs: data
                .fn_decls
                .into_iter()
                .map(|FnDec { src_loc, fn_name, vis: _ }| (fn_name, src_loc))
                .collect::<HashMap<_, _>>(),
            call_graph: data.call_graph,
            skipped_macros: data.skipped_macros,
            skipped_fn_calls: data.skipped_fn_calls,
        }
    }
}

/// Holds the intermediate state between scans which doesn't hold references
/// to file data
#[derive(Debug, Default)]
pub struct ScanData {
    effects: Vec<EffectInstance>,
    effect_blocks: Vec<EffectBlock>,
    unsafe_traits: Vec<TraitDec>,
    unsafe_impls: Vec<TraitImpl>,
    // Saved function declarations
    fn_decls: Vec<FnDec>,

    call_graph: DiGraph<ident::Path, SrcLoc>,
    node_idxs: HashMap<ident::Path, NodeIndex>,
    // info about skipped nodes
    skipped_macros: usize,
    skipped_fn_calls: usize,
}

impl ScanData {
    pub fn new() -> Self {
        ScanData {
            effects: Vec::new(),
            effect_blocks: Vec::new(),
            unsafe_traits: Vec::new(),
            unsafe_impls: Vec::new(),
            fn_decls: Vec::new(),
            call_graph: DiGraph::new(),
            node_idxs: HashMap::new(),
            skipped_macros: 0,
            skipped_fn_calls: 0,
        }
    }
}

/// Stateful object to scan Rust source code for effects (fn calls of interest)
#[derive(Debug)]
pub struct Scanner<'a, 'b> {
    // filepath that the scanner is being run on
    filepath: &'a Path,

    // stack-based scopes for parsing (always empty at top-level)
    scope_mods: Vec<&'a syn::Ident>,
    scope_use: Vec<&'a syn::Ident>,
    scope_fun: Vec<&'a syn::Ident>,
    scope_effect_blocks: Vec<EffectBlock>,

    // Number of unsafe keywords the current scope is nested inside
    // (includes only unsafe blocks and fn decls -- unsafe traits and
    // unsafe impls don't alone imply unsafe operations)
    scope_unsafe: usize,

    // collecting use statements that are in scope
    use_names: HashMap<&'a syn::Ident, Vec<&'a syn::Ident>>,
    use_globs: Vec<Vec<&'a syn::Ident>>,

    // Saved FFI declarations
    ffi_decls: HashMap<&'a syn::Ident, ident::Path>,

    data: &'b mut ScanData,
}

impl<'a, 'b> Scanner<'a, 'b> {
    /*
        Main public API
    */
    /// Create a new scanner tied to a filepath
    pub fn new(filepath: &'a Path, data: &'b mut ScanData) -> Self {
        Self {
            filepath,
            ffi_decls: HashMap::new(),
            scope_mods: Vec::new(),
            scope_use: Vec::new(),
            scope_fun: Vec::new(),
            scope_effect_blocks: Vec::new(),
            scope_unsafe: 0,
            use_names: HashMap::new(),
            use_globs: Vec::new(),
            data,
        }
    }

    /// Top-level invariant -- called before consuming results
    pub fn assert_top_level_invariant(&self) {
        debug_assert!(self.scope_mods.is_empty());
        debug_assert!(self.scope_use.is_empty());
        debug_assert!(self.scope_fun.is_empty());
        debug_assert!(self.scope_effect_blocks.is_empty());
        debug_assert_eq!(self.scope_unsafe, 0);
    }

    /*
        Additional top-level items and modules

        These are public, with the caveat that the Scanner currently assumes
        they are all run on syntax within the same file.
    */
    pub fn scan_file(&mut self, f: &'a syn::File) {
        // scan the file and return a list of all calls in it
        for i in &f.items {
            self.scan_item(i);
        }
    }

    pub fn scan_item(&mut self, i: &'a syn::Item) {
        match i {
            syn::Item::Mod(m) => self.scan_mod(m),
            syn::Item::Use(u) => self.scan_use(u),
            syn::Item::Impl(imp) => self.scan_impl(imp),
            syn::Item::Fn(fun) => self.scan_fn_decl(fun),
            syn::Item::Trait(t) => self.scan_trait(t),
            syn::Item::ForeignMod(fm) => self.scan_foreign_mod(fm),
            syn::Item::Macro(_) => {
                self.data.skipped_macros += 1;
            }
            _ => (),
            // For all syntax elements see
            // https://docs.rs/syn/latest/syn/enum.Item.html
            // Potentially interesting:
            // Const(ItemConst), Static(ItemStatic) -- for information flow
        }
    }

    pub fn scan_mod(&mut self, m: &'a syn::ItemMod) {
        // TBD: reset use state; handle super keywords
        if let Some((_, items)) = &m.content {
            // modules can exist inside of functions apparently
            // handling that in what seems to be the most straightforward way
            let n = self.scope_fun.len();
            self.scope_mods.append(&mut self.scope_fun);
            debug_assert!(self.scope_fun.is_empty());

            self.scope_mods.push(&m.ident);
            for i in items {
                self.scan_item(i);
            }
            self.scope_mods.pop();

            // restore scope_fun state
            for _ in 0..n {
                self.scope_fun.push(self.scope_mods.pop().unwrap());
            }
        }
    }

    /*
        Reusable warning loggers
    */

    fn warning(&self, msg: &str) {
        eprintln!("Warning: {}", msg)
    }

    fn syn_warning<S: Spanned + Debug>(&self, msg: &str, syn_node: S) {
        let file = self.filepath.to_string_lossy();
        let line = syn_node.span().start().line;
        let col = syn_node.span().start().column;
        eprintln!("Warning: {} ({:?}) ({}:{}:{})", msg, syn_node, file, line, col);
    }

    /*
        Extern blocks
    */

    fn scan_foreign_mod(&mut self, fm: &'a syn::ItemForeignMod) {
        self.scope_unsafe += 1;
        for i in &fm.items {
            self.scan_foreign_item(i);
        }
        self.scope_unsafe -= 1;
    }

    fn scan_foreign_item(&mut self, i: &'a syn::ForeignItem) {
        match i {
            syn::ForeignItem::Fn(f) => self.scan_foreign_fn(f),
            syn::ForeignItem::Macro(_) => {
                self.data.skipped_macros += 1;
            }
            _ => {}
        }
        // Ignored: Static, Type, Macro, Verbatim
        // https://docs.rs/syn/latest/syn/enum.ForeignItem.html
    }

    fn scan_foreign_fn(&mut self, f: &'a syn::ForeignItemFn) {
        let fn_name = &f.sig.ident;
        let fn_path = self.lookup_fn_decl(fn_name);
        self.ffi_decls.insert(fn_name, fn_path);
    }

    /*
        Use statements
    */
    fn scope_use_snapshot(&self) -> Vec<&'a syn::Ident> {
        self.scope_use.clone()
    }
    fn save_scope_use_under(&mut self, lookup_key: &'a syn::Ident) {
        // save the use scope under an identifier/lookup key
        let v_new = self.scope_use_snapshot();
        if cfg!(debug) && self.use_names.contains_key(lookup_key) {
            let v_old = self.use_names.get(lookup_key).unwrap();
            if *v_old != v_new {
                let msg = format!(
                    "Name conflict found in use scope: {:?} (old: {:?} new: {:?})",
                    lookup_key, v_old, v_new
                );
                self.warning(&msg);
            }
        }
        self.use_names.insert(lookup_key, v_new);
    }
    fn scan_use(&mut self, u: &'a syn::ItemUse) {
        // TBD: may need to do something special here if already inside a fn
        // (scope_fun is nonempty)
        self.scan_use_tree(&u.tree);
    }
    fn scan_use_tree(&mut self, u: &'a syn::UseTree) {
        match u {
            syn::UseTree::Path(p) => self.scan_use_path(p),
            syn::UseTree::Name(n) => self.scan_use_name(n),
            syn::UseTree::Rename(r) => self.scan_use_rename(r),
            syn::UseTree::Glob(g) => self.scan_use_glob(g),
            syn::UseTree::Group(g) => self.scan_use_group(g),
        }
    }
    fn scan_use_path(&mut self, p: &'a syn::UsePath) {
        self.scope_use.push(&p.ident);
        self.scan_use_tree(&p.tree);
        self.scope_use.pop();
    }
    fn scan_use_name(&mut self, n: &'a syn::UseName) {
        self.scope_use.push(&n.ident);
        self.save_scope_use_under(&n.ident);
        self.scope_use.pop();
    }
    fn scan_use_rename(&mut self, r: &'a syn::UseRename) {
        self.scope_use.push(&r.ident);
        self.save_scope_use_under(&r.rename);
        self.scope_use.pop();
    }

    fn scan_use_glob(&mut self, _g: &'a syn::UseGlob) {
        self.use_globs.push(self.scope_use_snapshot());
    }

    fn scan_use_group(&mut self, g: &'a syn::UseGroup) {
        for t in g.items.iter() {
            self.scan_use_tree(t);
        }
    }

    /*
        Name resolution methods
    */

    // weird signature: need a double reference on i because i is owned by cur function
    // all hail the borrow checker for catching this error
    fn lookup_ident<'c>(&'c self, i: &'c &'a syn::Ident) -> &'c [&'a syn::Ident]
    where
        'a: 'b,
    {
        self.use_names
            .get(i)
            .map(|v| v.as_slice())
            .unwrap_or_else(|| std::slice::from_ref(i))
    }

    // this one creates a new path, so it has to return a Vec anyway
    // precond: input path must be nonempty
    // return: nonempty Vec of identifiers in the full path
    fn lookup_path(&self, p: &'a syn::Path) -> Vec<&'a syn::Ident> {
        let mut result = Vec::new();
        let mut it = p.segments.iter().map(|seg| &seg.ident);

        // first part of the path based on lookup
        let fst: &'a syn::Ident = it.next().unwrap();
        result.extend(self.lookup_ident(&fst));
        // second part of the path based on any additional sub-scoping
        result.extend(it);

        result
    }

    fn get_mod_scope(&self) -> ident::Path {
        let mods: Vec<String> = self.scope_mods.iter().map(|i| i.to_string()).collect();
        ident::Path::new_owned(mods.join("::"))
    }

    fn get_fun_scope(&self) -> Option<ident::Ident> {
        self.scope_fun.last().cloned().map(Self::syn_to_ident)
    }

    fn syn_to_ident(i: &syn::Ident) -> ident::Ident {
        ident::Ident::new_owned(i.to_string())
    }

    fn syn_to_path(i: &syn::Ident) -> ident::Path {
        ident::Path::from_ident(Self::syn_to_ident(i))
    }

    fn aggregate_path(p: &[&'a syn::Ident]) -> ident::Path {
        let mut result = ident::Path::new_empty();
        for &i in p {
            result.push_ident(&Self::syn_to_ident(i));
        }
        result
    }

    fn lookup_fn_decl(&self, i: &'a syn::Ident) -> ident::Path {
        // TODO after name resolution is working:
        // decide which of these we actually need and make it robust
        let mut result =
            ident::Path::new_owned(infer::fully_qualified_prefix(self.filepath));
        result.append(&Self::aggregate_path(self.lookup_ident(&i)));
        result
    }

    fn lookup_ffi(&self, ffi: &syn::Ident) -> Option<ident::Path> {
        // TBD
        self.ffi_decls.get(ffi).cloned()
    }

    fn lookup_method(&self, i: &syn::Ident) -> ident::Path {
        ident::Path::new_owned(format!("[METHOD]::{}", i))
    }

    fn lookup_field(&self, i: &syn::Ident) -> ident::Path {
        ident::Path::new_owned(format!("[FIELD]::{}", i))
    }

    fn lookup_field_index(&self, idx: &syn::Index) -> ident::Path {
        ident::Path::new_owned(format!("[FIELD]::{}", idx.index))
    }

    fn lookup_expr_path(
        &self,
        expr_path: &'a syn::ExprPath,
    ) -> (&'a syn::Ident, ident::Path) {
        let callee_vec = self.lookup_path(&expr_path.path);
        let callee_ident = *callee_vec.last().unwrap();
        (callee_ident, Self::aggregate_path(&callee_vec))
    }

    fn lookup_caller(&self) -> ident::CanonicalPath {
        // Hacky inferences
        let mut result =
            ident::CanonicalPath::new_owned(infer::infer_crate(self.filepath));
        result.append_path(&ident::Path::new_owned(
            infer::infer_module(self.filepath).join("::"),
        ));

        // Push current mod scope [ "mod1", "mod2", ...]
        result.append_path(&self.get_mod_scope());

        // Push current function
        result.push_ident(&self.get_fun_scope().expect("not inside a function!"));

        result
    }

    #[allow(dead_code, unused_variables)]
    fn resolve_ident(&self, s: &SrcLoc, i: ident::Ident) -> Option<ident::CanonicalPath> {
        todo!()
    }

    #[allow(dead_code, unused_variables)]
    fn resolve_path(&self, s: &SrcLoc, i: ident::Path) -> Option<ident::CanonicalPath> {
        todo!()
    }

    /*
        Trait declarations
    */
    fn scan_trait(&mut self, t: &'a syn::ItemTrait) {
        let t_name = Self::syn_to_path(&t.ident);
        let t_unsafety = t.unsafety;
        if t_unsafety.is_some() {
            // we found an `unsafe trait` declaration
            self.data.unsafe_traits.push(TraitDec::new(t, self.filepath, t_name));
        }
        // TBD: handle trait block, e.g. default implementations
    }

    /*
        Impl blocks
    */
    fn scan_impl(&mut self, imp: &'a syn::ItemImpl) {
        // push the impl block scope to scope_mods
        let scope_adds = if let Some((_, tr, _)) = &imp.trait_ {
            // scope trait impls under trait name
            self.scan_impl_trait_path(tr, imp)
        } else {
            // scope type impls under type name
            self.scan_impl_type(&imp.self_ty)
        };

        // scan the impl block
        for item in &imp.items {
            match item {
                syn::ImplItem::Fn(m) => {
                    self.scan_method(m);
                }
                syn::ImplItem::Macro(_) => {
                    self.data.skipped_macros += 1;
                }
                syn::ImplItem::Verbatim(v) => {
                    self.syn_warning("skipping Verbatim expression", v);
                }
                _ => (),
            }
        }

        // Drop additional scope adds
        for _ in 0..scope_adds {
            self.scope_mods.pop();
        }
    }
    fn scan_impl_type(&mut self, ty: &'a syn::Type) -> usize {
        // return: the number of items added to scope_mods
        match ty {
            syn::Type::Group(x) => self.scan_impl_type(&x.elem),
            syn::Type::Paren(x) => self.scan_impl_type(&x.elem),
            syn::Type::Path(x) => self.scan_impl_type_path(&x.path),
            syn::Type::Macro(_) => {
                self.data.skipped_macros += 1;
                0
            }
            syn::Type::TraitObject(x) => {
                self.syn_warning("skipping 'impl dyn Trait' block", x);
                0
            }
            syn::Type::Verbatim(v) => {
                self.syn_warning("skipping Verbatim expression", v);
                0
            }
            _ => {
                self.syn_warning("unexpected impl block type (ignoring)", ty);
                0
            }
        }
        // other cases -- mostly built-ins, so shouldn't really occur in
        // impl blocks; only in impl Trait for blocks, which we should handle
        // separately
        // Array(x) => {}
        // BareFn(x) => {}
        // ImplTrait(x) => {}
        // Infer(x) => {}
        // Never(x) => {}
        // Ptr(x) => {}
        // Reference(x) => {}
        // Slice(x) => {}
        // TraitObject(x) => {}
        // Tuple(x) => {}
    }
    fn scan_impl_type_path(&mut self, ty: &'a syn::Path) -> usize {
        // return: the number of items added to scope_mods
        let fullpath = self.lookup_path(ty);
        self.scope_mods.extend(&fullpath);
        if fullpath.is_empty() {
            self.syn_warning("unexpected empty impl type path", ty);
        }
        fullpath.len()
    }
    fn scan_impl_trait_path(
        &mut self,
        tr: &'a syn::Path,
        imp: &'a syn::ItemImpl,
    ) -> usize {
        // return: the number of items added to scope_mods

        if imp.unsafety.is_some() {
            // we found an `unsafe impl` declaration
            let tr_name = Self::aggregate_path(&self.lookup_path(tr));
            self.data.unsafe_impls.push(TraitImpl::new(imp, self.filepath, tr_name));
        }

        let fullpath = self.lookup_path(tr);
        self.scope_mods.extend(&fullpath);
        if fullpath.is_empty() {
            self.syn_warning("unexpected empty trait name path", tr);
        }
        fullpath.len()
    }

    /*
        Function and method declarations
    */
    fn scan_fn_decl(&mut self, f: &'a syn::ItemFn) {
        self.scan_fn(&f.sig, &f.block, &f.vis);
    }

    fn scan_method(&mut self, m: &'a syn::ImplItemFn) {
        // NB: may or may not be a method, if there is no self keyword
        self.scan_fn(&m.sig, &m.block, &m.vis);
    }

    fn scan_fn(
        &mut self,
        f_sig: &'a syn::Signature,
        body: &'a syn::Block,
        vis: &'a syn::Visibility,
    ) {
        let f_ident = &f_sig.ident;
        let f_name = Self::syn_to_path(f_ident);
        // TBD
        let f_name_full = self.lookup_fn_decl(f_ident);
        let f_unsafety: &Option<syn::token::Unsafe> = &f_sig.unsafety;

        let fn_dec = FnDec::new(self.filepath, f_sig, f_name_full, vis);
        let node_idx = self.data.call_graph.add_node(fn_dec.fn_name.clone());
        self.data.node_idxs.insert(fn_dec.fn_name.clone(), node_idx);
        // NOTE: always push the new function declaration before scanning the
        //       body so we have access to the function its in for unsafe blocks
        self.data.fn_decls.push(fn_dec);

        let effect_block = if f_unsafety.is_some() {
            // we found an `unsafe fn` declaration
            self.scope_unsafe += 1;

            EffectBlock::new_unsafe_fn(self.filepath, body, f_name, vis)
        } else {
            EffectBlock::new_fn(self.filepath, body, f_name, vis)
        };
        self.scope_effect_blocks.push(effect_block);
        self.scope_fun.push(f_ident);
        for s in &body.stmts {
            self.scan_fn_statement(s);
        }
        self.scope_fun.pop();
        self.data.effect_blocks.push(self.scope_effect_blocks.pop().unwrap());
        if f_unsafety.is_some() {
            debug_assert!(self.scope_unsafe >= 1);
            self.scope_unsafe -= 1;
        }
    }
    fn scan_fn_statement(&mut self, s: &'a syn::Stmt) {
        match s {
            syn::Stmt::Local(l) => self.scan_fn_local(l),
            syn::Stmt::Expr(e, _semi) => self.scan_expr(e),
            syn::Stmt::Item(i) => self.scan_item_in_fn(i),
            syn::Stmt::Macro(_) => {
                self.data.skipped_macros += 1;
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
                self.scan_expr(&x.left);
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
                self.scan_expr(&x.expr);
            }
            syn::Expr::Closure(x) => {
                // TBD: closures are a bit weird!
                // Note that the body expression doesn't get evaluated yet,
                // and may be evaluated somewhere else.
                // May need to do something more special here.
                self.scan_expr(&x.body);
            }
            syn::Expr::Continue(_) => (),
            syn::Expr::Field(x) => {
                self.scan_expr(&x.base);
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
            syn::Expr::Macro(_) => {
                self.data.skipped_macros += 1;
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
            syn::Expr::Path(_) => {
                // typically a local variable
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
                // TODO: Once we have type info, check to see if we deref a
                //       pointer here
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

    fn scan_unsafe_block(&mut self, x: &'a syn::ExprUnsafe) {
        self.scope_unsafe += 1;

        // We will always be in a function definition inside of a block, so it
        // is safe to unwrap the last fn_decl
        let effect_block = EffectBlock::new_unsafe_expr(
            self.filepath,
            &x.block,
            self.data.fn_decls.last().unwrap().clone(),
        );
        self.scope_effect_blocks.push(effect_block);
        for s in &x.block.stmts {
            self.scan_fn_statement(s);
        }
        self.data.effect_blocks.push(self.scope_effect_blocks.pop().unwrap());

        self.scope_unsafe -= 1;
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

    /// push an Effect to the list of results based on this call site.
    fn push_callsite<S>(
        &mut self,
        callee_span: S,
        callee: ident::Path,
        ffi: Option<ident::Path>,
    ) where
        S: Debug + Spanned,
    {
        let caller = self.lookup_caller();

        // TBD: unwraps were causing `make test` to crash
        if let Some(caller_node_idx) = self.data.node_idxs.get(caller.as_path()) {
            if let Some(callee_node_idx) = self.data.node_idxs.get(&callee) {
                self.data.call_graph.add_edge(
                    *caller_node_idx,
                    *callee_node_idx,
                    SrcLoc::from_span(self.filepath, &callee_span.span()),
                );
            }
        }

        let eff = EffectInstance::new_call(
            self.filepath,
            caller,
            callee,
            &callee_span,
            self.scope_unsafe > 0,
            ffi,
        );
        if let Some(effect_block) = self.scope_effect_blocks.last_mut() {
            effect_block.push_effect(eff.clone())
        } else {
            self.syn_warning(
                "Unexpected function call site found outside an effect block",
                callee_span,
            );
        }
        self.data.effects.push(eff);
    }

    fn scan_expr_call(&mut self, f: &'a syn::Expr) {
        match f {
            syn::Expr::Path(p) => {
                let (span, callee) = self.lookup_expr_path(p);
                let ffi = self.lookup_ffi(span);
                self.push_callsite(span, callee, ffi);
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
            syn::Expr::Macro(_) => {
                self.data.skipped_macros += 1;
            }
            _ => {
                // anything else could be a function, too -- could return a closure
                // or fn pointer. No way to tell w/o type information.
                self.data.skipped_fn_calls += 1;
            }
        }
    }

    fn scan_expr_call_field(&mut self, m: &'a syn::Member) {
        match m {
            syn::Member::Named(i) => {
                self.push_callsite(i, self.lookup_field(i), None);
            }
            syn::Member::Unnamed(idx) => {
                self.push_callsite(idx, self.lookup_field_index(idx), None);
            }
        }
    }

    fn scan_expr_call_method(&mut self, i: &'a syn::Ident) {
        self.push_callsite(i, self.lookup_method(i), self.lookup_ffi(i));
    }
}

/// Load the Rust file at the filepath and scan it
pub fn load_and_scan(filepath: &Path, scan_data: &mut ScanData) -> Result<()> {
    // based on example at https://docs.rs/syn/latest/syn/struct.File.html
    let mut file = File::open(filepath)?;

    let mut src = String::new();
    file.read_to_string(&mut src)?;

    let syntax_tree = syn::parse_file(&src)?;

    let mut scanner = Scanner::new(filepath, scan_data);
    scanner.scan_file(&syntax_tree);

    Ok(())
}

/// Scan the supplied crate
pub fn scan_crate(crate_path: &Path) -> Result<ScanResults> {
    // Make sure the path is a crate
    if !crate_path.is_dir() {
        return Err(anyhow!("Path is not a crate; not a directory: {:?}", crate_path));
    }

    let mut cargo_toml_path = crate_path.to_path_buf();
    cargo_toml_path.push("Cargo.toml");
    if !cargo_toml_path.try_exists()? || !cargo_toml_path.is_file() {
        return Err(anyhow!("Path is not a crate; missing Cargo.toml: {:?}", crate_path));
    }

    let mut scan_data = ScanData::new();

    // TODO: For now, only walking through the src dir, but might want to
    //       include others (e.g. might codegen in other dirs)
    // We have a valid crate, so iterate through all the rust src
    for entry in WalkDir::new(crate_path.join(Path::new("src")))
        .sort_by_file_name()
        .into_iter()
        .filter(|e| match e {
            Ok(ne) if ne.path().is_file() => {
                let fname = Path::new(ne.file_name());
                fname.to_str().map_or(false, |x| x.ends_with(".rs"))
            }
            _ => false,
        })
    {
        load_and_scan(Path::new(entry?.path()), &mut scan_data)?;
    }

    Ok(scan_data.into())
}
