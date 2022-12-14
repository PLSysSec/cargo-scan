/*
    Scanner to parse a Rust source file and find all function call locations.
*/

use super::effect::Effect;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use syn::spanned::Spanned;

/// Stateful object to scan Rust source code for effects (fn calls of interest)
#[derive(Debug)]
pub struct Scanner<'a> {
    // filepath that the scanner is being run on
    filepath: &'a Path,
    // output
    effects: Vec<Effect>,
    // stack-based scopes for parsing (always empty at top-level)
    // TBD: can probably combine all types of scope into one
    scope_mods: Vec<&'a syn::Ident>,
    scope_use: Vec<&'a syn::Ident>,
    scope_fun: Vec<&'a syn::Ident>,
    // collecting use statements that are in scope
    use_names: HashMap<String, Vec<&'a syn::Ident>>,
    use_globs: Vec<Vec<&'a syn::Ident>>,
    // info about skipped nodes
    skipped_macros: usize,
    skipped_fn_calls: usize,
}

/// Results of a scan
pub struct ScanResults {
    pub effects: Vec<Effect>,
    pub skipped_macros: usize,
    pub skipped_fn_calls: usize,
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
            scope_mods: Vec::new(),
            scope_use: Vec::new(),
            scope_fun: Vec::new(),
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
                for s in &x.block.stmts {
                    self.scan_fn_statement(s);
                }
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
    fn scan_expr_call(&mut self, f: &'a syn::Expr) {
        match f {
            syn::Expr::Path(p) => {
                let callee_path = self.lookup_path(&p.path);
                let callee_ident = callee_path.last().unwrap();
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
