/*
    Scanner to parse a Rust source file and find all function call locations.
*/

use super::effect::{BlockDec, Effect, FFICall, FnDec, ImplDec, SrcLoc, TraitDec};
use super::sink;

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use syn::spanned::Spanned;
use walkdir::WalkDir;

/// Results of a scan
#[derive(Debug)]
pub struct ScanResults {
    pub effects: Vec<Effect>,
    pub unsafe_decls: Vec<FnDec>,
    pub unsafe_impls: Vec<ImplDec>,
    pub unsafe_traits: Vec<TraitDec>,
    pub unsafe_blocks: Vec<BlockDec>,
    pub ffi_calls: Vec<FFICall>,
    pub skipped_macros: usize,
    pub skipped_fn_calls: usize,
}

impl ScanResults {
    fn new() -> Self {
        ScanResults {
            effects: Vec::new(),
            unsafe_decls: Vec::new(),
            unsafe_impls: Vec::new(),
            unsafe_traits: Vec::new(),
            unsafe_blocks: Vec::new(),
            ffi_calls: Vec::new(),
            skipped_macros: 0,
            skipped_fn_calls: 0,
        }
    }

    fn combine_results(&mut self, other: &mut Self) {
        let ScanResults {
            effects: o_effects,
            unsafe_decls: o_unsafe_decls,
            unsafe_impls: o_unsafe_impls,
            unsafe_traits: o_unsafe_traits,
            unsafe_blocks: o_unsafe_blocks,
            ffi_calls: o_ffi_calls,
            skipped_macros: o_skipped_macros,
            skipped_fn_calls: o_skipped_fn_calls,
        } = other;

        self.effects.append(o_effects);
        self.unsafe_decls.append(o_unsafe_decls);
        self.unsafe_impls.append(o_unsafe_impls);
        self.unsafe_traits.append(o_unsafe_traits);
        self.unsafe_blocks.append(o_unsafe_blocks);
        self.ffi_calls.append(o_ffi_calls);
        self.skipped_macros += *o_skipped_macros;
        self.skipped_fn_calls += *o_skipped_fn_calls;
    }
}

// TODO: Make this a trait so we have a uniform interface between cargo-scan
//       the mirai tool

/// Stateful object to scan Rust source code for effects (fn calls of interest)
#[derive(Debug)]
pub struct Scanner<'a> {
    // filepath that the scanner is being run on
    filepath: &'a Path,
    // output
    effects: Vec<Effect>,
    unsafe_decls: Vec<FnDec>,
    unsafe_impls: Vec<ImplDec>,
    unsafe_traits: Vec<TraitDec>,
    unsafe_blocks: Vec<BlockDec>,
    ffi_decls: HashMap<String, &'a syn::Ident>,
    ffi_calls: Vec<FFICall>,
    // stack-based scopes for parsing (always empty at top-level)
    // TBD: can probably combine all types of scope into one
    scope_mods: Vec<&'a syn::Ident>,
    scope_use: Vec<&'a syn::Ident>,
    scope_fun: Vec<&'a syn::Ident>,
    scope_blocks: Vec<&'a syn::Block>,
    // collecting use statements that are in scope
    use_names: HashMap<String, Vec<&'a syn::Ident>>,
    use_globs: Vec<Vec<&'a syn::Ident>>,
    // info about skipped nodes
    skipped_macros: usize,
    skipped_fn_calls: usize,
}

impl<'a> Scanner<'a> {
    /*
        Main public API
    */
    /// Create a new scanner tied to a filepath
    pub fn new(filepath: &'a Path) -> Self {
        Self {
            filepath,
            effects: Vec::new(),
            ffi_calls: Vec::new(),
            ffi_decls: HashMap::new(),
            unsafe_decls: Vec::new(),
            unsafe_impls: Vec::new(),
            unsafe_traits: Vec::new(),
            unsafe_blocks: Vec::new(),
            scope_mods: Vec::new(),
            scope_use: Vec::new(),
            scope_fun: Vec::new(),
            scope_blocks: Vec::new(),
            use_names: HashMap::new(),
            use_globs: Vec::new(),
            skipped_macros: 0,
            skipped_fn_calls: 0,
        }
    }
    /// Results of the scan (consumes the scanner)
    pub fn get_results(self) -> ScanResults {
        ScanResults {
            effects: self.effects,
            ffi_calls: self.ffi_calls,
            unsafe_decls: self.unsafe_decls,
            unsafe_impls: self.unsafe_impls,
            unsafe_traits: self.unsafe_traits,
            unsafe_blocks: self.unsafe_blocks,
            skipped_macros: self.skipped_macros,
            skipped_fn_calls: self.skipped_fn_calls,
        }
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
            syn::Item::Fn(fun) => self.scan_fn(fun),
            syn::Item::Trait(t) => self.scan_trait(t),
            syn::Item::ForeignMod(fm) => self.scan_foreign_mod(fm),
            syn::Item::Macro(_) => {
                self.skipped_macros += 1;
            }
            _ => (),
            // For all syntax elements see
            // https://docs.rs/syn/latest/syn/enum.Item.html
            // Potentially interesting:
            // Trait(t) -- default impls are an issue here
            // ForeignMod(ItemForeignMod) -- extern items like extern "C" { ... }
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

    fn scan_foreign_mod(&mut self, fm: &'a syn::ItemForeignMod) {
        let items = &fm.items;
        for i in items {
            self.scan_foreign_item(i);
        }
    }

    fn scan_foreign_item(&mut self, i: &'a syn::ForeignItem) {
        if let syn::ForeignItem::Fn(f) = i {
            self.scan_foreign_fn(f)
        }
    }

    fn scan_foreign_fn(&mut self, f: &'a syn::ForeignItemFn) {
        let fn_name = &f.sig.ident;
        self.ffi_decls.insert(fn_name.to_string(), fn_name);
    }

    /*
        Reusable warning logger
    */
    fn syn_warning<S: Spanned + Debug>(&self, msg: &str, syn_node: S) {
        let file = self.filepath.to_string_lossy();
        let line = syn_node.span().start().line;
        let col = syn_node.span().start().column;
        eprintln!("Warning: {} ({:?}) ({}:{}:{})", msg, syn_node, file, line, col);
    }

    /*
        Use statements
    */
    fn scope_use_snapshot(&self) -> Vec<&'a syn::Ident> {
        self.scope_use.clone()
    }
    fn save_scope_use_under(&mut self, lookup_key: &'a syn::Ident) {
        // save the use scope under an identifier/lookup key
        // TBD: maybe warn if there is a name conflict
        self.use_names.insert(lookup_key.to_string(), self.scope_use_snapshot());
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

    // use map lookups
    // weird signature: need a double reference on i because i is owned by cur function
    // all hail the borrow checker for catching this error
    fn lookup_ident<'b>(&'b self, i: &'b &'a syn::Ident) -> &'b [&'a syn::Ident]
    where
        'a: 'b,
    {
        let s = i.to_string();
        self.use_names
            .get(&s)
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
    fn path_to_string(p: &[&'a syn::Ident]) -> String {
        let mut result: String = "".to_string();
        for i in p {
            result.push_str(&i.to_string());
            result.push_str("::");
        }
        assert_eq!(result.pop(), Some(':'));
        assert_eq!(result.pop(), Some(':'));
        result
    }

    /*
        Trait declarations
    */
    fn scan_trait(&mut self, t: &'a syn::ItemTrait) {
        let t_name = &t.ident;
        let t_unsafety = t.unsafety;
        if t_unsafety.is_some() {
            // we found an `unsafe trait` declaration
            self.unsafe_traits.push(TraitDec::new(t, self.filepath, t_name.to_string()));
        }
    }

    /*
        Impl blocks
    */
    fn scan_impl(&mut self, imp: &'a syn::ItemImpl) {
        // push the impl block scope to scope_mods
        let scope_adds = if let Some((_, tr, _)) = &imp.trait_ {
            // scope trait impls under trait name
            self.scan_impl_trait_path(tr)
        } else {
            // scope type impls under type name
            self.scan_impl_type(&imp.self_ty)
        };

        if imp.unsafety.is_some() {
            // we found an `unsafe impl` declaration
            if let Some((_, tr, _)) = &imp.trait_ {
                let tr_name = tr.segments[0].ident.to_string();
                self.unsafe_impls.push(ImplDec::new(imp, self.filepath, tr_name));
            }
        }

        // scan the impl block
        for item in &imp.items {
            match item {
                syn::ImplItem::Method(m) => {
                    self.scan_method(m);
                }
                syn::ImplItem::Macro(_) => {
                    self.skipped_macros += 1;
                }
                syn::ImplItem::Verbatim(v) => {
                    self.syn_warning("skipping Verbatim expression", v);
                }
                _ => (),
            }
        }

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
                self.skipped_macros += 1;
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
    fn scan_impl_trait_path(&mut self, tr: &'a syn::Path) -> usize {
        // return: the number of items added to scope_mods
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
    fn scan_fn(&mut self, f: &'a syn::ItemFn) {
        let f_name = &f.sig.ident;
        let f_unsafety: &Option<syn::token::Unsafe> = &f.sig.unsafety;
        if f_unsafety.is_some() {
            // we found an `unsafe fn` declaration
            self.unsafe_decls.push(FnDec::new(f, self.filepath, f_name.to_string()));
        }
        self.scope_fun.push(f_name);
        for s in &f.block.stmts {
            self.scan_fn_statement(s);
        }
        self.scope_fun.pop();
    }
    fn scan_method(&mut self, m: &'a syn::ImplItemMethod) {
        let m_name = &m.sig.ident;
        self.scope_fun.push(m_name);
        for s in &m.block.stmts {
            self.scan_fn_statement(s);
        }
        self.scope_fun.pop();
    }
    fn scan_fn_statement(&mut self, s: &'a syn::Stmt) {
        match s {
            syn::Stmt::Local(l) => self.scan_fn_local(l),
            syn::Stmt::Expr(e) => self.scan_expr(e),
            syn::Stmt::Semi(e, _) => self.scan_expr(e),
            syn::Stmt::Item(i) => self.scan_item_in_fn(i),
        }
    }
    fn scan_item_in_fn(&mut self, i: &'a syn::Item) {
        // TBD: may need to track additional state here
        self.scan_item(i);
    }
    fn scan_fn_local(&mut self, l: &'a syn::Local) {
        if let Some((_, b)) = &l.init {
            self.scan_expr(b)
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
            syn::Expr::AssignOp(x) => {
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
            syn::Expr::Box(x) => {
                self.syn_warning("encountered box expression (unstable feature)", x);
                self.scan_expr(&x.expr);
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
                self.skipped_macros += 1;
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
                if let Some(y) = &x.from {
                    self.scan_expr(y);
                }
                if let Some(y) = &x.to {
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
            syn::Expr::Type(x) => {
                self.scan_expr(&x.expr);
            }
            syn::Expr::Unary(x) => {
                self.scan_expr(&x.expr);
            }
            syn::Expr::Unsafe(x) => {
                // Add code here to gather unsafe blocks.
                let unsafe_block = &x.block;
                self.scope_blocks.push(unsafe_block);
                self.unsafe_blocks.push(BlockDec::new(unsafe_block, self.filepath));
                for s in &x.block.stmts {
                    self.scan_fn_statement(s);
                }
                self.scope_blocks.pop();
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
    fn get_mod_scope(&self) -> Vec<String> {
        self.scope_mods.iter().map(|i| i.to_string()).collect()
    }
    fn push_callsite<S>(&mut self, callee_span: S, callee_path: String)
    where
        S: Spanned,
    {
        // push an Effect to the list of results based on this call site.

        // caller
        let caller_name = self
            .scope_fun
            .last()
            .expect("push_callsite called outside of a function!")
            .to_string();

        let mod_scope = self.get_mod_scope();

        // callee
        let call_line = callee_span.span().start().line;
        let call_col = callee_span.span().start().column;

        self.effects.push(Effect::new(
            caller_name,
            callee_path,
            self.filepath,
            &mod_scope,
            call_line,
            call_col,
        ));
    }
    fn get_block_loc(filepath: &Path, b: &syn::Block) -> SrcLoc {
        let line = b.span().start().line;
        let col = b.span().start().column;

        SrcLoc::new(filepath, line, col)
    }
    fn push_ffi_call_to_unsafe_block(&mut self, ffi_call: &FFICall) {
        let index = self.scope_blocks.len() - 1;
        let b = self.scope_blocks.get_mut(index).unwrap();
        let cur_block_loc = Scanner::<'a>::get_block_loc(self.filepath, b);

        for b in &mut self.unsafe_blocks {
            let src_loc = b.get_src_loc();
            if cur_block_loc == *src_loc {
                b.add_ffi_call(FFICall::clone(ffi_call));
                break;
            }
        }
    }
    fn scan_expr_call(&mut self, f: &'a syn::Expr) {
        match f {
            syn::Expr::Path(p) => {
                let callee_path = self.lookup_path(&p.path);
                let callee_ident = callee_path.last().unwrap();
                let fn_name = callee_ident.to_string();
                if let Some(&dec_loc) = self.ffi_decls.get(&fn_name) {
                    let ffi_call =
                        FFICall::new(callee_ident, &dec_loc, self.filepath, fn_name);
                    self.push_ffi_call_to_unsafe_block(&ffi_call);
                    self.ffi_calls.push(ffi_call);
                }
                let callee_path_str = Self::path_to_string(&callee_path);
                self.push_callsite(callee_ident, callee_path_str);
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
                self.skipped_macros += 1;
            }
            _ => {
                // anything else could be a function, too -- could return a closure
                // or fn pointer. No way to tell w/o type information.
                self.skipped_fn_calls += 1;
            }
        }
    }
    fn scan_expr_call_field(&mut self, m: &'a syn::Member) {
        match m {
            syn::Member::Named(i) => {
                let callee_path = format!("[FIELD]::{}", i);
                self.push_callsite(i, callee_path);
            }
            syn::Member::Unnamed(idx) => {
                let callee_path = format!("[FIELD]::{}", idx.index);
                self.push_callsite(idx, callee_path);
            }
        }
    }
    fn scan_expr_call_method(&mut self, i: &'a syn::Ident) {
        let callee_path = format!("[METHOD]::{}", i);
        self.push_callsite(i, callee_path);
    }
}

/// Load the Rust file at the filepath and scan it
pub fn load_and_scan(filepath: &Path) -> ScanResults {
    // based on example at https://docs.rs/syn/latest/syn/struct.File.html
    let mut file = File::open(filepath).unwrap_or_else(|err| {
        panic!("scanner.rs: Error: Unable to open file: {:?} ({:?})", filepath, err)
    });

    let mut src = String::new();
    file.read_to_string(&mut src).unwrap_or_else(|err| {
        panic!("scanner.rs: Error: Unable to read file: {:?} ({:?})", filepath, err)
    });

    let syntax_tree = syn::parse_file(&src).unwrap_or_else(|err| {
        panic!("scanner.rs: Error: Unable to parse file: {:?} ({:?})", filepath, err)
    });

    let mut scanner = Scanner::new(filepath);
    scanner.scan_file(&syntax_tree);

    // for debugging
    // println!("Final scanner state: {:?}", scanner);

    scanner.get_results()
}

/// Scan the supplied crate
pub fn scan_crate(crate_path: &Path) -> Result<ScanResults> {
    // Make sure the path is a crate
    if !crate_path.is_dir() {
        return Err(anyhow!("Path is not a crate; not a directory"));
    }

    let mut cargo_toml_path = crate_path.to_path_buf();
    cargo_toml_path.push("Cargo.toml");
    if !cargo_toml_path.try_exists()? || !cargo_toml_path.is_file() {
        return Err(anyhow!("Path is not a crate; missing Cargo.toml"));
    }

    let mut final_result = ScanResults::new();

    // TODO: For now, only walking through the src dir, but might want to
    //       include others (e.g. might codegen in other dirs)
    // We have a valid crate, so iterate through all the rust src
    for entry in
        WalkDir::new(crate_path.join(Path::new("src".into()))).into_iter().filter(|e| {
            match e {
                Ok(ne) if ne.path().is_file() => {
                    let fname = Path::new(ne.file_name());
                    fname.to_str().map_or(false, |x| x.ends_with(".rs"))
                }
                _ => false,
            }
        })
    {
        let mut next_scan = load_and_scan(Path::new(entry?.path()));

        // TODO: Remove this once pattern setting makes sense. It's this ugly
        //       optional initialization function right now.
        for e in &mut next_scan.effects {
            sink::set_pattern(e)
        }

        final_result.combine_results(&mut next_scan);
    }

    Ok(final_result)
}
