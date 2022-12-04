/*
Parse a Rust source file and find all function calls, printing them to stdout
(one per line).
*/

use cargo_scan::effect::Effect;
use clap::Parser;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    filename: String,
}

/// Stateful object to scan Rust source code for effects (fn calls of interest)
#[derive(Debug)]
struct Scanner<'a> {
    results: Vec<Effect>,
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
        Self { results, scope_mods, scope_use, scope_fun, use_names, use_globs }
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
        let v: Vec<String> = self.scope_use.iter().map(|ident| ident.to_string()).collect();
        v.join("::")
    }
    fn save_scope_use_under(&mut self, lookup_key: &'a syn::Ident) {
        // save the use scope under an identifier/lookup key
        // TBD: maybe warn if there is a name conflict
        self.use_names.insert(lookup_key.to_string(), self.scope_use_as_string());
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
            eprintln!("warning: found function {} when already in function {}", f_name, existing_name);
        }
        for s in &f.block.stmts {
            self.scan_fn_statement(s);
        }
    }
    fn scan_fn_statement(&mut self, s: &'a syn::Stmt) {
        match s {
            syn::Stmt::Local(l) => self.scan_fn_local(l),
            syn::Stmt::Expr(e) => self.scan_fn_expr(e),
            syn::Stmt::Semi(e, _) => self.scan_fn_expr(e),
            syn::Stmt::Item(_) => eprintln!("warning: ignoring item within function block {:?}", self.scope_fun),
        }
    }
    fn scan_fn_local(&mut self, l: &'a syn::Local) {
        if let Some((_, b)) = &l.init {
            self.scan_fn_expr(b)
        }
    }
    fn scan_fn_expr(&mut self, _e: &'a syn::Expr) {
        // TODO: 40 cases
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

    println!("Final scanner state: {:?}", scanner);

    for effect in scanner.results {
        println!("{}", effect.to_csv());
    }
}
