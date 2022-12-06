/*
Parse a Rust source file and find all function calls, printing them to stdout
(one per line).
*/

use cargo_scan::effect::Effect;
use clap::Parser;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    // ../path/to/my_rust_crate/src/my_mod/my_file.rs
    filepath: PathBuf,
}

/// Stateful object to scan Rust source code for effects (fn calls of interest)
#[derive(Debug)]
struct Scanner<'a> {
    // filepath that the scanner is being run on
    filepath: &'a Path,
    // output
    results: Vec<Effect>,
    // stack-based scopes for parsing (always empty at top-level)
    scope_mods: Vec<&'a syn::Ident>,
    scope_use: Vec<&'a syn::Ident>,
    scope_fun: Vec<&'a syn::Ident>,
    // collecting use statements that are in scope
    use_names: HashMap<String, Vec<&'a syn::Ident>>,
    use_globs: Vec<Vec<&'a syn::Ident>>,
}

impl<'a> Scanner<'a> {
    /*
        Top-level items and modules
    */
    fn new(filepath: &'a Path) -> Self {
        let results = Vec::new();
        let scope_mods = Vec::new();
        let scope_use = Vec::new();
        let scope_fun = Vec::new();
        let use_names = HashMap::new();
        let use_globs = Vec::new();
        Self { filepath, results, scope_mods, scope_use, scope_fun, use_names, use_globs }
    }
    fn scan_file(&mut self, f: &'a syn::File) {
        // scan the file and return a list of all calls in it
        for i in &f.items {
            self.scan_item(i);
        }
    }
    fn scan_item(&mut self, i: &'a syn::Item) {
        match i {
            syn::Item::Mod(m) => self.scan_mod(m),
            syn::Item::Use(u) => self.scan_use(u),
            syn::Item::Impl(imp) => self.scan_impl(imp),
            syn::Item::Fn(fun) => self.scan_fn(fun),
            _ => (),
            // For all syntax elements see
            // https://docs.rs/syn/latest/syn/enum.Item.html
            // Potentially interesting:
            // Trait(t) -- default impls are an issue here
            // ForeignMod(ItemForeignMod) -- extern items like extern "C" { ... }
            // Macro, Macro2 -- macro invocations and declarations
            // Const(ItemConst), Static(ItemStatic) -- for information flow
        }
    }
    fn scan_mod(&mut self, m: &'a syn::ItemMod) {
        if let Some(existing_name) = self.scope_fun.last() {
            eprintln!(
                "warning: found module {:?} when already in function {}",
                m, existing_name
            );
        }
        // TBD: reset use state; handle super keywords
        if let Some((_, items)) = &m.content {
            self.scope_mods.push(&m.ident);
            for i in items {
                self.scan_item(i);
            }
            self.scope_mods.pop();
        }
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
        let fst: &'a syn::Ident = it.next().unwrap();
        // first part of the path based on lookup
        // TBD use extend
        for i in self.lookup_ident(&fst).iter() {
            let i: &'a syn::Ident = i;
            result.push(i);
        }
        // second part of the path based on any additional sub-scoping
        for i in it {
            result.push(i);
        }

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
        // TBD: trait impls should be scoped under trait name, not type name:
        // if let Some((_, path, _)) = &imp.trait_ ...
        //     self.scan_impl_type(&imp.self_ty);
        let scope_adds = self.scan_impl_type(&imp.self_ty);

        // scan the impl block
        for item in &imp.items {
            match item {
                syn::ImplItem::Method(m) => {
                    self.scan_method(m);
                }
                syn::ImplItem::Macro(_) => {
                    // incompleteness
                }
                syn::ImplItem::Verbatim(v) => {
                    eprintln!("warning: skipping Verbatim expression: {:?}", v);
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
                // incompleteness
                0
            }
            syn::Type::Verbatim(v) => {
                eprintln!("warning: skipping Verbatim expression: {:?}", v);
                0
            }
            _ => {
                eprintln!("warning: unexpected impl block type (ignoring): {:?}", ty);
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
    fn scan_impl_type_path(&mut self, p: &'a syn::Path) -> usize {
        let mut count = 0;
        for seg in p.segments.iter() {
            self.scope_mods.push(&seg.ident);
            count += 1;
        }
        if count == 0 {
            eprintln!("warning: unexpected empty impl type path: {:?}", p)
        } else if count > 1 {
            eprintln!("warning: found :: in impl type path; not sure if this case is handled correctly: {:?}", p)
        }
        count
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
                eprintln!("warning: encountered box expression (unstable feature)");
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
                // TODO: closures are a bit weird!
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
                // Note inherent incompleteness in this case
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
                self.scan_expr_call_ident(&x.method);
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
                eprintln!("warning: encountered try block (unstable feature)");
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
                eprintln!("warning: skipping Verbatim expression: {:?}", v);
            }
            syn::Expr::While(x) => {
                self.scan_expr(&x.cond);
                for s in &x.body.stmts {
                    self.scan_fn_statement(s);
                }
            }
            syn::Expr::Yield(x) => {
                eprintln!("warning: encountered yield expression (unstable feature)");
                if let Some(y) = &x.expr {
                    self.scan_expr(y);
                }
            }
            _ => {
                eprintln!("warning: encountered unknown expression")
            }
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
    fn push_callsite(&mut self, callee_ident: &'a syn::Ident, callee_path: String) {
        // push an Effect to the list of results based on this call site.

        // caller
        let caller_name = self
            .scope_fun
            .last()
            .expect("scan_expr_call_ident called outside of a function!")
            .to_string();

        let mod_scope = self.get_mod_scope();

        // callee
        let call_line = callee_ident.span().start().line;
        let call_col = callee_ident.span().start().column;
        // TBD: callee_name could also be useful
        // let callee_name = callee_ident.to_string();

        self.results.push(Effect::new(
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
            syn::Expr::Macro(_) => {
                // Note inherent incompleteness in this case
            }
            _ => {
                let line = f.span().start().line;
                let col = f.span().start().column;
                eprintln!(
                    "encountered unexpected function call expression: {:?} ({}:{})",
                    f, line, col
                )
            }
        }
    }
    fn scan_expr_call_ident(&mut self, i: &'a syn::Ident) {
        let callee_path = format!("[METHOD]::{}", i);
        self.push_callsite(i, callee_path);
    }
}

fn main() {
    let args = Args::parse();

    // based on example at https://docs.rs/syn/latest/syn/struct.File.html
    let mut file = File::open(&args.filepath).unwrap_or_else(|err| {
        panic!(
            "find_calls.rs: Error: Unable to open file: {:?} ({:?})",
            args.filepath, err
        )
    });
    let mut src = String::new();
    file.read_to_string(&mut src).unwrap_or_else(|err| {
        panic!(
            "find_calls.rs: Error: Unable to read file: {:?} ({:?})",
            args.filepath, err
        )
    });

    let syntax_tree = syn::parse_file(&src).unwrap_or_else(|err| {
        panic!(
            "find_calls.rs: Error: Unable to parse file: {:?} ({:?})",
            args.filepath, err
        )
    });
    let mut scanner = Scanner::new(&args.filepath);
    scanner.scan_file(&syntax_tree);

    // for debugging
    // println!("Final scanner state: {:?}", scanner);

    for result in scanner.results {
        println!("{}", result.to_csv());
    }
}
