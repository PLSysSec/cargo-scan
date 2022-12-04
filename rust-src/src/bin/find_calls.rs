/*
Parse a Rust source file and find all function calls, printing them to stdout
(one per line).
*/

use cargo_scan::effect::Effect;
use clap::Parser;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    filename: PathBuf,
}

#[allow(dead_code)]
#[derive(Debug)]
struct CallSite {
    caller_iden: String, // my_read_wrapper_fun
    callee_iden: String, // read
    callee_path: String, // std::fs::read
}
impl CallSite {
    #[allow(dead_code)]
    fn as_effect(&self) -> Effect {
        todo!()
    }
}

/// Stateful object to scan Rust source code for effects (fn calls of interest)
#[derive(Debug)]
struct Scanner<'a> {
    results: Vec<CallSite>,
    // stack-based scopes for parsing (always empty at top-level)
    scope_mods: Vec<&'a syn::Ident>,
    scope_use: Vec<&'a syn::Ident>,
    scope_fun: Option<&'a syn::Ident>,
    // collecting use statements that are in scope
    use_names: HashMap<String, String>,
    use_globs: Vec<String>,
}

impl<'a> Scanner<'a> {
    /*
        Top-level items and modules
    */
    fn new() -> Self {
        let results = Vec::new();
        let scope_mods = Vec::new();
        let scope_use = Vec::new();
        let scope_fun = None;
        let use_names = HashMap::new();
        let use_globs = Vec::new();
        Self {
            results,
            scope_mods,
            scope_use,
            scope_fun,
            use_names,
            use_globs,
        }
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
            syn::Item::Fn(fun) => self.scan_fn(fun),
            _ => (),
            // TODO:
            // syn::Item::Impl(i) => ...
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
    fn scope_use_as_string(&self) -> String {
        let v: Vec<String> = self
            .scope_use
            .iter()
            .map(|ident| ident.to_string())
            .collect();
        v.join("::")
    }
    fn save_scope_use_under(&mut self, lookup_key: &'a syn::Ident) {
        // save the use scope under an identifier/lookup key
        // TBD: maybe warn if there is a name conflict
        self.use_names
            .insert(lookup_key.to_string(), self.scope_use_as_string());
    }
    fn scan_use(&mut self, u: &'a syn::ItemUse) {
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
        self.use_globs.push(self.scope_use_as_string());
    }
    fn scan_use_group(&mut self, g: &'a syn::UseGroup) {
        for t in g.items.iter() {
            self.scan_use_tree(t);
        }
    }

    /*
        Function declarations
    */
    fn scan_fn(&mut self, f: &'a syn::ItemFn) {
        let f_name = &f.sig.ident;
        if let Some(existing_name) = self.scope_fun {
            eprintln!(
                "warning: found function {} when already in function {}",
                f_name, existing_name
            );
        }
        self.scope_fun = Some(f_name);
        for s in &f.block.stmts {
            self.scan_fn_statement(s);
        }
        self.scope_fun = None;
    }
    fn scan_fn_statement(&mut self, s: &'a syn::Stmt) {
        match s {
            syn::Stmt::Local(l) => self.scan_fn_local(l),
            syn::Stmt::Expr(e) => self.scan_expr(e),
            syn::Stmt::Semi(e, _) => self.scan_expr(e),
            syn::Stmt::Item(_) => eprintln!(
                "warning: ignoring item within function block {:?}",
                self.scope_fun
            ),
        }
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
                // Note that there is an inherent incompleteness in this case
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
            syn::Expr::Verbatim(_) => {
                eprintln!("warning: encountered Verbatim expression (skipping)");
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
    fn scan_expr_call_args(
        &mut self,
        a: &'a syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    ) {
        for y in a.iter() {
            self.scan_expr(y);
        }
    }
    fn lookup_ident(&self, i: &'a syn::Ident) -> String {
        let s = i.to_string();
        self.use_names.get(&s).cloned().unwrap_or(s)
    }
    fn push_callsite(&mut self, callee_iden: String, callee_path: String) {
        let caller_iden = self
            .scope_fun
            .expect("scan_expr_call_ident called outside of a function!")
            .to_string();
        self.results.push(CallSite {
            caller_iden,
            callee_iden,
            callee_path,
        })
    }
    fn scan_expr_call(&mut self, f: &'a syn::Expr) {
        match f {
            syn::Expr::Path(p) => {
                let mut it = p.path.segments.iter().map(|seg| &seg.ident);
                let fst = it.next().unwrap();
                let mut callee_path = self.lookup_ident(fst);
                let mut callee_iden = fst.to_string();
                for id in it {
                    callee_path.push_str("::");
                    callee_iden = id.to_string(); // overwrite
                    callee_path.push_str(&callee_iden);
                }
                self.push_callsite(callee_iden, callee_path);
            }
            _ => {
                eprintln!("encountered unexpected function call which was not a path expression; ignoring")
            }
        }
    }
    fn scan_expr_call_ident(&mut self, i: &'a syn::Ident) {
        let callee_iden = i.to_string();
        let callee_path = format!("[METHOD]::{}", i);
        self.push_callsite(callee_iden, callee_path);
    }
}

fn main() {
    let args = Args::parse();

    // based on example at https://docs.rs/syn/latest/syn/struct.File.html
    let mut file = File::open(&args.filename).expect("Unable to open file");
    let mut src = String::new();
    file.read_to_string(&mut src).expect("Unable to read file");

    let syntax_tree = syn::parse_file(&src).expect("Unable to parse file");
    let mut scanner = Scanner::new();
    scanner.scan_file(&syntax_tree);

    // for debugging
    // println!("Final scanner state: {:?}", scanner);

    for result in scanner.results {
        println!("{:?}", result);
    }
}
